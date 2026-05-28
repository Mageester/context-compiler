use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The persistent index stored in .ctx/index.db
pub struct Store {
    conn: Connection,
    path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileEntry {
    pub path: String,
    pub summary: String,
    pub token_count: usize,
    pub language: String,
    pub tree_hash: String,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImportEdge {
    pub from_path: String,
    pub to_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub id: String,
    pub task: String,
    pub task_embedding: Vec<f32>,
    pub file_paths: Vec<String>,
    pub created_at: String,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let db_path = path.join(".ctx/index.db");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&db_path)?;
        let store = Store {
            conn,
            path: path.to_path_buf(),
        };
        store.initialize_schema()?;
        Ok(store)
    }

    fn initialize_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                summary TEXT NOT NULL,
                token_count INTEGER NOT NULL DEFAULT 0,
                language TEXT NOT NULL DEFAULT '',
                tree_hash TEXT NOT NULL DEFAULT '',
                embedding BLOB,
                indexed_at INTEGER NOT NULL DEFAULT (unixepoch())
            );

            CREATE TABLE IF NOT EXISTS imports (
                from_path TEXT NOT NULL,
                to_path TEXT NOT NULL,
                PRIMARY KEY (from_path, to_path)
            );

            CREATE TABLE IF NOT EXISTS history (
                id TEXT PRIMARY KEY,
                task TEXT NOT NULL,
                task_embedding BLOB,
                file_paths TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_imports_from ON imports(from_path);
            CREATE INDEX IF NOT EXISTS idx_imports_to ON imports(to_path);

            CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(
                path, summary, content='files', content_rowid='rowid'
            );
            ",
        )?;
        Ok(())
    }

    pub fn upsert_file(&self, entry: &FileEntry) -> Result<()> {
        let embedding_blob = entry
            .embedding
            .as_ref()
            .map(|v| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>());
        self.conn.execute(
            "INSERT OR REPLACE INTO files (path, summary, token_count, language, tree_hash, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                entry.path,
                entry.summary,
                entry.token_count,
                entry.language,
                entry.tree_hash,
                embedding_blob,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_import(&self, edge: &ImportEdge) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO imports (from_path, to_path) VALUES (?1, ?2)",
            rusqlite::params![edge.from_path, edge.to_path],
        )?;
        Ok(())
    }

    pub fn get_all_files(&self) -> Result<Vec<FileEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, summary, token_count, language, tree_hash, embedding FROM files",
        )?;
        let entries = stmt
            .query_map([], |row| {
                let embedding_blob: Option<Vec<u8>> = row.get(5)?;
                let embedding = embedding_blob.map(|blob| {
                    blob.chunks_exact(4)
                        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                        .collect()
                });
                Ok(FileEntry {
                    path: row.get(0)?,
                    summary: row.get(1)?,
                    token_count: row.get(2)?,
                    language: row.get(3)?,
                    tree_hash: row.get(4)?,
                    embedding,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    pub fn get_imports(&self) -> Result<Vec<ImportEdge>> {
        let mut stmt = self
            .conn
            .prepare("SELECT from_path, to_path FROM imports")?;
        let edges = stmt
            .query_map([], |row| {
                Ok(ImportEdge {
                    from_path: row.get(0)?,
                    to_path: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(edges)
    }

    pub fn get_imports_for_file(&self, file_path: &str) -> Result<Vec<String>> {
        let stmt = self
            .conn
            .prepare("SELECT to_path FROM imports WHERE from_path = ?1")?
            .query_map(rusqlite::params![file_path], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        // Manually collect to avoid type inference issues
        let paths: Vec<String> = stmt;
        Ok(paths)
    }

    pub fn add_history(&self, entry: &HistoryEntry) -> Result<()> {
        let embedding_blob: Vec<u8> = entry
            .task_embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let file_paths_json = serde_json::to_string(&entry.file_paths)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO history (id, task, task_embedding, file_paths, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                entry.id,
                entry.task,
                embedding_blob,
                file_paths_json,
                entry.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_history(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, task, task_embedding, file_paths, created_at FROM history ORDER BY created_at DESC LIMIT ?1",
        )?;
        let entries = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                let embedding_blob: Vec<u8> = row.get(2)?;
                let task_embedding = embedding_blob
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                    .collect();
                let file_paths: String = row.get(3)?;
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    task: row.get(1)?,
                    task_embedding,
                    file_paths: serde_json::from_str(&file_paths).unwrap_or_default(),
                    created_at: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    pub fn file_count(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    pub fn clear(&self) -> Result<()> {
        self.conn
            .execute_batch("DELETE FROM files; DELETE FROM imports; DELETE FROM files_fts;")?;
        Ok(())
    }

    pub fn remove_file(&self, path: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM files WHERE path = ?1", rusqlite::params![path])?;
        self.conn.execute(
            "DELETE FROM imports WHERE from_path = ?1 OR to_path = ?1",
            rusqlite::params![path],
        )?;
        Ok(())
    }
}

pub fn ctx_dir(path: &Path) -> PathBuf {
    path.join(".ctx")
}

pub fn ctx_db(path: &Path) -> PathBuf {
    ctx_dir(path).join("index.db")
}

pub fn ctx_exists(path: &Path) -> bool {
    ctx_db(path).exists()
}

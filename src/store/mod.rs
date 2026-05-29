use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The persistent index stored in .ctx/index.db
pub struct Store {
    conn: Connection,
    #[allow(dead_code)]
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
    pub identifiers: Vec<String>,
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
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        let store = Store {
            conn,
            path: path.to_path_buf(),
        };
        store.initialize_schema()?;
        Ok(store)
    }

    fn initialize_schema(&self) -> Result<()> {
        // Use a migration-friendly approach: create tables with IF NOT EXISTS,
        // then try to add columns that might be missing from older schemas.
        self.conn.execute_batch(
            "
            -- Main files table
            CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                summary TEXT NOT NULL DEFAULT '',
                token_count INTEGER NOT NULL DEFAULT 0,
                language TEXT NOT NULL DEFAULT '',
                tree_hash TEXT NOT NULL DEFAULT '',
                embedding BLOB,
                identifiers TEXT NOT NULL DEFAULT '',
                indexed_at INTEGER NOT NULL DEFAULT (unixepoch())
            );

            -- Imports table
            CREATE TABLE IF NOT EXISTS imports (
                from_path TEXT NOT NULL,
                to_path TEXT NOT NULL,
                PRIMARY KEY (from_path, to_path)
            );

            -- History table
            CREATE TABLE IF NOT EXISTS history (
                id TEXT PRIMARY KEY,
                task TEXT NOT NULL,
                task_embedding BLOB,
                file_paths TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- FTS5 virtual table for full-text search
            -- Stores path, summary, and identifiers together for BM25 scoring
            CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(
                path, summary, identifiers,
                content='files', content_rowid='rowid',
                tokenize='ascii'
            );

            -- Triggers to keep FTS in sync with files table
            CREATE TRIGGER IF NOT EXISTS files_ai AFTER INSERT ON files BEGIN
                INSERT INTO files_fts(rowid, path, summary, identifiers)
                VALUES (new.rowid, new.path, new.summary, new.identifiers);
            END;

            CREATE TRIGGER IF NOT EXISTS files_ad AFTER DELETE ON files BEGIN
                INSERT INTO files_fts(files_fts, rowid, path, summary, identifiers)
                VALUES ('delete', old.rowid, old.path, old.summary, old.identifiers);
            END;

            CREATE TRIGGER IF NOT EXISTS files_au AFTER UPDATE ON files BEGIN
                INSERT INTO files_fts(files_fts, rowid, path, summary, identifiers)
                VALUES ('delete', old.rowid, old.path, old.summary, old.identifiers);
                INSERT INTO files_fts(rowid, path, summary, identifiers)
                VALUES (new.rowid, new.path, new.summary, new.identifiers);
            END;
            ",
        )?;

        // Try to add identifiers column if it doesn't exist (schema migration)
        let _ = self.conn.execute_batch(
            "ALTER TABLE files ADD COLUMN identifiers TEXT NOT NULL DEFAULT '';"
        );

        Ok(())
    }

    pub fn upsert_file(&self, entry: &FileEntry) -> Result<()> {
        let embedding_blob = entry
            .embedding
            .as_ref()
            .map(|v| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>());
        let identifiers_json = serde_json::to_string(&entry.identifiers)?;

        // Use INSERT ... ON CONFLICT to handle both insert and update
        self.conn.execute(
            "INSERT INTO files (path, summary, token_count, language, tree_hash, embedding, identifiers)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(path) DO UPDATE SET
                summary = excluded.summary,
                token_count = excluded.token_count,
                language = excluded.language,
                tree_hash = excluded.tree_hash,
                embedding = excluded.embedding,
                identifiers = excluded.identifiers,
                indexed_at = unixepoch()",
            rusqlite::params![
                entry.path,
                entry.summary,
                entry.token_count,
                entry.language,
                entry.tree_hash,
                embedding_blob,
                identifiers_json,
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
            "SELECT path, summary, token_count, language, tree_hash, embedding, identifiers FROM files",
        )?;
        let entries = stmt
            .query_map([], |row| {
                let embedding_blob: Option<Vec<u8>> = row.get(5)?;
                let embedding = embedding_blob.map(|blob| {
                    blob.chunks_exact(4)
                        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                        .collect()
                });
                let identifiers_str: String = row.get::<_, String>(6).unwrap_or_default();
                let identifiers: Vec<String> =
                    serde_json::from_str(&identifiers_str).unwrap_or_default();

                Ok(FileEntry {
                    path: row.get(0)?,
                    summary: row.get(1)?,
                    token_count: row.get(2)?,
                    language: row.get(3)?,
                    tree_hash: row.get(4)?,
                    embedding,
                    identifiers,
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
        let paths: Vec<String> = stmt;
        Ok(paths)
    }

    /// Search the FTS5 index with BM25 scoring.
    /// Returns a map of file_path -> BM25 score (0.0 to 1.0 normalized).
    pub fn search_fts(&self, query: &str, limit: usize) -> Result<HashMap<String, f32>> {
        // Clean the query for FTS5: use AND for multi-word, wrap phrases
        let fts_query = Self::to_fts_query(query);
        if fts_query.is_empty() {
            return Ok(HashMap::new());
        }

        // Get the max BM25 score across all results for normalization
        let mut stmt = self.conn.prepare(
            "SELECT files.path, bm25(files_fts, 0, 1.0, 1.0, 0.5, 1.0, 1.0) AS score
             FROM files_fts
             JOIN files ON files.rowid = files_fts.rowid
             WHERE files_fts MATCH ?1
             ORDER BY score
             LIMIT ?2"
        )?;

        // BM25 returns lower = better. Normalize to 0-1 where 1 = best match.
        let results: Vec<(String, f64)> = stmt
            .query_map(rusqlite::params![fts_query, limit as i64], |row| {
                let path: String = row.get(0)?;
                let score: f64 = row.get(1)?;
                Ok((path, score))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .collect();

        if results.is_empty() {
            return Ok(HashMap::new());
        }

        // BM25: lower is better. Normalize so best score = 1.0, worst = 0.0.
        // BM25 is typically 0 to ~10, lower = better.
        // We use: score_normalized = 1.0 - (score / max_possible)
        // Or better: 1.0 / (1.0 + score) gives a 0-1 range where 1 = perfect match.
        let scores: HashMap<String, f32> = results
            .into_iter()
            .map(|(path, raw_score)| {
                // Convert BM25 to 0-1 where higher = better
                // BM25 minimum is 0 (perfect match), typically goes up to 5-10
                let normalized = (1.0 / (1.0 + raw_score)).min(1.0);
                (path, normalized as f32)
            })
            .collect();

        Ok(scores)
    }

    /// Convert a user query to an FTS5-safe query.
    fn to_fts_query(query: &str) -> String {
        // Split by whitespace, filter short/noise words, join with AND
        let terms: Vec<&str> = query
            .split_whitespace()
            .filter(|w| {
                w.len() > 1
                    && ![
                        "the", "and", "for", "are", "but", "not", "you", "all", "can", "had",
                        "her", "was", "one", "our", "out", "has", "get", "its", "how", "why",
                        "use", "set", "add", "fix", "new", "bug", "to", "in", "it", "of", "is",
                        "on", "be", "at", "an", "by", "we", "or", "as", "if", "do", "no", "so",
                        "up",
                    ]
                    .contains(w)
            })
            .collect();

        if terms.is_empty() {
            return String::new();
        }

        // Use OR for broader matching. FTS5 BM25 will rank files that match
        // more terms higher, so this works better than AND for code search.
        // Wrap compound terms in quotes for exact matching.
        let parts: Vec<String> = terms
            .iter()
            .map(|t| {
                if t.contains('.') || t.contains('/') || t.contains('_') || t.contains('-') {
                    format!("\"{}\"", t)
                } else {
                    t.to_string()
                }
            })
            .collect();

        parts.join(" OR ")
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
        // FTS is synced via trigger
        Ok(())
    }
}

#[allow(dead_code)]
pub fn ctx_dir(path: &Path) -> PathBuf {
    path.join(".ctx")
}

#[allow(dead_code)]
pub fn ctx_db(path: &Path) -> PathBuf {
    ctx_dir(path).join("index.db")
}

#[allow(dead_code)]
pub fn ctx_exists(path: &Path) -> bool {
    ctx_db(path).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_temp_store() -> (Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        (store, dir)
    }

    fn sample_embedding() -> Vec<f32> {
        vec![0.1; 384]
    }

    #[test]
    fn test_open_creates_dir_and_db() {
        let dir = tempfile::tempdir().unwrap();
        let _store = Store::open(dir.path()).unwrap();
        assert!(dir.path().join(".ctx/index.db").exists());
    }

    #[test]
    fn test_upsert_and_get_all_files() {
        let (store, _dir) = setup_temp_store();
        let file = FileEntry {
            path: "src/main.rs".into(),
            summary: "Main entry point".into(),
            token_count: 100,
            language: "rust".into(),
            tree_hash: "abc123".into(),
            embedding: Some(sample_embedding()),
            identifiers: vec!["main".into(), "run".into()],
        };
        store.upsert_file(&file).unwrap();
        let files = store.get_all_files().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/main.rs");
    }

    #[test]
    fn test_upsert_replace_same_path() {
        let (store, _dir) = setup_temp_store();
        let f1 = FileEntry {
            path: "lib.rs".into(),
            summary: "old".into(),
            token_count: 50,
            language: "rust".into(),
            tree_hash: "old".into(),
            embedding: None,
            identifiers: vec![],
        };
        let f2 = FileEntry {
            path: "lib.rs".into(),
            summary: "new".into(),
            token_count: 200,
            language: "rust".into(),
            tree_hash: "new".into(),
            embedding: None,
            identifiers: vec![],
        };
        store.upsert_file(&f1).unwrap();
        store.upsert_file(&f2).unwrap();
        let files = store.get_all_files().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].summary, "new");
    }

    #[test]
    fn test_file_count() {
        let (store, _dir) = setup_temp_store();
        assert_eq!(store.file_count().unwrap(), 0);
        store
            .upsert_file(&FileEntry {
                path: "a.rs".into(),
                summary: "".into(),
                token_count: 0,
                language: "rust".into(),
                tree_hash: "".into(),
                embedding: None,
                identifiers: vec![],
            })
            .unwrap();
        assert_eq!(store.file_count().unwrap(), 1);
    }

    #[test]
    fn test_upsert_and_get_imports() {
        let (store, _dir) = setup_temp_store();
        let edge = ImportEdge {
            from_path: "a.rs".into(),
            to_path: "b.rs".into(),
        };
        store.upsert_import(&edge).unwrap();
        let edges = store.get_imports().unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from_path, "a.rs");
    }

    #[test]
    fn test_import_dedup() {
        let (store, _dir) = setup_temp_store();
        let edge = ImportEdge {
            from_path: "a.rs".into(),
            to_path: "b.rs".into(),
        };
        store.upsert_import(&edge).unwrap();
        store.upsert_import(&edge).unwrap();
        assert_eq!(store.get_imports().unwrap().len(), 1);
    }

    #[test]
    fn test_history_roundtrip() {
        let (store, _dir) = setup_temp_store();
        let emb: Vec<f32> = vec![0.5; 384];
        let entry = HistoryEntry {
            id: "test-1".into(),
            task: "fix bug".into(),
            task_embedding: emb.clone(),
            file_paths: vec!["src/main.rs".into()],
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        store.add_history(&entry).unwrap();
        let hist = store.get_history(10).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].task, "fix bug");
        assert_eq!(hist[0].file_paths, vec!["src/main.rs"]);
        assert_eq!(hist[0].task_embedding.len(), 384);
    }

    #[test]
    fn test_history_limit() {
        let (store, _dir) = setup_temp_store();
        let emb: Vec<f32> = vec![0.0; 384];
        for i in 0..5 {
            store
                .add_history(&HistoryEntry {
                    id: format!("h-{}", i),
                    task: format!("task {}", i),
                    task_embedding: emb.clone(),
                    file_paths: vec![],
                    created_at: format!("2026-01-{:02}T00:00:00Z", i + 1),
                })
                .unwrap();
        }
        assert_eq!(store.get_history(3).unwrap().len(), 3);
    }

    #[test]
    fn test_fts_search_finds_by_path() {
        let (store, _dir) = setup_temp_store();
        store
            .upsert_file(&FileEntry {
                path: "src/App.tsx".into(),
                summary: "Main application component".into(),
                token_count: 200,
                language: "typescript".into(),
                tree_hash: "abc".into(),
                embedding: None,
                identifiers: vec!["App".into(), "Application".into()],
            })
            .unwrap();
        store
            .upsert_file(&FileEntry {
                path: "src/lib/repos.ts".into(),
                summary: "Repository management utilities".into(),
                token_count: 150,
                language: "typescript".into(),
                tree_hash: "def".into(),
                embedding: None,
                identifiers: vec!["repos".into(), "Repository".into()],
            })
            .unwrap();

        let results = store.search_fts("repos.ts", 10).unwrap();
        assert!(
            results.contains_key("src/lib/repos.ts"),
            "FTS should find repos.ts: {:?}",
            results
        );
    }

    #[test]
    fn test_fts_search_finds_by_summary() {
        let (store, _dir) = setup_temp_store();
        store
            .upsert_file(&FileEntry {
                path: "src/auth/middleware.ts".into(),
                summary: "Authentication middleware for API routes".into(),
                token_count: 300,
                language: "typescript".into(),
                tree_hash: "ghi".into(),
                embedding: None,
                identifiers: vec!["auth".into(), "middleware".into()],
            })
            .unwrap();

        let results = store.search_fts("authentication middleware", 10).unwrap();
        assert!(
            results.contains_key("src/auth/middleware.ts"),
            "FTS should find auth middleware: {:?}",
            results
        );
    }

    #[test]
    fn test_fts_search_returns_scores() {
        let (store, _dir) = setup_temp_store();
        store
            .upsert_file(&FileEntry {
                path: "src/payments/webhook.ts".into(),
                summary: "Handles incoming payment webhook events".into(),
                token_count: 100,
                language: "typescript".into(),
                tree_hash: "jkl".into(),
                embedding: None,
                identifiers: vec!["payments".into(), "webhook".into()],
            })
            .unwrap();

        let results = store.search_fts("payment webhook", 10).unwrap();
        assert!(results.contains_key("src/payments/webhook.ts"));
        let score = results["src/payments/webhook.ts"];
        assert!(
            score > 0.0 && score <= 1.0,
            "BM25 score should be normalized 0-1, got {}",
            score
        );
    }
}

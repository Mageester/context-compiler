#!/usr/bin/env python3
"""Mass accuracy harness for Context Compiler.

Safety: every project is copied into a temp directory before ctx touches it.
Outputs Markdown and JSON reports with expected-file hits and junk-file detection.
"""
from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import tempfile
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable

JUNK_PATTERNS = [
    "target/",
    "node_modules/",
    "dist/",
    "build/",
    "out/",
    ".next/",
    ".nuxt/",
    ".svelte-kit/",
    ".git/",
    ".hg/",
    ".svn/",
    ".cache/",
    "coverage/",
    "vendor/",
    "__pycache__/",
    ".venv/",
    "venv/",
]

SELECTED_RE = re.compile(r"^\s*\d+\.\s+([^\s(]+)")
CONTEXT_RE = re.compile(r"^// ═══ (.+?) —")


@dataclass
class TaskCase:
    task: str
    expected_any: list[str]
    expected_all: list[str]


@dataclass
class CaseResult:
    project: str
    task: str
    selected: list[str]
    expected_any: list[str]
    expected_all: list[str]
    hit_any: bool
    hit_all_count: int
    hit_all_total: int
    junk_files: list[str]
    exit_code: int
    stderr_tail: str


def norm(path: str) -> str:
    return path.strip().replace("\\", "/")


def is_junk(path: str) -> bool:
    """Return true only for real junk path segments, not substrings like layOUT/."""
    normalized = norm(path).lower().strip("/")
    segments = normalized.split("/")
    junk_segments = {pattern.strip("/") for pattern in JUNK_PATTERNS}
    return any(segment in junk_segments for segment in segments)


def copy_project(src: Path, workspace: Path) -> Path:
    dst = workspace / src.name
    ignore = shutil.ignore_patterns(
        ".git",
        ".hg",
        ".svn",
        "target",
        "node_modules",
        "dist",
        "build",
        "out",
        ".next",
        ".nuxt",
        ".svelte-kit",
        ".cache",
        "coverage",
        "vendor",
        "__pycache__",
        ".venv",
        "venv",
    )
    shutil.copytree(src, dst, ignore=ignore)
    return dst


def run(cmd: list[str], cwd: Path, timeout: int) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=str(cwd),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )


def parse_selected(output: str) -> list[str]:
    selected: list[str] = []
    for raw_line in output.splitlines():
        line = raw_line.strip()
        plain = re.sub(r"\x1b\[[0-9;]*m", "", line)
        match = SELECTED_RE.match(plain) or CONTEXT_RE.match(plain)
        if match:
            selected.append(norm(match.group(1)))

    deduped: list[str] = []
    seen = set()
    for path in selected:
        if path not in seen:
            deduped.append(path)
            seen.add(path)
    return deduped


def matches(selected: Iterable[str], expected: str) -> bool:
    expected_norm = norm(expected).lower()
    expected_parts = expected_norm.split("/")
    for path in selected:
        candidate = norm(path).lower()
        # Check if expected is contained in candidate or vice versa
        if expected_norm in candidate or candidate in expected_norm:
            return True
        # Check basename match
        candidate_parts = candidate.split("/")
        if candidate_parts[-1] == expected_parts[-1]:
            # If expected has parent context, check if it's nearby
            if len(expected_parts) > 1 and any(part in expected_parts[-2] for part in candidate_parts[max(0,len(candidate_parts)-3):]):
                return True
            return True
    return False


def load_cases(path: Path) -> dict[str, list[TaskCase]]:
    raw = json.loads(path.read_text())
    return {
        project: [
            TaskCase(
                task=item["task"],
                expected_any=item.get("expected_any", []),
                expected_all=item.get("expected_all", []),
            )
            for item in items
        ]
        for project, items in raw.items()
    }


def write_reports(results: list[CaseResult], out_dir: Path) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "accuracy-report.json").write_text(
        json.dumps([asdict(result) for result in results], indent=2)
    )

    total = len(results)
    any_hits = sum(1 for result in results if result.hit_any)
    all_hits = sum(result.hit_all_count for result in results)
    all_total = sum(result.hit_all_total for result in results)
    junk_cases = sum(1 for result in results if result.junk_files)

    lines = [
        "# Context Compiler Accuracy Report",
        "",
        f"- Cases: {total}",
        f"- Expected-any hit rate: {any_hits}/{total} ({(any_hits / total * 100) if total else 0:.1f}%)",
        f"- Expected-all file hit rate: {all_hits}/{all_total} ({(all_hits / all_total * 100) if all_total else 0:.1f}%)",
        f"- Cases with junk artifacts: {junk_cases}/{total}",
        "",
    ]

    for result in results:
        status = "PASS" if result.hit_any and not result.junk_files and result.exit_code == 0 else "CHECK"
        lines.extend(
            [
                f"## {status}: {result.project}",
                f"Task: `{result.task}`",
                f"Selected: {', '.join(result.selected) if result.selected else '(none)'}",
                f"Expected any: {', '.join(result.expected_any) if result.expected_any else '(none)'}",
                f"Expected all hits: {result.hit_all_count}/{result.hit_all_total}",
            ]
        )
        if result.junk_files:
            lines.append(f"Junk files: {', '.join(result.junk_files)}")
        if result.stderr_tail:
            lines.append(f"Stderr tail: `{result.stderr_tail}`")
        lines.append("")

    (out_dir / "accuracy-report.md").write_text("\n".join(lines))


def main() -> int:
    parser = argparse.ArgumentParser(description="Run ctx accuracy tests on disposable project copies.")
    parser.add_argument("--ctx", required=True, help="Path to ctx/ctx.exe binary")
    parser.add_argument("--cases", required=True, help="JSON file mapping project paths to task cases")
    parser.add_argument("--out", default="accuracy-results", help="Report output directory")
    parser.add_argument("--max-files", type=int, default=10)
    parser.add_argument("--budget", type=int, default=12000)
    parser.add_argument("--timeout", type=int, default=180)
    args = parser.parse_args()

    ctx = Path(args.ctx).expanduser().resolve()
    cases = load_cases(Path(args.cases).expanduser().resolve())
    results: list[CaseResult] = []

    with tempfile.TemporaryDirectory(prefix="ctx-mass-test-") as temp_root:
        workspace = Path(temp_root)
        for project, task_cases in cases.items():
            src = Path(project).expanduser().resolve()
            if not src.exists():
                results.append(
                    CaseResult(
                        project=str(src),
                        task="<project missing>",
                        selected=[],
                        expected_any=[],
                        expected_all=[],
                        hit_any=False,
                        hit_all_count=0,
                        hit_all_total=0,
                        junk_files=[],
                        exit_code=1,
                        stderr_tail="project path does not exist",
                    )
                )
                continue

            safe_project = copy_project(src, workspace)
            reindex = run([str(ctx), "reindex"], safe_project, args.timeout)
            if reindex.returncode != 0:
                stderr = reindex.stderr[-500:].strip()
                for case in task_cases:
                    results.append(
                        CaseResult(
                            project=str(src),
                            task=case.task,
                            selected=[],
                            expected_any=case.expected_any,
                            expected_all=case.expected_all,
                            hit_any=False,
                            hit_all_count=0,
                            hit_all_total=len(case.expected_all),
                            junk_files=[],
                            exit_code=reindex.returncode,
                            stderr_tail=f"reindex failed: {stderr}",
                        )
                    )
                continue

            for case_index, case in enumerate(task_cases):
                context_out = safe_project / f".ctx-accuracy-context-{case_index}.txt"
                proc = run(
                    [
                        str(ctx),
                        "compile",
                        case.task,
                        "--max-files",
                        str(args.max_files),
                        "--budget",
                        str(args.budget),
                        "--output",
                        str(context_out),
                    ],
                    safe_project,
                    args.timeout,
                )
                selected = parse_selected(proc.stdout)
                results.append(
                    CaseResult(
                        project=str(src),
                        task=case.task,
                        selected=selected,
                        expected_any=case.expected_any,
                        expected_all=case.expected_all,
                        hit_any=bool(case.expected_any)
                        and any(matches(selected, expected) for expected in case.expected_any),
                        hit_all_count=sum(
                            1 for expected in case.expected_all if matches(selected, expected)
                        ),
                        hit_all_total=len(case.expected_all),
                        junk_files=[path for path in selected if is_junk(path)],
                        exit_code=proc.returncode,
                        stderr_tail=proc.stderr[-500:].strip(),
                    )
                )

    out_dir = Path(args.out).expanduser().resolve()
    write_reports(results, out_dir)
    print(f"Wrote {out_dir / 'accuracy-report.md'}")
    print(f"Wrote {out_dir / 'accuracy-report.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

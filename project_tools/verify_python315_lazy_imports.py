#!/usr/bin/env python3.15
"""Verify explicit Python 3.15 lazy imports in maintained project sources."""

from __future__ import annotations

lazy import ast
lazy import re
lazy import sys
lazy from collections import Counter
lazy from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parents[1]
EXCLUDED_PARTS = {
    ".git",
    ".toolchains",
    ".venv",
    "archive",
    "artifacts",
    "managed_components",
    "node_modules",
    "private",
    "vendor",
}
NON_PYTHON_REFERENCE = Path("software/ruview/references/script_1.py")
IMPORT_LINE = re.compile(rb"(?m)^\s*(?:lazy\s+)?(?:from|import)\s+")


def python_sources(root: Path = PROJECT_ROOT) -> list[Path]:
    """Return maintained Python sources, excluding generated and external trees."""
    sources = []
    for path in root.rglob("*.py"):
        relative = path.relative_to(root)
        if any(part in EXCLUDED_PARTS for part in relative.parts):
            continue
        if any(part.startswith("build") for part in relative.parts[:-1]):
            continue
        sources.append(path)
    return sorted(sources)


def eager_reason(node: ast.AST, parents: dict[ast.AST, ast.AST]) -> str | None:
    """Explain why Python requires this import to remain eager, if applicable."""
    if isinstance(node, ast.ImportFrom):
        if node.module == "__future__":
            return "future"
        if any(alias.name == "*" for alias in node.names):
            return "star"

    current = parents.get(node)
    while current is not None:
        if isinstance(current, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)):
            return "function"
        if isinstance(current, ast.ClassDef):
            return "class"
        if isinstance(current, (ast.Try, ast.TryStar)):
            return "try"
        current = parents.get(current)
    return None


def verify(root: Path = PROJECT_ROOT) -> tuple[int, Counter[str], list[str]]:
    """Return checked-file count, import counts, and actionable violations."""
    counts: Counter[str] = Counter()
    violations = []
    sources = python_sources(root)

    for path in sources:
        relative = path.relative_to(root)
        source = path.read_bytes()
        try:
            tree = ast.parse(source, filename=str(relative))
        except SyntaxError as error:
            if relative == NON_PYTHON_REFERENCE and not IMPORT_LINE.search(source):
                counts["non_python_reference"] += 1
                continue
            violations.append(
                f"{relative}:{error.lineno}: cannot parse with Python 3.15: {error.msg}"
            )
            continue

        parents = {
            child: parent
            for parent in ast.walk(tree)
            for child in ast.iter_child_nodes(parent)
        }
        for node in ast.walk(tree):
            if not isinstance(node, (ast.Import, ast.ImportFrom)):
                continue
            reason = eager_reason(node, parents)
            if reason is not None:
                counts[f"required_eager_{reason}"] += 1
                continue
            counts["eligible"] += 1
            if not getattr(node, "is_lazy", 0):
                violations.append(
                    f"{relative}:{node.lineno}: module import must be explicit lazy"
                )

    return len(sources), counts, violations


def main() -> int:
    if sys.version_info < (3, 15):
        print(
            "Python 3.15 or newer is required to verify lazy imports.", file=sys.stderr
        )
        return 2

    file_count, counts, violations = verify()
    if violations:
        print("\n".join(violations), file=sys.stderr)
        return 1

    required_eager = sum(
        count
        for reason, count in counts.items()
        if reason.startswith("required_eager_")
    )
    print(
        f"Verified {counts['eligible']} explicit lazy imports across {file_count} files; "
        f"{required_eager} imports remain eager because Python 3.15 requires it."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3.15
"""Create a static-only parser view; never import or run application sources.

Semgrep CE 1.180 cannot parse PEP 810 or some ESP-IDF/wasm3 macros.
The original sources stay unchanged. Lines are preserved and SARIF columns
are mapped back after expanding the small, explicit macro set below.
Remove this adapter when the pinned scanner parses those constructs natively.
"""

from __future__ import annotations

lazy import argparse
lazy import ast
lazy import json
lazy import re
lazy import shutil
lazy import subprocess
lazy from pathlib import Path

ROOTS = ("software/ruview", "project_tools", "tools")
EXCLUDED = {"vendor", "node_modules", "target", "managed_components", "build",
            "private", ".codex", ".agents"}
C_TOKENS = re.compile(r'//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|[A-Za-z_]\w*|\s+|.', re.S)
WASM_MACROS = {"m3ApiRawFunction", "m3ApiReturnType", "m3ApiGetArg", "m3ApiReturn"}


def python_view(source: str) -> str:
    """Validate PEP 810 with Python itself; change only its import modifier."""
    if "lazy" not in source:
        return source
    tree = ast.parse(source)
    # ast.parse alone accepts lazy imports in forbidden scopes; compile checks
    # Python's restrictions without executing any module or import.
    compile(tree, "<scan-source>", "exec")
    lines = source.splitlines(keepends=True)
    for node in ast.walk(tree):
        if isinstance(node, (ast.Import, ast.ImportFrom)) and node.is_lazy:
            line = lines[node.lineno - 1].encode("utf-8")
            start = node.col_offset
            if line[start:start + 5] != b"lazy ":
                raise ValueError("Unexpected lazy-import spelling; update the adapter")
            # Same byte count and indentation, including semicolon/inline imports.
            lines[node.lineno - 1] = (line[:start] + b"pass;" + line[start + 5:]).decode("utf-8")
    result = "".join(lines)
    ast.parse(result)
    return result


def c_view(source: str) -> tuple[str, list[dict]]:
    """Expand only literal format macros and wasm3's four raw-API macros.

    wasm3 v0.5.0 source/wasm3.h:323-331 defines these expansions. All other
    tokens, including both sides of preprocessor conditions, are retained.
    """
    tokens = list(C_TOKENS.finditer(source))
    edits = []
    index = 0
    while index < len(tokens):
        token = tokens[index]
        name = token.group()
        end = token.end()
        replacement = None
        fmt = re.fullmatch(r"PRI([diuoxX])(8|16|32|64)", name)
        if fmt:
            replacement = '"' + ("ll" if fmt[2] == "64" else "") + fmt[1] + '"'
        elif name == "IPSTR":
            replacement = '"%d.%d.%d.%d"'
        elif name in WASM_MACROS:
            opening = index + 1
            while opening < len(tokens) and tokens[opening].group().isspace():
                opening += 1
            if opening >= len(tokens) or tokens[opening].group() != "(":
                raise ValueError("Unexpected wasm3 macro spelling")
            closing, depth = opening + 1, 1
            while closing < len(tokens) and depth:
                value = tokens[closing].group()
                depth += (value == "(") - (value == ")")
                closing += 1
            if depth:
                raise ValueError("Unclosed wasm3 macro")
            last = tokens[closing - 1]
            argument = source[tokens[opening].end():last.start()]
            if any(char in source[token.start():last.end()] for char in "\r\n"):
                raise ValueError("Multiline wasm3 macro needs explicit source mapping")
            if name == "m3ApiRawFunction":
                replacement = f"const void * {argument} (IM3Runtime runtime, IM3ImportContext _ctx, uint64_t * _sp, void * _mem)"
            elif name == "m3ApiReturnType":
                replacement = f"{argument} *raw_return = (({argument} *) (_sp++))"
            elif name == "m3ApiGetArg":
                arg_type, arg_name = argument.split(",")
                replacement = f"{arg_type} {arg_name} = *(({arg_type} *) (_sp++))"
            else:
                replacement = f"{{ *raw_return = ({argument}); return m3Err_none; }}"
            end, index = last.end(), closing - 1
        if replacement is not None:
            line = source.count("\n", 0, token.start()) + 1
            line_start = source.rfind("\n", 0, token.start()) + 1
            edits.append({"start": token.start(), "end": end, "text": replacement,
                          "line": line, "column": token.start() - line_start + 1})
        index += 1
    result = source
    for edit in reversed(edits):
        result = result[:edit["start"]] + edit["text"] + result[edit["end"]:]
    return result, edits


def prepare(root: Path, output: Path) -> None:
    # Fail on a reused destination, rather than overwriting files or backups.
    output.mkdir(parents=True, exist_ok=False)
    names = subprocess.check_output(
        ["git", "-C", str(root), "ls-files", "-z", "--", *ROOTS]
    ).decode().split("\0")
    mapping = {}
    changed = {"python": 0, "c": 0}
    for name in filter(None, names):
        relative = Path(name)
        if (any(part in EXCLUDED or part.startswith("build-") for part in relative.parts[:-1])
                or "data/raw" in name or "data/recordings" in name):
            continue
        source_path = root / relative
        if source_path.is_symlink():
            raise ValueError(f"Scan input must not be a symlink: {name}")
        destination = output / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        if relative.suffix in {".py", ".c"}:
            source = source_path.read_text(encoding="utf-8")
            if relative.suffix == ".py":
                converted = python_view(source)
                kind = "python"
            else:
                converted, edits = c_view(source)
                if edits:
                    mapping[name] = edits
                kind = "c"
            changed[kind] += converted != source
            destination.write_text(converted, encoding="utf-8")
        else:
            shutil.copyfile(source_path, destination)
    (output / "source-map.json").write_text(json.dumps(mapping), encoding="utf-8")
    print(f"Static parser view: {changed['python']} PEP 810 files, {changed['c']} C macro files")


def original_column(column: int, line: int, edits: list[dict]) -> int:
    delta = 0
    for edit in edits:
        if edit["line"] != line:
            continue
        start = edit["column"] + delta
        if column < start:
            break
        if column < start + len(edit["text"]):
            return edit["column"]
        delta += len(edit["text"]) - (edit["end"] - edit["start"])
    return column - delta


def remap_sarif(sarif: Path, mapping: Path) -> None:
    document = json.loads(sarif.read_text())
    edits_by_path = json.loads(mapping.read_text())

    def visit(value):
        if isinstance(value, dict):
            location = value.get("physicalLocation", {})
            path = location.get("artifactLocation", {}).get("uri", "")
            for key in ("region", "contextRegion"):
                region = location.get(key, {})
                for bound in ("start", "end"):
                    if bound + "Column" in region and path in edits_by_path:
                        region[bound + "Column"] = original_column(
                            region[bound + "Column"], region.get(bound + "Line", region.get("startLine")), edits_by_path[path])
                if "snippet" in region and path in edits_by_path:
                    original = (Path(__file__).resolve().parents[1] / path).read_text().splitlines(keepends=True)
                    first = region["startLine"] - 1
                    last = region.get("endLine", region["startLine"]) - 1
                    snippet = original[first:last + 1]
                    if "endColumn" in region:
                        snippet[-1] = snippet[-1][:region["endColumn"] - 1]
                    snippet[0] = snippet[0][region.get("startColumn", 1) - 1:]
                    region["snippet"]["text"] = "".join(snippet)
            for child in value.values():
                visit(child)
        elif isinstance(value, list):
            for child in value:
                visit(child)

    visit(document)
    sarif.write_text(json.dumps(document), encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--sarif", type=Path)
    parser.add_argument("--mapping", type=Path)
    args = parser.parse_args()
    if args.output and not (args.sarif or args.mapping):
        prepare(Path(__file__).resolve().parents[1], args.output)
    elif args.sarif and args.mapping and not args.output:
        remap_sarif(args.sarif, args.mapping)
    else:
        parser.error("Use --output or both --sarif and --mapping")


if __name__ == "__main__":
    main()

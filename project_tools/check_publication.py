#!/usr/bin/env python3
"""Check the proposed Git tree locally; never publish detected values."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

LOCAL_ROOTS = {"artifacts", "devlogs", ".toolchains"}
LOCAL_PARTS = {"private", ".claude",
               ".claude-flow", ".ruv", ".ruvector", ".swarm", ".hive-mind", ".codex", ".agents"}


def path_problem(name):
    path = Path(name)
    if path.is_absolute() or ".." in path.parts or not path.parts:
        return "unsafe path"
    if name.startswith(("software/ruview/data/", "software/ruview/v2/data/")):
        return "runtime recordings/configuration, not source"
    if path.parts[0] == "historical-screenshots" and path.name not in {
        "2026-06-23_22-33-57_display-pinout.png",
        "2026-06-26_17-00-30_websocket-pose-stream-real-backend.png",
    }:
        return "unreviewed/private historical desktop image"
    if path.parts[0] in LOCAL_ROOTS or LOCAL_PARTS.intersection(path.parts):
        return "private/local state"
    if path.name in {".env", "sdkconfig", "sdkconfig.old"} or ".private." in path.name:
        return "private configuration/evidence"
    if path.suffix.lower() in {".pem", ".key", ".pcap", ".pcapng"}:
        return "private key/capture"
    if path.name in {"session_memory.json", "brain_corpus.jsonl", "merged_corpus.jsonl"}:
        return "generated learning/session memory"
    if "claude-code-rvsource" in path.parts:
        return "excluded extracted CLI research dump"
    return None


def finding_key(item, root):
    path = Path(item["File"])
    name = path.relative_to(root).as_posix() if path.is_absolute() else path.as_posix()
    digest = hashlib.sha256((root / name).read_bytes()).hexdigest()
    return name, item["RuleID"], item["StartLine"], item["EndLine"], digest


def check_secrets(root):
    if not shutil.which("gitleaks"):
        raise SystemExit("Gitleaks is required for --secrets (tested: 8.30.1)")
    baseline = json.loads((root / "project_tools/secret-scan-baseline.json").read_text())
    approved = {(x["path"], x["rule"], x["start_line"], x["end_line"], x["file_sha256"])
                for x in baseline}
    with tempfile.TemporaryDirectory(prefix="observatory-secrets-") as tmp:
        temp = Path(tmp)
        config = temp / "rules.toml"
        config.write_text("[extend]\nuseDefault = true\n")
        report = temp / "findings.json"
        result = subprocess.run([
            "gitleaks", "dir", str(root), "--config", str(config),
            "--redact=100", "--no-banner", "--no-color", "--ignore-gitleaks-allow",
            "--gitleaks-ignore-path", str(temp / "no-ignore-file"),
            "--max-archive-depth", "2", "--report-format", "json", "--report-path", str(report)
        ], capture_output=True, text=True)
        if result.returncode not in (0, 1) or not report.exists():
            raise SystemExit("Secret scanner failed; no clean result can be claimed")
        findings = json.loads(report.read_text())
        unknown = [x for x in findings if finding_key(x, root) not in approved]
        for item in unknown:
            name, rule, start, _, _ = finding_key(item, root)
            print(f"REVIEW REQUIRED: {name}:{start} ({rule}); value withheld")
        if unknown:
            raise SystemExit(f"Secret check blocked: {len(unknown)} unreviewed findings")
        print(f"Secret check: {len(findings)} reviewed synthetic/example findings, 0 unreviewed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--secrets", action="store_true", help="also run local Gitleaks")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = subprocess.check_output([
        "git", "-C", str(root), "ls-files", "--cached", "--others", "--exclude-standard", "-z"
    ])
    names = sorted(set(output.decode().strip("\0").split("\0")))
    failures = []
    total = 0
    for name in names:
        path = root / name
        reason = path_problem(name)
        if reason:
            failures.append(f"{name}: {reason}")
            continue
        if path.is_symlink():
            target = path.resolve()
            if not target.is_relative_to(root) or not target.exists() or target.relative_to(root).as_posix() not in names:
                failures.append(f"{name}: external, missing, or unpublished symlink target")
        elif not path.is_file():
            failures.append(f"{name}: missing/non-regular source")
        else:
            total += path.stat().st_size
            if path.stat().st_size > 95 * 1024 * 1024:
                failures.append(f"{name}: exceeds 95 MiB publication limit")
    if failures:
        raise SystemExit("Publication boundary failed:\n" + "\n".join(failures))
    print(f"Publication boundary: {len(names)} files, {total} bytes, no private paths or external links")
    if args.secrets:
        check_secrets(root)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""
Export training data for RuvLTRA model fine-tuning.

Implements dataset governance from ADR-129 Section 2.2:
  - Record schema validation (id, source, text, license, quality_score, provenance, content_hash)
  - SHA-256 content dedup
  - Quality score filtering (< 0.5 excluded)
  - Output statistics (count, token count per source, quality histogram)

Sources:
  1. Brain memories from pi.ruv.io (graceful fallback on connection failure)
  2. ADR corpus from docs/adr/
  3. Claude Flow routing dataset reference (ruvnet/claude-flow-routing on HF)

Output: data/training/corpus.jsonl
"""

import hashlib
import json
import os
import sys
import uuid
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path
from urllib.error import URLError
from urllib.request import Request, urlopen

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parents[2]
ADR_DIR = REPO_ROOT / "docs" / "adr"
OUTPUT_DIR = REPO_ROOT / "data" / "training"
OUTPUT_FILE = OUTPUT_DIR / "corpus.jsonl"

BRAIN_API = "https://pi.ruv.io/v1/memories/list"
BRAIN_LIMIT = 5000
BRAIN_TIMEOUT_S = 15

QUALITY_THRESHOLD = 0.5

# ADR-129 Section 2.2 source allowlist
ALLOWED_SOURCES = {"brain", "wet", "claude-routing", "code", "adr"}
ALLOWED_LICENSES = {"apache-2.0", "mit", "cc-by-4.0", "public-domain"}


# ---------------------------------------------------------------------------
# Record helpers
# ---------------------------------------------------------------------------

def content_hash(text: str) -> str:
    """SHA-256 hash of the text content for dedup."""
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def make_record(
    source: str,
    text: str,
    license_id: str,
    quality_score: float,
    provenance: str,
    created_at: str | None = None,
) -> dict:
    """Build a training record conforming to ADR-129 Section 2.2 schema."""
    if source not in ALLOWED_SOURCES:
        raise ValueError(f"Source '{source}' not in allowlist: {ALLOWED_SOURCES}")
    if license_id not in ALLOWED_LICENSES:
        raise ValueError(f"License '{license_id}' not in allowlist: {ALLOWED_LICENSES}")

    return {
        "id": str(uuid.uuid4()),
        "source": source,
        "text": text,
        "license": license_id,
        "quality_score": round(quality_score, 4),
        "provenance": provenance,
        "created_at": created_at or datetime.now(timezone.utc).isoformat(),
        "content_hash": content_hash(text),
    }


def validate_record(record: dict) -> list[str]:
    """Validate a record against the ADR-129 schema. Returns list of errors."""
    required = {"id", "source", "text", "license", "quality_score", "provenance",
                "created_at", "content_hash"}
    errors = []
    missing = required - set(record.keys())
    if missing:
        errors.append(f"Missing fields: {missing}")
    if record.get("source") not in ALLOWED_SOURCES:
        errors.append(f"Invalid source: {record.get('source')}")
    if record.get("license") not in ALLOWED_LICENSES:
        errors.append(f"Invalid license: {record.get('license')}")
    qs = record.get("quality_score")
    if qs is not None and not (0.0 <= qs <= 1.0):
        errors.append(f"quality_score out of range: {qs}")
    if not record.get("text", "").strip():
        errors.append("Empty text")
    ch = record.get("content_hash", "")
    if len(ch) != 64:
        errors.append(f"Invalid content_hash length: {len(ch)}")
    return errors


def estimate_tokens(text: str) -> int:
    """Rough token estimate: ~4 chars per token for English/code."""
    return max(1, len(text) // 4)


# ---------------------------------------------------------------------------
# Source 1: Brain memories from pi.ruv.io
# ---------------------------------------------------------------------------

def fetch_brain_memories() -> list[dict]:
    """Fetch memories from pi.ruv.io. Returns empty list on failure."""
    url = f"{BRAIN_API}?limit={BRAIN_LIMIT}"
    print(f"[brain] Fetching memories from {url} ...")

    try:
        req = Request(url, headers={"Accept": "application/json"})
        with urlopen(req, timeout=BRAIN_TIMEOUT_S) as resp:
            data = json.loads(resp.read().decode("utf-8"))
    except (URLError, OSError, json.JSONDecodeError, TimeoutError) as exc:
        print(f"[brain] Connection failed ({type(exc).__name__}: {exc}). "
              "Falling back to local-only sources.")
        return []

    memories = data if isinstance(data, list) else data.get("memories", [])
    print(f"[brain] Received {len(memories)} memories.")

    records = []
    for mem in memories:
        text = mem.get("content") or mem.get("text") or mem.get("value", "")
        if not text or not text.strip():
            continue

        # Quality score from brain API confidence, default 0.7
        confidence = mem.get("confidence", mem.get("quality", 0.7))
        try:
            quality = float(confidence)
        except (TypeError, ValueError):
            quality = 0.7

        provenance = mem.get("url") or mem.get("source_url") or "pi.ruv.io/brain"
        created = mem.get("created_at") or mem.get("timestamp") or datetime.now(timezone.utc).isoformat()

        records.append(make_record(
            source="brain",
            text=text.strip(),
            license_id="apache-2.0",
            quality_score=quality,
            provenance=provenance,
            created_at=created,
        ))

    return records


# ---------------------------------------------------------------------------
# Source 2: ADR corpus from docs/adr/
# ---------------------------------------------------------------------------

def load_adr_corpus() -> list[dict]:
    """Read all ADR markdown files and convert to training records."""
    if not ADR_DIR.is_dir():
        print(f"[adr] Directory not found: {ADR_DIR}")
        return []

    adr_files = sorted(ADR_DIR.glob("*.md"))
    print(f"[adr] Found {len(adr_files)} ADR files in {ADR_DIR}")

    records = []
    for adr_path in adr_files:
        try:
            text = adr_path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as exc:
            print(f"[adr] Skipping {adr_path.name}: {exc}")
            continue

        if not text.strip():
            continue

        # ADRs are project-owned, MIT, quality = 1.0 per ADR-129 Section 2.2
        records.append(make_record(
            source="adr",
            text=text.strip(),
            license_id="mit",
            quality_score=1.0,
            provenance=f"docs/adr/{adr_path.name}",
        ))

    return records


# ---------------------------------------------------------------------------
# Source 3: Claude Flow routing dataset reference
# ---------------------------------------------------------------------------

def routing_dataset_reference() -> list[dict]:
    """
    Output a reference record for the HuggingFace routing dataset.

    The actual dataset (ruvnet/claude-flow-routing, 2700+ examples) is not
    downloaded here -- it should be fetched via `datasets` library or
    `huggingface-cli` during the actual training pipeline. This record serves
    as a corpus manifest entry so the dataset is tracked in provenance.
    """
    ref_text = (
        "Claude Flow routing dataset — 2,700+ examples of agent routing decisions. "
        "Source: HuggingFace dataset ruvnet/claude-flow-routing. "
        "This is a reference record; fetch the full dataset via "
        "`datasets.load_dataset('ruvnet/claude-flow-routing')` during training."
    )
    return [make_record(
        source="claude-routing",
        text=ref_text,
        license_id="apache-2.0",
        quality_score=1.0,
        provenance="https://huggingface.co/datasets/ruvnet/claude-flow-routing",
    )]


# ---------------------------------------------------------------------------
# Governance: dedup, quality filter, validation, statistics
# ---------------------------------------------------------------------------

def deduplicate(records: list[dict]) -> list[dict]:
    """SHA-256 content-hash dedup at record level (ADR-129 Section 2.2)."""
    seen: set[str] = set()
    unique = []
    dupes = 0
    for rec in records:
        h = rec["content_hash"]
        if h in seen:
            dupes += 1
            continue
        seen.add(h)
        unique.append(rec)
    if dupes:
        print(f"[dedup] Removed {dupes} duplicate records by content hash.")
    return unique


def quality_filter(records: list[dict]) -> list[dict]:
    """Exclude records with quality_score < 0.5 (ADR-129 Section 2.2)."""
    before = len(records)
    filtered = [r for r in records if r["quality_score"] >= QUALITY_THRESHOLD]
    removed = before - len(filtered)
    if removed:
        print(f"[quality] Excluded {removed} records below quality threshold {QUALITY_THRESHOLD}.")
    return filtered


def validate_all(records: list[dict]) -> list[dict]:
    """Validate all records, dropping invalid ones with warnings."""
    valid = []
    for rec in records:
        errors = validate_record(rec)
        if errors:
            print(f"[validate] Dropping record {rec.get('id', '?')}: {errors}")
        else:
            valid.append(rec)
    return valid


def compute_statistics(records: list[dict]) -> dict:
    """Compute corpus statistics as required by ADR-129 Section 2.2."""
    source_counts: Counter = Counter()
    source_tokens: Counter = Counter()
    quality_bins = defaultdict(int)  # 0.0-0.1, 0.1-0.2, ..., 0.9-1.0

    total_tokens = 0
    for rec in records:
        src = rec["source"]
        tokens = estimate_tokens(rec["text"])
        source_counts[src] += 1
        source_tokens[src] += tokens
        total_tokens += tokens

        # Histogram bin
        bin_idx = min(int(rec["quality_score"] * 10), 9)
        bin_label = f"{bin_idx/10:.1f}-{(bin_idx+1)/10:.1f}"
        quality_bins[bin_label] += 1

    # Sort bins
    quality_histogram = dict(sorted(quality_bins.items()))

    stats = {
        "total_records": len(records),
        "total_estimated_tokens": total_tokens,
        "per_source": {
            src: {"count": source_counts[src], "estimated_tokens": source_tokens[src]}
            for src in sorted(source_counts)
        },
        "quality_histogram": quality_histogram,
        "exported_at": datetime.now(timezone.utc).isoformat(),
    }
    return stats


def print_statistics(stats: dict) -> None:
    """Pretty-print corpus statistics."""
    print("\n" + "=" * 60)
    print("CORPUS STATISTICS")
    print("=" * 60)
    print(f"Total records:          {stats['total_records']}")
    print(f"Total estimated tokens: {stats['total_estimated_tokens']:,}")
    print()
    print("Per source:")
    for src, info in stats["per_source"].items():
        print(f"  {src:20s}  {info['count']:6d} records  {info['estimated_tokens']:>10,} tokens")
    print()
    print("Quality histogram:")
    for bin_label, count in stats["quality_histogram"].items():
        bar = "#" * min(count, 60)
        print(f"  [{bin_label}]  {count:5d}  {bar}")
    print("=" * 60)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

    # Collect from all sources
    print("Collecting training data (ADR-129 Section 2.2 governance)...\n")

    records: list[dict] = []
    records.extend(fetch_brain_memories())
    records.extend(load_adr_corpus())
    records.extend(routing_dataset_reference())

    print(f"\n[total] Collected {len(records)} raw records.")

    # Governance pipeline
    records = validate_all(records)
    records = deduplicate(records)
    records = quality_filter(records)

    print(f"[final] {len(records)} records after governance pipeline.")

    # Write JSONL
    with open(OUTPUT_FILE, "w", encoding="utf-8") as fh:
        for rec in records:
            fh.write(json.dumps(rec, ensure_ascii=False) + "\n")

    print(f"\nCorpus written to: {OUTPUT_FILE}")

    # Statistics
    stats = compute_statistics(records)
    print_statistics(stats)

    # Write stats sidecar
    stats_file = OUTPUT_DIR / "corpus_stats.json"
    with open(stats_file, "w", encoding="utf-8") as fh:
        json.dump(stats, fh, indent=2, ensure_ascii=False)
    print(f"Statistics written to: {stats_file}")


if __name__ == "__main__":
    main()

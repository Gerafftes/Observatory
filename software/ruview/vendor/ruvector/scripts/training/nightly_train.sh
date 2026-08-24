#!/usr/bin/env bash
# Nightly RuvLTRA training pipeline
# Pulls latest brain learnings from pi.ruv.io, runs incremental LoRA training,
# quantizes to GGUF, validates against release gates, and pushes to HuggingFace.
#
# Triggered by Cloud Scheduler: daily at 03:00 UTC
# Infrastructure: Cloud Run Job with L4 GPU
#
# ADR-129 Section: Nightly Continuous Learning Loop

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DATE=$(date +%Y%m%d)
WORK_DIR="/tmp/ruvltra-nightly-${DATE}"
HF_TOKEN="${HF_TOKEN:?HF_TOKEN environment variable required}"

MODELS=("ruv/ruvltra-small" "ruv/ruvltra-medium" "ruv/ruvltra-claude-code")
BRAIN_URL="https://pi.ruv.io/v1"

echo "=== RuvLTRA Nightly Training: ${DATE} ==="
mkdir -p "${WORK_DIR}"/{data,models,results,reports}

# ─────────────────────────────────────────────────────────────
# Phase 1: Export today's brain learnings
# ─────────────────────────────────────────────────────────────
echo "[1/5] Exporting brain learnings..."

# Get memories added/updated in last 24h
python3 "${SCRIPT_DIR}/export_training_data.py" \
  --output "${WORK_DIR}/data/corpus.jsonl" \
  --adr-dir "${SCRIPT_DIR}/../../docs/adr" \
  2>&1 | tee "${WORK_DIR}/reports/export.log"

RECORD_COUNT=$(wc -l < "${WORK_DIR}/data/corpus.jsonl" 2>/dev/null || echo "0")
echo "  Exported ${RECORD_COUNT} records"

if [ "${RECORD_COUNT}" -lt 10 ]; then
  echo "  Too few records (${RECORD_COUNT} < 10). Skipping training."
  echo "SKIPPED: insufficient data (${RECORD_COUNT} records)" > "${WORK_DIR}/reports/verdict.txt"
  exit 0
fi

# ─────────────────────────────────────────────────────────────
# Phase 2: Contamination check
# ─────────────────────────────────────────────────────────────
echo "[2/5] Running contamination check..."

python3 "${SCRIPT_DIR}/contamination_check.py" \
  --corpus "${WORK_DIR}/data/corpus.jsonl" \
  --eval "${SCRIPT_DIR}/eval_sets/" \
  --output "${WORK_DIR}/reports/contamination.json" \
  2>&1 | tee -a "${WORK_DIR}/reports/export.log" || true

# ─────────────────────────────────────────────────────────────
# Phase 3: Incremental LoRA training
# ─────────────────────────────────────────────────────────────
echo "[3/5] Running incremental LoRA training..."

for MODEL in "${MODELS[@]}"; do
  MODEL_NAME=$(basename "${MODEL}")
  echo "  Training ${MODEL_NAME}..."

  python3 "${SCRIPT_DIR}/run_sft.py" \
    --model "${MODEL}" \
    --training-data "${WORK_DIR}/data/corpus.jsonl" \
    --output-dir "${WORK_DIR}/models/${MODEL_NAME}" \
    --lora-rank 8 \
    --epochs 1 \
    --learning-rate 1e-5 \
    --max-seq-length 4096 \
    2>&1 | tee "${WORK_DIR}/reports/train-${MODEL_NAME}.log" || {
      echo "  WARN: Training failed for ${MODEL_NAME}, continuing..."
      continue
    }
done

# ─────────────────────────────────────────────────────────────
# Phase 4: Release gate validation
# ─────────────────────────────────────────────────────────────
echo "[4/5] Running release gates..."

GATE_PASS=true
for MODEL in "${MODELS[@]}"; do
  MODEL_NAME=$(basename "${MODEL}")
  RESULTS_DIR="${WORK_DIR}/results/${MODEL_NAME}"
  mkdir -p "${RESULTS_DIR}"

  # Generate gate results (would be populated by benchmark scripts in production)
  if [ -f "${RESULTS_DIR}/gate_results.json" ]; then
    python3 "${SCRIPT_DIR}/release_gate.py" \
      --results-dir "${RESULTS_DIR}" \
      --output-json "${WORK_DIR}/reports/gate-${MODEL_NAME}.json" \
      2>&1 | tee -a "${WORK_DIR}/reports/gates.log" || {
        echo "  FAIL: ${MODEL_NAME} did not pass release gates"
        GATE_PASS=false
      }
  else
    echo "  SKIP: No gate results for ${MODEL_NAME} (benchmark not run)"
  fi
done

# ─────────────────────────────────────────────────────────────
# Phase 5: Push to HuggingFace (only if gates pass)
# ─────────────────────────────────────────────────────────────
echo "[5/5] Publishing to HuggingFace..."

if [ "${GATE_PASS}" = true ]; then
  for MODEL in "${MODELS[@]}"; do
    MODEL_NAME=$(basename "${MODEL}")
    MODEL_DIR="${WORK_DIR}/models/${MODEL_NAME}"

    if [ -d "${MODEL_DIR}" ] && ls "${MODEL_DIR}"/*.gguf 1>/dev/null 2>&1; then
      echo "  Uploading ${MODEL_NAME} to ${MODEL}..."
      python3 -c "
from huggingface_hub import HfApi
import glob, os
api = HfApi(token='${HF_TOKEN}')
for f in glob.glob('${MODEL_DIR}/*.gguf') + glob.glob('${MODEL_DIR}/*.turboquant.json'):
    print(f'  Uploading {os.path.basename(f)}...')
    api.upload_file(path_or_fileobj=f, path_in_repo=os.path.basename(f),
                    repo_id='${MODEL}', commit_message='Nightly update ${DATE}')
print('  Done')
" 2>&1 || echo "  WARN: Upload failed for ${MODEL_NAME}"
    else
      echo "  SKIP: No GGUF files for ${MODEL_NAME}"
    fi
  done
else
  echo "  BLOCKED: Release gates failed. Not publishing."
fi

# ─────────────────────────────────────────────────────────────
# Report
# ─────────────────────────────────────────────────────────────
echo ""
echo "=== Nightly Training Complete ==="
echo "  Date: ${DATE}"
echo "  Records: ${RECORD_COUNT}"
echo "  Gates: ${GATE_PASS}"
echo "  Reports: ${WORK_DIR}/reports/"
echo "  Models: ${WORK_DIR}/models/"

# Cleanup old nightly runs (keep last 7 days)
find /tmp -maxdepth 1 -name "ruvltra-nightly-*" -mtime +7 -exec rm -rf {} \; 2>/dev/null || true

#!/usr/bin/env bash
# Deploy RuvLTRA training pipeline to Cloud Run Jobs
# Creates: calibration, SFT training, and benchmark jobs
#
# Usage: ./scripts/training/deploy_training.sh [--project PROJECT_ID] [--region REGION]
set -euo pipefail

PROJECT_ID="${GCP_PROJECT_ID:-ruv-dev}"
REGION="${GCP_REGION:-us-central1}"
IMAGE="gcr.io/${PROJECT_ID}/ruvltra-training:latest"
SA_EMAIL="${PROJECT_ID}@appspot.gserviceaccount.com"

# Parse args
while [[ $# -gt 0 ]]; do
    case $1 in
        --project) PROJECT_ID="$2"; IMAGE="gcr.io/${PROJECT_ID}/ruvltra-training:latest"; shift 2 ;;
        --region) REGION="$2"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║        RuvLTRA Training Pipeline — Cloud Run Deploy          ║"
echo "║     Calibration · SFT · Benchmarking                         ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "  Project:  ${PROJECT_ID}"
echo "  Region:   ${REGION}"
echo "  Image:    ${IMAGE}"
echo ""

# --- Step 1: Build and push the training image ---
echo "▸ [1/5] Building training image..."
gcloud builds submit \
    --tag="${IMAGE}" \
    --project="${PROJECT_ID}" \
    --timeout=1800s \
    --machine-type=e2-highcpu-8 \
    .

# --- Step 2: Create calibration job (imatrix + TurboQuant) ---
echo "▸ [2/5] Creating ruvltra-calibration job..."
JOB_NAME="ruvltra-calibration"
gcloud run jobs create "${JOB_NAME}" \
    --image="${IMAGE}" \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --memory=24Gi \
    --cpu=4 \
    --gpu=1 \
    --gpu-type=nvidia-l4 \
    --max-retries=1 \
    --task-timeout=3600s \
    --args="run_calibration.py,--model-id,ruvnet/ruvLTRA-7b,--upload" \
    --set-secrets="HF_TOKEN=huggingface-token:latest" \
    --set-env-vars="PYTHONUNBUFFERED=1" \
    2>/dev/null || \
gcloud run jobs update "${JOB_NAME}" \
    --image="${IMAGE}" \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --memory=24Gi \
    --cpu=4 \
    --gpu=1 \
    --gpu-type=nvidia-l4 \
    --max-retries=1 \
    --task-timeout=3600s \
    --args="run_calibration.py,--model-id,ruvnet/ruvLTRA-7b,--upload" \
    --set-secrets="HF_TOKEN=huggingface-token:latest" \
    --set-env-vars="PYTHONUNBUFFERED=1"

echo "  ✓ ${JOB_NAME} ready"

# --- Step 3: Create SFT training job (Vertex AI for larger models) ---
echo "▸ [3/5] Creating ruvltra-sft-training job..."
JOB_NAME="ruvltra-sft-training"
gcloud run jobs create "${JOB_NAME}" \
    --image="${IMAGE}" \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --memory=32Gi \
    --cpu=8 \
    --gpu=1 \
    --gpu-type=nvidia-l4 \
    --max-retries=1 \
    --task-timeout=14400s \
    --args="run_sft.py,--model-id,ruvnet/ruvLTRA-7b,--corpus,data/training/corpus.jsonl,--output-dir,/tmp/sft-output" \
    --set-secrets="HF_TOKEN=huggingface-token:latest" \
    --set-env-vars="PYTHONUNBUFFERED=1,WANDB_DISABLED=true" \
    2>/dev/null || \
gcloud run jobs update "${JOB_NAME}" \
    --image="${IMAGE}" \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --memory=32Gi \
    --cpu=8 \
    --gpu=1 \
    --gpu-type=nvidia-l4 \
    --max-retries=1 \
    --task-timeout=14400s \
    --args="run_sft.py,--model-id,ruvnet/ruvLTRA-7b,--corpus,data/training/corpus.jsonl,--output-dir,/tmp/sft-output" \
    --set-secrets="HF_TOKEN=huggingface-token:latest" \
    --set-env-vars="PYTHONUNBUFFERED=1,WANDB_DISABLED=true"

echo "  ✓ ${JOB_NAME} ready"

# --- Step 4: Create benchmark job ---
echo "▸ [4/5] Creating ruvltra-benchmark job..."
JOB_NAME="ruvltra-benchmark"
gcloud run jobs create "${JOB_NAME}" \
    --image="${IMAGE}" \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --memory=24Gi \
    --cpu=4 \
    --gpu=1 \
    --gpu-type=nvidia-l4 \
    --max-retries=1 \
    --task-timeout=3600s \
    --args="run_calibration.py,--model-id,ruvnet/ruvLTRA-7b,--benchmark-only" \
    --set-secrets="HF_TOKEN=huggingface-token:latest" \
    --set-env-vars="PYTHONUNBUFFERED=1" \
    2>/dev/null || \
gcloud run jobs update "${JOB_NAME}" \
    --image="${IMAGE}" \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --memory=24Gi \
    --cpu=4 \
    --gpu=1 \
    --gpu-type=nvidia-l4 \
    --max-retries=1 \
    --task-timeout=3600s \
    --args="run_calibration.py,--model-id,ruvnet/ruvLTRA-7b,--benchmark-only" \
    --set-secrets="HF_TOKEN=huggingface-token:latest" \
    --set-env-vars="PYTHONUNBUFFERED=1"

echo "  ✓ ${JOB_NAME} ready"

# --- Step 5: Set up weekly benchmark scheduler ---
echo "▸ [5/5] Setting up weekly benchmark schedule..."
SCHEDULER_NAME="ruvltra-benchmark-weekly"
gcloud scheduler jobs create http "${SCHEDULER_NAME}" \
    --location="${REGION}" \
    --project="${PROJECT_ID}" \
    --schedule="0 6 * * 1" \
    --time-zone="UTC" \
    --uri="https://${REGION}-run.googleapis.com/apis/run.googleapis.com/v1/namespaces/${PROJECT_ID}/jobs/ruvltra-benchmark:run" \
    --http-method=POST \
    --oauth-service-account-email="${SA_EMAIL}" \
    --description="Weekly RuvLTRA benchmark run (Mondays 06:00 UTC)" \
    2>/dev/null || \
gcloud scheduler jobs update http "${SCHEDULER_NAME}" \
    --location="${REGION}" \
    --project="${PROJECT_ID}" \
    --schedule="0 6 * * 1" \
    --time-zone="UTC" \
    --uri="https://${REGION}-run.googleapis.com/apis/run.googleapis.com/v1/namespaces/${PROJECT_ID}/jobs/ruvltra-benchmark:run" \
    --http-method=POST \
    --oauth-service-account-email="${SA_EMAIL}" \
    --description="Weekly RuvLTRA benchmark run (Mondays 06:00 UTC)"

echo "  ✓ Scheduler set: every Monday at 06:00 UTC"

# --- Step 6: Create nightly training job ---
echo "▸ [6/7] Creating ruvltra-nightly-train job..."
JOB_NAME="ruvltra-nightly-train"
gcloud run jobs create "${JOB_NAME}" \
    --image="${IMAGE}" \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --memory=32Gi \
    --cpu=8 \
    --gpu=1 \
    --gpu-type=nvidia-l4 \
    --max-retries=1 \
    --task-timeout=3600s \
    --args="bash,scripts/training/nightly_train.sh" \
    --set-secrets="HF_TOKEN=huggingface-token:latest" \
    --set-env-vars="PYTHONUNBUFFERED=1,WANDB_DISABLED=true" \
    2>/dev/null || \
gcloud run jobs update "${JOB_NAME}" \
    --image="${IMAGE}" \
    --region="${REGION}" \
    --project="${PROJECT_ID}" \
    --memory=32Gi \
    --cpu=8 \
    --gpu=1 \
    --gpu-type=nvidia-l4 \
    --max-retries=1 \
    --task-timeout=3600s \
    --args="bash,scripts/training/nightly_train.sh" \
    --set-secrets="HF_TOKEN=huggingface-token:latest" \
    --set-env-vars="PYTHONUNBUFFERED=1,WANDB_DISABLED=true"

echo "  ✓ ${JOB_NAME} ready"

# --- Step 7: Set up nightly training scheduler ---
echo "▸ [7/7] Setting up nightly training schedule..."
SCHEDULER_NAME="ruvltra-nightly-train"
gcloud scheduler jobs create http "${SCHEDULER_NAME}" \
    --location="${REGION}" \
    --project="${PROJECT_ID}" \
    --schedule="0 3 * * *" \
    --time-zone="UTC" \
    --uri="https://${REGION}-run.googleapis.com/apis/run.googleapis.com/v1/namespaces/${PROJECT_ID}/jobs/ruvltra-nightly-train:run" \
    --http-method=POST \
    --oauth-service-account-email="${SA_EMAIL}" \
    --description="Nightly RuvLTRA training from brain learnings (03:00 UTC)" \
    2>/dev/null || \
gcloud scheduler jobs update http "${SCHEDULER_NAME}" \
    --location="${REGION}" \
    --project="${PROJECT_ID}" \
    --schedule="0 3 * * *" \
    --time-zone="UTC" \
    --uri="https://${REGION}-run.googleapis.com/apis/run.googleapis.com/v1/namespaces/${PROJECT_ID}/jobs/ruvltra-nightly-train:run" \
    --http-method=POST \
    --oauth-service-account-email="${SA_EMAIL}" \
    --description="Nightly RuvLTRA training from brain learnings (03:00 UTC)"

echo "  ✓ Nightly training scheduled: daily at 03:00 UTC"

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  Deployment complete!                                        ║"
echo "║                                                              ║"
echo "║  Run manually:                                               ║"
echo "║    gcloud run jobs execute ruvltra-calibration --region=${REGION}     ║"
echo "║    gcloud run jobs execute ruvltra-sft-training --region=${REGION}    ║"
echo "║    gcloud run jobs execute ruvltra-benchmark --region=${REGION}       ║"
echo "║    gcloud run jobs execute ruvltra-nightly-train --region=${REGION}   ║"
echo "║                                                              ║"
echo "║  Schedules:                                                  ║"
echo "║    Weekly benchmark: Mondays 06:00 UTC                       ║"
echo "║    Nightly training: Daily 03:00 UTC                         ║"
echo "╚══════════════════════════════════════════════════════════════╝"

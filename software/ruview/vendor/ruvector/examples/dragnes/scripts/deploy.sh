#!/usr/bin/env bash
set -euo pipefail

# DrAgnes Cloud Run Deployment Script
PROJECT_ID="${GCP_PROJECT_ID:-ruv-dev}"
REGION="${GCP_REGION:-us-central1}"
IMAGE="gcr.io/${PROJECT_ID}/dragnes:latest"

echo "Building DrAgnes container..."
docker build -t "${IMAGE}" .

echo "Pushing to GCR..."
docker push "${IMAGE}"

echo "Deploying to Cloud Run..."
gcloud run services replace cloud-run.yaml \
  --project="${PROJECT_ID}" \
  --region="${REGION}"

echo "Done. Service URL:"
gcloud run services describe dragnes \
  --project="${PROJECT_ID}" \
  --region="${REGION}" \
  --format='value(status.url)'

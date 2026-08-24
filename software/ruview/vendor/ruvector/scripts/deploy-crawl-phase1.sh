#!/bin/bash
# Deploy Common Crawl Phase 1 scheduler jobs (ADR-118)
# Medical domain only: $11-28/month budget
#
# Auth: The brain server uses Bearer token auth (any token 8-256 chars).
#       Cloud Scheduler sends the token via --headers instead of OIDC
#       because Cloud Run's allow-unauthenticated + brain's own auth
#       means OIDC JWTs (>256 chars) get rejected.
set -euo pipefail

PROJECT="${1:-ruv-dev}"
REGION="us-central1"
BRAIN_URL="https://pi.ruv.io"
BEARER_TOKEN="brain-crawl-phase1-scheduler"

echo "=== Common Crawl Phase 1 Deployment ==="
echo "Project: ${PROJECT}"
echo "Budget: \$11-28/month"
echo ""

# Job 1: Medical domain crawl (daily 2AM, 100 pages — API caps at 100/request)
echo "Creating/updating brain-crawl-medical..."
gcloud scheduler jobs create http brain-crawl-medical \
  --project="${PROJECT}" \
  --location="${REGION}" \
  --schedule="0 2 * * *" \
  --uri="${BRAIN_URL}/v1/pipeline/crawl/discover" \
  --http-method=POST \
  --headers="Content-Type=application/json,Authorization=Bearer ${BEARER_TOKEN}" \
  --message-body='{"domain_pattern":"*.pubmed.ncbi.nlm.nih.gov/*","crawl_index":"CC-MAIN-2026-08","limit":100,"category":"medical","tags":["pubmed","medical","phase-1"],"inject":true}' \
  --description="Phase 1: Medical domain crawl (ADR-118)" \
  2>/dev/null || \
gcloud scheduler jobs update http brain-crawl-medical \
  --project="${PROJECT}" \
  --location="${REGION}" \
  --schedule="0 2 * * *" \
  --uri="${BRAIN_URL}/v1/pipeline/crawl/discover" \
  --http-method=POST \
  --update-headers="Content-Type=application/json,Authorization=Bearer ${BEARER_TOKEN}" \
  --message-body='{"domain_pattern":"*.pubmed.ncbi.nlm.nih.gov/*","crawl_index":"CC-MAIN-2026-08","limit":100,"category":"medical","tags":["pubmed","medical","phase-1"],"inject":true}' \
  --description="Phase 1: Medical domain crawl (ADR-118)"

echo "  brain-crawl-medical OK"

# Job 2: Dermatology crawl (daily 3AM, 50 pages)
echo "Creating/updating brain-crawl-derm..."
gcloud scheduler jobs create http brain-crawl-derm \
  --project="${PROJECT}" \
  --location="${REGION}" \
  --schedule="0 3 * * *" \
  --uri="${BRAIN_URL}/v1/pipeline/crawl/discover" \
  --http-method=POST \
  --headers="Content-Type=application/json,Authorization=Bearer ${BEARER_TOKEN}" \
  --message-body='{"domain_pattern":"*.dermnetnz.org/*","crawl_index":"CC-MAIN-2026-08","limit":50,"category":"medical","tags":["dermatology","skin-cancer","phase-1"],"inject":true}' \
  --description="Phase 1: Dermatology domain crawl (ADR-118)" \
  2>/dev/null || \
gcloud scheduler jobs update http brain-crawl-derm \
  --project="${PROJECT}" \
  --location="${REGION}" \
  --schedule="0 3 * * *" \
  --uri="${BRAIN_URL}/v1/pipeline/crawl/discover" \
  --http-method=POST \
  --update-headers="Content-Type=application/json,Authorization=Bearer ${BEARER_TOKEN}" \
  --message-body='{"domain_pattern":"*.dermnetnz.org/*","crawl_index":"CC-MAIN-2026-08","limit":50,"category":"medical","tags":["dermatology","skin-cancer","phase-1"],"inject":true}' \
  --description="Phase 1: Dermatology domain crawl (ADR-118)"

echo "  brain-crawl-derm OK"

# Job 3: Partition cache recompute (hourly)
echo "Creating/updating brain-partition-cache..."
gcloud scheduler jobs create http brain-partition-cache \
  --project="${PROJECT}" \
  --location="${REGION}" \
  --schedule="5 * * * *" \
  --uri="${BRAIN_URL}/v1/pipeline/optimize" \
  --http-method=POST \
  --headers="Content-Type=application/json,Authorization=Bearer ${BEARER_TOKEN}" \
  --message-body='{"actions":["rebuild_graph"]}' \
  --description="Hourly partition cache recompute (ADR-118)" \
  2>/dev/null || \
gcloud scheduler jobs update http brain-partition-cache \
  --project="${PROJECT}" \
  --location="${REGION}" \
  --schedule="5 * * * *" \
  --uri="${BRAIN_URL}/v1/pipeline/optimize" \
  --http-method=POST \
  --update-headers="Content-Type=application/json,Authorization=Bearer ${BEARER_TOKEN}" \
  --message-body='{"actions":["rebuild_graph"]}' \
  --description="Hourly partition cache recompute (ADR-118)"

echo "  brain-partition-cache OK"

echo ""
echo "=== Deployed Jobs ==="
gcloud scheduler jobs list --project="${PROJECT}" --location="${REGION}" --filter="name~brain-crawl OR name~brain-partition" 2>/dev/null
echo ""
echo "=== Verification ==="
echo "Pipeline metrics:"
curl -s "${BRAIN_URL}/v1/pipeline/metrics" | python3 -c "import sys,json; d=json.load(sys.stdin); print(json.dumps(d, indent=2))" 2>/dev/null || echo "(no metrics yet)"
echo ""
echo "Crawl stats:"
curl -s "${BRAIN_URL}/v1/pipeline/crawl/stats" | python3 -c "import sys,json; d=json.load(sys.stdin); print(json.dumps(d, indent=2))" 2>/dev/null || echo "(no crawl stats yet)"
echo ""
echo "Brain status:"
curl -s "${BRAIN_URL}/v1/status" | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'Memories: {d[\"total_memories\"]}, Graph: {d[\"graph_edges\"]} edges, Sparsifier: {d[\"sparsifier_compression\"]:.1f}x')"
echo ""
echo "Phase 1 deployed. Estimated cost: \$11-28/month."
echo "Run 'gcloud scheduler jobs run brain-crawl-medical --project=${PROJECT} --location=${REGION}' to trigger immediately."

#!/bin/bash
# Historical Common Crawl evolutionary comparison importer
# Queries the same medical domains across quarterly crawl snapshots 2020-2026
# ADR-119 implementation
set -euo pipefail

BRAIN_URL="${BRAIN_URL:-https://pi.ruv.io}"
AUTH_HEADER="Authorization: Bearer ruvector-crawl-2026"
LIMIT="${LIMIT:-50}"  # pages per domain per crawl

# Target domains for medical/dermatology evolution tracking
DOMAINS=(
  "aad.org"
  "dermnetnz.org"
  "skincancer.org"
  "cancer.org"
  "melanoma.org"
)

# Quarterly crawl snapshots (2020-2026)
CRAWLS=(
  "CC-MAIN-2020-16"
  "CC-MAIN-2020-50"
  "CC-MAIN-2021-17"
  "CC-MAIN-2021-43"
  "CC-MAIN-2022-05"
  "CC-MAIN-2022-33"
  "CC-MAIN-2023-06"
  "CC-MAIN-2023-40"
  "CC-MAIN-2024-10"
  "CC-MAIN-2024-42"
  "CC-MAIN-2025-13"
  "CC-MAIN-2025-40"
  "CC-MAIN-2026-06"
  "CC-MAIN-2026-08"
)

echo "=== Historical Common Crawl Evolutionary Import ==="
echo "Brain: ${BRAIN_URL}"
echo "Domains: ${#DOMAINS[@]}"
echo "Crawls: ${#CRAWLS[@]} quarterly snapshots (2020-2026)"
echo "Limit: ${LIMIT} pages/domain/crawl"
echo ""

TOTAL_IMPORTED=0
TOTAL_ERRORS=0

for crawl in "${CRAWLS[@]}"; do
  echo "--- Crawl: ${crawl} ---"

  for domain in "${DOMAINS[@]}"; do
    echo -n "  ${domain}: "

    # Call the brain's crawl discover endpoint
    RESULT=$(curl -s -X POST "${BRAIN_URL}/v1/pipeline/crawl/discover" \
      -H "Content-Type: application/json" \
      -H "${AUTH_HEADER}" \
      -d "{
        \"domain_pattern\": \"*.${domain}/*\",
        \"crawl_index\": \"${crawl}\",
        \"limit\": ${LIMIT},
        \"filters\": {\"language\": \"en\", \"min_length\": 500}
      }" \
      --max-time 60 2>/dev/null || echo '{"error":"timeout"}')

    # Parse result
    DISCOVERED=$(echo "$RESULT" | python3 -c "
import sys,json
try:
    d=json.load(sys.stdin)
    if 'error' in d:
        print(f'ERROR: {d[\"error\"]}')
    else:
        count = d.get('discovered', d.get('total', d.get('returned', 0)))
        print(f'{count} pages')
except:
    print('parse error')
" 2>/dev/null || echo "?")

    echo "${DISCOVERED}"

    # Rate limit: 2 seconds between requests
    sleep 2
  done

  echo ""
done

echo "=== Import Complete ==="
echo ""

# Check final brain state
echo "=== Brain State After Import ==="
curl -s "${BRAIN_URL}/v1/status" | python3 -c "
import sys,json; d=json.load(sys.stdin)
print(f'Memories: {d[\"total_memories\"]}')
print(f'Graph: {d[\"graph_edges\"]} edges')
print(f'Sparsifier: {d[\"sparsifier_compression\"]:.1f}x')
print(f'Clusters: {d[\"cluster_count\"]}')
"

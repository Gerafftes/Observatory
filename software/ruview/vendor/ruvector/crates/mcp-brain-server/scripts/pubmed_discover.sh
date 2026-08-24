#!/usr/bin/env bash
# PubMed Discovery Pipeline — π.ruv.io
#
# Fetches biomedical abstracts from PubMed, processes through the RuVector
# web memory pipeline, and reports emerging topics + contradictions.
#
# Usage:
#   ./scripts/pubmed_discover.sh "CRISPR gene therapy 2026"
#   ./scripts/pubmed_discover.sh "cancer immunotherapy" 200
#   ./scripts/pubmed_discover.sh "alzheimer amyloid" 100 --push
#
# Arguments:
#   $1  PubMed search query (required)
#   $2  Max results (default: 100)
#   $3  --push to also push discoveries to π.ruv.io

set -euo pipefail

QUERY="${1:?Usage: $0 <pubmed-query> [max-results] [--push]}"
MAX_RESULTS="${2:-100}"
PUSH_FLAG="${3:-}"

BRAIN_API="https://pi.ruv.io"
ESEARCH="https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi"
EFETCH="https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OUTPUT_DIR="${SCRIPT_DIR}/../data/pubmed"
mkdir -p "$OUTPUT_DIR"

echo "═══════════════════════════════════════════════════════════════"
echo "  π.ruv.io PubMed Discovery Pipeline"
echo "  Query: ${QUERY}"
echo "  Max results: ${MAX_RESULTS}"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# ── Step 1: Search PubMed for PMIDs ─────────────────────────────────
echo "[1/5] Searching PubMed..."
ENCODED_QUERY=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${QUERY}'))" 2>/dev/null || echo "${QUERY// /+}")

SEARCH_RESULT=$(curl -s "${ESEARCH}?db=pubmed&term=${ENCODED_QUERY}&retmax=${MAX_RESULTS}&retmode=json&sort=date")
PMID_COUNT=$(echo "$SEARCH_RESULT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d.get('esearchresult',{}).get('idlist',[])))" 2>/dev/null || echo "0")
PMIDS=$(echo "$SEARCH_RESULT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(','.join(d.get('esearchresult',{}).get('idlist',[])))" 2>/dev/null || echo "")

if [ -z "$PMIDS" ] || [ "$PMID_COUNT" = "0" ]; then
    echo "  No results found for query: ${QUERY}"
    exit 1
fi
echo "  Found ${PMID_COUNT} articles"

# ── Step 2: Fetch abstracts ─────────────────────────────────────────
echo "[2/5] Fetching abstracts from PubMed..."
sleep 0.35  # NCBI rate limit

ABSTRACTS_FILE="${OUTPUT_DIR}/abstracts_$(date +%Y%m%d_%H%M%S).xml"
curl -s "${EFETCH}?db=pubmed&id=${PMIDS}&rettype=xml&retmode=xml" > "$ABSTRACTS_FILE"

ARTICLE_COUNT=$(grep -c "<PubmedArticle>" "$ABSTRACTS_FILE" 2>/dev/null || echo "0")
echo "  Downloaded ${ARTICLE_COUNT} articles → ${ABSTRACTS_FILE}"

# ── Step 3: Parse and analyze ───────────────────────────────────────
echo "[3/5] Parsing articles and extracting features..."

# Parse XML to JSON using Python (lightweight, no deps beyond stdlib)
PARSED_FILE="${OUTPUT_DIR}/parsed_$(date +%Y%m%d_%H%M%S).json"
python3 - "$ABSTRACTS_FILE" "$PARSED_FILE" << 'PYEOF'
import json, sys, re
from xml.etree import ElementTree as ET

input_file = sys.argv[1]
output_file = sys.argv[2]

tree = ET.parse(input_file)
root = tree.getroot()
articles = []

for article_elem in root.findall('.//PubmedArticle'):
    mc = article_elem.find('.//MedlineCitation')
    if mc is None:
        continue

    pmid_elem = mc.find('.//PMID')
    pmid = pmid_elem.text if pmid_elem is not None else ''

    title_elem = mc.find('.//ArticleTitle')
    title = ''.join(title_elem.itertext()) if title_elem is not None else ''

    # Abstract (may have multiple segments)
    abstract_parts = []
    for at in mc.findall('.//AbstractText'):
        label = at.get('Label', '')
        text = ''.join(at.itertext()).strip()
        if label:
            abstract_parts.append(f"{label}: {text}")
        else:
            abstract_parts.append(text)
    abstract_text = ' '.join(abstract_parts)

    # Authors
    authors = []
    for author in mc.findall('.//Author'):
        last = author.findtext('LastName', '')
        first = author.findtext('ForeName', '')
        if last:
            authors.append(f"{last} {first}".strip())

    # Journal
    journal = mc.findtext('.//Journal/Title', '')

    # Date
    year = mc.findtext('.//PubDate/Year', '')
    month = mc.findtext('.//PubDate/Month', '')
    date = mc.findtext('.//PubDate/MedlineDate', '') or f"{year} {month}".strip()

    # MeSH terms
    mesh = [d.text for d in mc.findall('.//DescriptorName') if d.text]

    # References
    refs = []
    pd = article_elem.find('.//PubmedData')
    if pd is not None:
        for aid in pd.findall('.//ArticleId[@IdType="pubmed"]'):
            if aid.text and aid.text != pmid:
                refs.append(aid.text)

    if pmid and (title or abstract_text):
        articles.append({
            'pmid': pmid,
            'title': title,
            'abstract': abstract_text,
            'authors': authors,
            'journal': journal,
            'date': date,
            'mesh_terms': mesh,
            'references': refs,
        })

with open(output_file, 'w') as f:
    json.dump(articles, f, indent=2)

print(f"  Parsed {len(articles)} articles with abstracts")

# Quick analysis
mesh_counts = {}
for a in articles:
    for m in a['mesh_terms']:
        mesh_counts[m] = mesh_counts.get(m, 0) + 1

print(f"  Unique MeSH terms: {len(mesh_counts)}")
top_mesh = sorted(mesh_counts.items(), key=lambda x: -x[1])[:10]
if top_mesh:
    print("  Top MeSH terms:")
    for term, count in top_mesh:
        print(f"    {count:4d}  {term}")
PYEOF

# ── Step 4: Discover patterns ──────────────────────────────────────
echo ""
echo "[4/5] Running discovery analysis..."

python3 - "$PARSED_FILE" << 'PYEOF'
import json, sys, hashlib
from collections import Counter, defaultdict

with open(sys.argv[1]) as f:
    articles = json.load(f)

print(f"\n{'='*65}")
print(f"  DISCOVERY REPORT — {len(articles)} articles analyzed")
print(f"{'='*65}")

# ── Emerging Topics (articles with rare/new MeSH combinations) ──
mesh_counts = Counter()
for a in articles:
    for m in a['mesh_terms']:
        mesh_counts[m] += 1

# Articles with rare MeSH terms (appear ≤2 times) are more novel
novel_articles = []
for a in articles:
    rare_terms = [m for m in a['mesh_terms'] if mesh_counts[m] <= 2]
    if len(rare_terms) >= 2:
        novel_articles.append((len(rare_terms), a, rare_terms))
novel_articles.sort(key=lambda x: -x[0])

print(f"\n{'─'*65}")
print(f"  EMERGING TOPICS (articles with rare MeSH combinations)")
print(f"{'─'*65}")
for i, (score, art, terms) in enumerate(novel_articles[:10]):
    print(f"\n  [{i+1}] PMID {art['pmid']} (novelty: {score} rare terms)")
    print(f"      {art['title'][:100]}")
    print(f"      Rare terms: {', '.join(terms[:5])}")
    if art['date']:
        print(f"      Published: {art['date']}")

# ── Contradiction Signals ──
# Articles sharing MeSH terms but with opposing keywords
print(f"\n{'─'*65}")
print(f"  POTENTIAL CONTRADICTIONS")
print(f"{'─'*65}")

positive_words = {'effective', 'beneficial', 'improved', 'positive', 'success',
                  'enhanced', 'protective', 'increased', 'promotes', 'supports'}
negative_words = {'ineffective', 'harmful', 'no effect', 'negative', 'failed',
                  'decreased', 'inhibits', 'reduces', 'contrary', 'contradicts',
                  'no significant', 'no association', 'no evidence'}

contradictions = []
for i, a in enumerate(articles):
    for j in range(i+1, min(len(articles), i+50)):
        b = articles[j]
        shared = set(a['mesh_terms']) & set(b['mesh_terms'])
        if len(shared) >= 2:
            text_a = (a['title'] + ' ' + a['abstract']).lower()
            text_b = (b['title'] + ' ' + b['abstract']).lower()
            pos_a = any(w in text_a for w in positive_words)
            neg_a = any(w in text_a for w in negative_words)
            pos_b = any(w in text_b for w in positive_words)
            neg_b = any(w in text_b for w in negative_words)
            # One positive + one negative on same topic
            if (pos_a and neg_b) or (neg_a and pos_b):
                contradictions.append((len(shared), a, b, list(shared)))

contradictions.sort(key=lambda x: -x[0])
if contradictions:
    for i, (score, a, b, shared) in enumerate(contradictions[:5]):
        print(f"\n  [{i+1}] Shared terms ({score}): {', '.join(shared[:4])}")
        print(f"      A: PMID {a['pmid']} — {a['title'][:80]}")
        print(f"      B: PMID {b['pmid']} — {b['title'][:80]}")
else:
    print("  No contradictions detected in this batch.")

# ── Citation Network ──
print(f"\n{'─'*65}")
print(f"  CITATION NETWORK")
print(f"{'─'*65}")

ref_counts = Counter()
pmid_set = {a['pmid'] for a in articles}
for a in articles:
    for ref in a['references']:
        ref_counts[ref] += 1

# Most cited within this corpus
internal_refs = [(pmid, count) for pmid, count in ref_counts.most_common(20)
                 if pmid in pmid_set]
if internal_refs:
    print(f"  Most cited within corpus:")
    for pmid, count in internal_refs[:5]:
        art = next((a for a in articles if a['pmid'] == pmid), None)
        title = art['title'][:70] if art else '(not in batch)'
        print(f"    {count:3d} citations  PMID {pmid} — {title}")

# ── Journal Distribution ──
print(f"\n{'─'*65}")
print(f"  JOURNAL DISTRIBUTION")
print(f"{'─'*65}")
journal_counts = Counter(a['journal'] for a in articles if a['journal'])
for journal, count in journal_counts.most_common(10):
    print(f"    {count:4d}  {journal[:60]}")

# ── Timeline ──
print(f"\n{'─'*65}")
print(f"  PUBLICATION TIMELINE")
print(f"{'─'*65}")
year_counts = Counter()
for a in articles:
    year = ''.join(c for c in a['date'][:4] if c.isdigit())
    if len(year) == 4:
        year_counts[year] += 1
for year in sorted(year_counts.keys())[-5:]:
    bar = '█' * min(year_counts[year], 50)
    print(f"    {year}: {bar} ({year_counts[year]})")

print(f"\n{'='*65}")
print(f"  Pipeline complete. Data saved to {sys.argv[1]}")
print(f"{'='*65}")
PYEOF

# ── Step 5: Push to π.ruv.io (optional) ────────────────────────────
if [ "$PUSH_FLAG" = "--push" ]; then
    echo ""
    echo "[5/5] Pushing discoveries to π.ruv.io..."

    python3 - "$PARSED_FILE" "$BRAIN_API" << 'PYEOF'
import json, sys, time
import urllib.request

with open(sys.argv[1]) as f:
    articles = json.load(f)

brain_api = sys.argv[2]
pushed = 0
failed = 0

for art in articles:
    if len(art['abstract']) < 50:
        continue

    tags = art['mesh_terms'][:10]
    tags.append(f"pmid:{art['pmid']}")
    tags.append(f"journal:{art['journal']}")
    tags.append("source:pubmed")
    if art['date']:
        tags.append(f"date:{art['date']}")

    body = json.dumps({
        "category": "pattern",
        "title": art['title'],
        "content": art['abstract'],
        "tags": tags,
    }).encode()

    req = urllib.request.Request(
        f"{brain_api}/v1/memories",
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )

    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            if resp.status < 300:
                pushed += 1
            else:
                failed += 1
    except Exception as e:
        failed += 1

    time.sleep(0.05)  # Rate limit

    if (pushed + failed) % 25 == 0:
        print(f"    Progress: {pushed} pushed, {failed} failed of {pushed+failed}")

print(f"  Pushed {pushed} discoveries to π.ruv.io ({failed} failed)")
PYEOF
else
    echo ""
    echo "[5/5] Skipping push to π.ruv.io (use --push to enable)"
fi

echo ""
echo "Done. Raw data: ${OUTPUT_DIR}/"

#!/usr/bin/env bash
#
# swarm_train_15.sh - 15-agent concurrent discovery + training swarm
#
# Runs 15 parallel data fetchers targeting ALL untrained/undertrained domains,
# then trains them into pi.ruv.io brain concurrently.
#
# Usage: ./scripts/swarm_train_15.sh [--train-only] [--discover-only]
#
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="${REPO_ROOT}/examples/data/discoveries"
BRAIN_API="https://pi.ruv.io"
BRAIN_API_KEY="${BRAIN_API_KEY:-ruvector-discovery-trainer-benevolent}"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
DATE_TODAY=$(date -u +"%Y-%m-%d")
DATE_WEEK_AGO=$(date -u -d "7 days ago" +"%Y-%m-%d" 2>/dev/null || date -u -v-7d +"%Y-%m-%d" 2>/dev/null || echo "2026-03-08")

# Colors
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; MAGENTA='\033[0;35m'; BOLD='\033[1m'; NC='\033[0m'

log_info()  { echo -e "${CYAN}[INFO]${NC}  $(date '+%H:%M:%S') $*"; }
log_ok()    { echo -e "${GREEN}[OK]${NC}    $(date '+%H:%M:%S') $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $(date '+%H:%M:%S') $*"; }
log_fail()  { echo -e "${RED}[FAIL]${NC}  $(date '+%H:%M:%S') $*"; }
log_phase() { echo -e "\n${MAGENTA}${BOLD}═══ $* ═══${NC}\n"; }

TRAIN_ONLY=false
DISCOVER_ONLY=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --train-only) TRAIN_ONLY=true; shift ;;
        --discover-only) DISCOVER_ONLY=true; shift ;;
        *) shift ;;
    esac
done

mkdir -p "$OUTPUT_DIR"

# ─────────────────────────────────────────────────────────────
# BRAIN HELPERS
# ─────────────────────────────────────────────────────────────
get_nonce() {
    curl -sf --max-time 10 "${BRAIN_API}/v1/challenge" 2>/dev/null | jq -r '.nonce // empty'
}

train_entry() {
    local title="$1" content="$2" tags_json="$3"
    local nonce
    nonce=$(get_nonce) || return 1
    local payload
    payload=$(jq -n --arg t "$title" --arg c "$content" --argjson tags "$tags_json" \
        '{ title: $t, content: $c, category: "pattern", tags: $tags }')
    local http_code
    http_code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 15 \
        -X POST "${BRAIN_API}/v1/memories" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${BRAIN_API_KEY}" \
        -H "X-Challenge-Nonce: ${nonce}" \
        -d "$payload" 2>/dev/null) || http_code=0
    [[ "$http_code" =~ ^2 ]]
}

train_file() {
    local filepath="$1" agent_id="$2"
    local file_len
    file_len=$(jq 'length' "$filepath" 2>/dev/null) || file_len=0
    [[ "$file_len" -eq 0 ]] && return 0

    local trained=0 failed=0 idx=0
    while [[ $idx -lt $file_len ]]; do
        local title content tags_json
        title=$(jq -r ".[$idx].title // \"Discovery $idx\"" "$filepath")
        content=$(jq -r ".[$idx].content // \"\"" "$filepath")
        tags_json=$(jq -c ".[$idx].tags // [\"discovery\"]" "$filepath")

        if train_entry "$title" "$content" "$tags_json"; then
            trained=$((trained + 1))
            echo -e "  ${GREEN}✓${NC} [$agent_id] $title"
        else
            failed=$((failed + 1))
            echo -e "  ${RED}✗${NC} [$agent_id] $title"
        fi
        sleep 0.5
        idx=$((idx + 1))
    done
    echo "  [$agent_id] Done: $trained trained, $failed failed"
}

# ─────────────────────────────────────────────────────────────
# 15 DISCOVERY AGENT SCRIPTS (each writes to its own temp script)
# ─────────────────────────────────────────────────────────────

make_agent_script() {
    local id="$1" name="$2" outfile="$3"
    shift 3
    local body="$*"

    cat > "/tmp/rv_agent_${id}.sh" <<AGENTEOF
#!/usr/bin/env bash
set -uo pipefail
TIMESTAMP="$TIMESTAMP"
DATE_TODAY="$DATE_TODAY"
DATE_WEEK_AGO="$DATE_WEEK_AGO"
OUTPUT_DIR="$OUTPUT_DIR"
out="$outfile"

echo "[]" > "\$out"
$body
echo "AGENT_${id}_DONE \$(jq 'length' "\$out" 2>/dev/null || echo 0)"
AGENTEOF
    chmod +x "/tmp/rv_agent_${id}.sh"
}

# ─────────────────────────────────────────────────────────────
# Agent bodies
# ─────────────────────────────────────────────────────────────

# Agent 1: NASA NEOs
make_agent_script 1 "NEOs" "$OUTPUT_DIR/swarm_neos.json" '
data=$(curl -sf --max-time 20 "https://api.nasa.gov/neo/rest/v1/feed?start_date=${DATE_TODAY}&end_date=${DATE_TODAY}&api_key=DEMO_KEY" 2>/dev/null) || exit 0
echo "$data" | jq --arg ts "$TIMESTAMP" "[.near_earth_objects[][] | select(.is_potentially_hazardous_asteroid == true or ((.close_approach_data[0].miss_distance.kilometers | tonumber) < 7500000)) | {title: (\"NEO: \" + .name + (if .is_potentially_hazardous_asteroid then \" [HAZARDOUS]\" else \"\" end)), content: (.name + \" at \" + .close_approach_data[0].miss_distance.kilometers + \"km. Vel: \" + .close_approach_data[0].relative_velocity.kilometers_per_hour + \"km/h. Diam: \" + (.estimated_diameter.meters.estimated_diameter_max | tostring) + \"m\"), category: \"anomaly\", tags: [\"space\",\"neo\",\"asteroid\",(if .is_potentially_hazardous_asteroid then \"hazardous\" else \"close-approach\" end)], domain: \"space-science\", source_api: \"NASA NEO\", timestamp: \$ts, confidence: 0.88, data_points: 1}]" > "$out" 2>/dev/null || echo "[]" > "$out"
'

# Agent 2: USGS Earthquakes
make_agent_script 2 "Earthquakes" "$OUTPUT_DIR/swarm_earthquakes.json" '
data=$(curl -sf --max-time 20 "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/4.5_week.geojson" 2>/dev/null) || exit 0
echo "$data" | jq --arg ts "$TIMESTAMP" "[.features[] | select((.properties.mag // 0) >= 5.0 or (.geometry.coordinates[2] // 0) > 200) | {title: (\"M\" + (.properties.mag | tostring) + \" \" + (.properties.place // \"?\")), content: (\"M\" + (.properties.mag | tostring) + \" at \" + (.properties.place // \"?\") + \". Depth: \" + ((.geometry.coordinates[2] // 0) | tostring) + \"km\"), category: \"anomaly\", tags: [\"earth\",\"seismic\",\"earthquake\",(if (.geometry.coordinates[2] // 0) > 300 then \"deep-focus\" else \"shallow\" end)], domain: \"earth-science\", source_api: \"USGS\", timestamp: \$ts, confidence: (if (.properties.mag // 0) >= 7 then 0.98 else 0.85 end), data_points: 1}] | .[:10]" > "$out" 2>/dev/null || echo "[]" > "$out"
'

# Agent 3: NOAA Solar Flares
make_agent_script 3 "Solar" "$OUTPUT_DIR/swarm_solar.json" '
data=$(curl -sf --max-time 15 "https://services.swpc.noaa.gov/json/goes/primary/xray-flares-latest.json" 2>/dev/null) || exit 0
echo "$data" | jq --arg ts "$TIMESTAMP" "[.[] | select(.max_class != null) | select(.max_class | test(\"^[CMX]\")) | {title: (\"Solar flare: \" + .max_class), content: (.max_class + \"-class. Begin: \" + (.begin_time // \"?\") + \", peak: \" + (.max_time // \"?\")), category: \"anomaly\", tags: [\"space\",\"solar\",\"flare\"], domain: \"space-science\", source_api: \"NOAA SWPC\", timestamp: \$ts, confidence: 0.85, data_points: 1}] | .[:5]" > "$out" 2>/dev/null || echo "[]" > "$out"
'

# Agent 4: LIGO Gravitational Waves
make_agent_script 4 "GW" "$OUTPUT_DIR/swarm_gw.json" '
data=$(curl -sf --max-time 15 "https://gracedb.ligo.org/api/superevents/?query=far+%3C+1e-6&format=json&count=8" 2>/dev/null) || exit 0
echo "$data" | jq --arg ts "$TIMESTAMP" "[.superevents[]? | {title: (\"GW event: \" + (.superevent_id // \"?\")), content: (\"Superevent \" + (.superevent_id // \"?\") + \" cat=\" + (.category // \"?\") + \" FAR=\" + ((.far // 0) | tostring) + \"Hz. Preferred: \" + (.preferred_event // \"?\")), category: \"anomaly\", tags: [\"space\",\"gravitational-wave\",\"ligo\",\"gap-fill\"], domain: \"space-science\", source_api: \"LIGO GraceDB\", timestamp: \$ts, confidence: 0.92, data_points: 1}]" > "$out" 2>/dev/null || echo "[]" > "$out"
'

# Agent 5: DONKI CMEs
make_agent_script 5 "CME" "$OUTPUT_DIR/swarm_cme.json" '
data=$(curl -sf --max-time 15 "https://api.nasa.gov/DONKI/CME?startDate=${DATE_WEEK_AGO}&endDate=${DATE_TODAY}&api_key=DEMO_KEY" 2>/dev/null) || exit 0
echo "$data" | jq --arg ts "$TIMESTAMP" "[.[]? | {title: (\"CME: \" + (.activityID // \"?\")), content: (\"CME \" + (.activityID // \"?\") + \". Time: \" + (.startTime // \"?\") + \". Source: \" + (.sourceLocation // \"?\")), category: \"anomaly\", tags: [\"space\",\"cme\",\"solar\",\"gap-fill\"], domain: \"space-science\", source_api: \"NASA DONKI\", timestamp: \$ts, confidence: 0.87, data_points: 1}] | .[:8]" > "$out" 2>/dev/null || echo "[]" > "$out"
'

# Agent 6: DONKI Geomagnetic Storms
make_agent_script 6 "GeoStorm" "$OUTPUT_DIR/swarm_geostorm.json" '
data=$(curl -sf --max-time 15 "https://api.nasa.gov/DONKI/GST?startDate=${DATE_WEEK_AGO}&endDate=${DATE_TODAY}&api_key=DEMO_KEY" 2>/dev/null) || exit 0
if echo "$data" | jq -e "type == \"array\" and length > 0" &>/dev/null; then
  echo "$data" | jq --arg ts "$TIMESTAMP" "[.[] | {title: (\"Geomagnetic storm: \" + (.gstID // \"?\")), content: (\"Storm \" + (.gstID // \"?\") + \". Start: \" + (.startTime // \"?\")), category: \"anomaly\", tags: [\"earth\",\"geomagnetic\",\"storm\",\"space-weather\"], domain: \"earth-science\", source_api: \"NASA DONKI\", timestamp: \$ts, confidence: 0.90, data_points: 1}]" > "$out" 2>/dev/null || echo "[]" > "$out"
fi
'

# Agent 7: NCBI Genes
make_agent_script 7 "Genomics" "$OUTPUT_DIR/swarm_genomics.json" '
entries="[]"
for gene in BRCA1 TP53 APOE CFTR HBB; do
    gene_id=$(curl -sf --max-time 10 "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=gene&term=${gene}[sym]+AND+human[orgn]&retmax=1&retmode=json" 2>/dev/null | jq -r ".esearchresult.idlist[0] // empty" 2>/dev/null) || continue
    [ -z "$gene_id" ] && continue
    summary=$(curl -sf --max-time 10 "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=gene&id=${gene_id}&retmode=json" 2>/dev/null) || continue
    desc=$(echo "$summary" | jq -r ".result.\"${gene_id}\".description // \"unknown\"" 2>/dev/null)
    chrom=$(echo "$summary" | jq -r ".result.\"${gene_id}\".chromosome // \"?\"" 2>/dev/null)
    entries=$(echo "$entries" | jq --arg t "Gene: ${gene} (${desc})" --arg c "Human gene ${gene}: ${desc}. Chr${chrom}. NCBI:${gene_id}." --arg ts "$TIMESTAMP" ". + [{title:\$t,content:\$c,category:\"pattern\",tags:[\"genomics\",\"gene\",\"ncbi\",\"gap-fill\"],domain:\"medical-genomics\",source_api:\"NCBI Gene\",timestamp:\$ts,confidence:0.90,data_points:1}]")
    sleep 0.3
done
echo "$entries" | jq "." > "$out"
'

# Agent 8: UniProt Proteins
make_agent_script 8 "Proteins" "$OUTPUT_DIR/swarm_proteins.json" '
entries="[]"
for protein in insulin hemoglobin collagen keratin albumin; do
    data=$(curl -sf --max-time 15 "https://rest.uniprot.org/uniprotkb/search?query=${protein}+AND+organism_id:9606&format=json&size=2" 2>/dev/null) || continue
    if echo "$data" | jq -e ".results | length > 0" &>/dev/null; then
        new=$(echo "$data" | jq --arg ts "$TIMESTAMP" --arg q "$protein" "[.results[] | {title: (\"Protein: \" + (.proteinDescription.recommendedName.fullName.value // \$q)), content: (\"UniProt \" + (.primaryAccession // \"?\") + \": \" + (.proteinDescription.recommendedName.fullName.value // \$q) + \". Gene: \" + ((.genes[0].geneName.value // \"?\"))), category: \"pattern\", tags: [\"genomics\",\"protein\",\"uniprot\",\"gap-fill\"], domain: \"medical-genomics\", source_api: \"UniProt\", timestamp: \$ts, confidence: 0.88, data_points: 1}]" 2>/dev/null) || continue
        entries=$(echo "$entries" "$new" | jq -s "flatten")
    fi
    sleep 0.3
done
echo "$entries" | jq "." > "$out"
'

# Agent 9: CrossRef Materials Science
make_agent_script 9 "CrossRef" "$OUTPUT_DIR/swarm_crossref.json" '
data=$(curl -sf --max-time 15 "https://api.crossref.org/works?query=materials+science+discovery&rows=8&sort=published&order=desc&mailto=ruvector@example.org" 2>/dev/null) || exit 0
echo "$data" | jq --arg ts "$TIMESTAMP" "[.message.items[]? | {title: (\"CrossRef: \" + (.title[0] // \"?\")), content: ((.title[0] // \"?\") + \". Publisher: \" + (.publisher // \"?\") + \". DOI: \" + (.DOI // \"?\")), category: \"pattern\", tags: [\"academic\",\"crossref\",\"materials\",\"gap-fill\"], domain: \"materials-physics\", source_api: \"CrossRef\", timestamp: \$ts, confidence: 0.80, data_points: 1}]" > "$out" 2>/dev/null || echo "[]" > "$out"
'

# Agent 10: World Bank Economics
make_agent_script 10 "Economics" "$OUTPUT_DIR/swarm_economics.json" '
entries="[]"
for indicator in NY.GDP.MKTP.CD FP.CPI.TOTL.ZG SL.UEM.TOTL.ZS BX.KLT.DINV.CD.WD SE.XPD.TOTL.GD.ZS; do
    data=$(curl -sf --max-time 15 "https://api.worldbank.org/v2/country/US;CN/indicator/${indicator}?format=json&date=2022:2025&per_page=4" 2>/dev/null) || continue
    if echo "$data" | jq -e ".[1] | length > 0" &>/dev/null; then
        new=$(echo "$data" | jq --arg ts "$TIMESTAMP" "[.[1][] | select(.value != null) | {title: (\"WorldBank: \" + (.indicator.value // \"?\") + \" \" + (.country.value // \"?\") + \" \" + (.date // \"?\")), content: ((.indicator.value // \"?\") + \" for \" + (.country.value // \"?\") + \" (\" + (.date // \"?\") + \"): \" + (.value | tostring)), category: \"pattern\", tags: [\"economics\",\"worldbank\",(.country.id // \"?\"),\"indicator\",\"gap-fill\"], domain: \"economics-finance\", source_api: \"World Bank\", timestamp: \$ts, confidence: 0.90, data_points: 1}]" 2>/dev/null) || continue
        entries=$(echo "$entries" "$new" | jq -s "flatten")
    fi
    sleep 0.3
done
echo "$entries" | jq "." > "$out"
'

# Agent 11: CERN Open Data
make_agent_script 11 "CERN" "$OUTPUT_DIR/swarm_physics.json" '
data=$(curl -sf --max-time 15 "https://opendata.cern.ch/api/records/?q=Higgs+boson&size=5&type=Dataset" 2>/dev/null) || exit 0
if echo "$data" | jq -e ".hits.hits | length > 0" &>/dev/null; then
    echo "$data" | jq --arg ts "$TIMESTAMP" "[.hits.hits[] | {title: (\"CERN: \" + (._source.title // \"?\")), content: ((._source.title // \"?\") + \". Experiment: \" + (._source.experiment // \"?\")), category: \"pattern\", tags: [\"physics\",\"cern\",\"particle\",\"higgs\",\"gap-fill\"], domain: \"materials-physics\", source_api: \"CERN Open Data\", timestamp: \$ts, confidence: 0.88, data_points: 1}]" > "$out" 2>/dev/null || echo "[]" > "$out"
fi
'

# Agent 12: PubChem Materials
make_agent_script 12 "Materials" "$OUTPUT_DIR/swarm_materials.json" '
entries="[]"
for compound in graphene perovskite titanium-dioxide silicon-carbide lithium-cobalt-oxide; do
    data=$(curl -sf --max-time 10 "https://pubchem.ncbi.nlm.nih.gov/rest/pug/compound/name/${compound}/property/MolecularFormula,MolecularWeight,IUPACName/JSON" 2>/dev/null) || continue
    if echo "$data" | jq -e ".PropertyTable.Properties | length > 0" &>/dev/null; then
        new=$(echo "$data" | jq --arg ts "$TIMESTAMP" --arg q "$compound" "[.PropertyTable.Properties[] | {title: (\"Material: \" + (.IUPACName // \$q)), content: ((.IUPACName // \$q) + \". Formula: \" + (.MolecularFormula // \"?\") + \". MW: \" + ((.MolecularWeight // 0) | tostring)), category: \"pattern\", tags: [\"materials\",\"chemistry\",\"pubchem\",\"gap-fill\"], domain: \"materials-physics\", source_api: \"PubChem\", timestamp: \$ts, confidence: 0.85, data_points: 1}]" 2>/dev/null) || continue
        entries=$(echo "$entries" "$new" | jq -s "flatten")
    fi
    sleep 0.3
done
echo "$entries" | jq "." > "$out"
'

# Agent 13: PubMed Medical (expanded)
make_agent_script 13 "Medical" "$OUTPUT_DIR/swarm_medical.json" '
entries="[]"
for query in "cancer+immunotherapy+breakthrough" "CRISPR+gene+therapy" "antibiotic+resistance+novel" "neurodegeneration+biomarker" "mRNA+vaccine+2025"; do
    ids=$(curl -sf --max-time 15 "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=pubmed&term=${query}&retmax=3&sort=date&retmode=json" 2>/dev/null | jq -r ".esearchresult.idlist[]" 2>/dev/null) || continue
    for pmid in $ids; do
        art=$(curl -sf --max-time 10 "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id=${pmid}&retmode=json" 2>/dev/null) || continue
        t=$(echo "$art" | jq -r ".result.\"${pmid}\".title // empty" 2>/dev/null)
        s=$(echo "$art" | jq -r ".result.\"${pmid}\".source // \"unknown\"" 2>/dev/null)
        [ -z "$t" ] && continue
        entries=$(echo "$entries" | jq --arg t "PubMed: $t" --arg c "Medical: ${t}. Journal: ${s}. PMID: ${pmid}." --arg ts "$TIMESTAMP" ". + [{title:\$t,content:\$c,category:\"pattern\",tags:[\"medical\",\"pubmed\",\"research\",\"gap-fill\"],domain:\"medical-genomics\",source_api:\"PubMed\",timestamp:\$ts,confidence:0.85,data_points:1}]")
        sleep 0.2
    done
    sleep 0.3
done
echo "$entries" | jq "." > "$out"
'

# Agent 14: DONKI IPS (Interplanetary Shocks)
make_agent_script 14 "IPS" "$OUTPUT_DIR/swarm_ips.json" '
data=$(curl -sf --max-time 15 "https://api.nasa.gov/DONKI/IPS?startDate=${DATE_WEEK_AGO}&endDate=${DATE_TODAY}&api_key=DEMO_KEY" 2>/dev/null) || exit 0
if echo "$data" | jq -e "type == \"array\" and length > 0" &>/dev/null; then
    echo "$data" | jq --arg ts "$TIMESTAMP" "[.[] | {title: (\"IPS: \" + (.catalog // \"?\")), content: (\"Interplanetary shock at \" + (.eventTime // \"?\") + \". Location: \" + (.location // \"?\")), category: \"anomaly\", tags: [\"space\",\"interplanetary-shock\",\"solar-wind\",\"gap-fill\"], domain: \"space-science\", source_api: \"NASA DONKI\", timestamp: \$ts, confidence: 0.85, data_points: 1}] | .[:5]" > "$out" 2>/dev/null || echo "[]" > "$out"
fi
'

# Agent 15: DONKI Solar Energetic Particles
make_agent_script 15 "SEP" "$OUTPUT_DIR/swarm_sep.json" '
data=$(curl -sf --max-time 15 "https://api.nasa.gov/DONKI/SEP?startDate=${DATE_WEEK_AGO}&endDate=${DATE_TODAY}&api_key=DEMO_KEY" 2>/dev/null) || exit 0
if echo "$data" | jq -e "type == \"array\" and length > 0" &>/dev/null; then
    echo "$data" | jq --arg ts "$TIMESTAMP" "[.[] | {title: (\"SEP: \" + (.eventTime // \"?\")), content: (\"Solar energetic particle event at \" + (.eventTime // \"?\") + \". Instruments: \" + ((.instruments // [{}])[0].displayName // \"?\")), category: \"anomaly\", tags: [\"space\",\"sep\",\"solar-particle\",\"gap-fill\"], domain: \"space-science\", source_api: \"NASA DONKI\", timestamp: \$ts, confidence: 0.83, data_points: 1}] | .[:5]" > "$out" 2>/dev/null || echo "[]" > "$out"
fi
'

# ─────────────────────────────────────────────────────────────
# MAIN: 15-AGENT CONCURRENT SWARM
# ─────────────────────────────────────────────────────────────

main() {
    echo ""
    echo -e "${BOLD}╔═══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}║  RuVector 15-Agent Discovery + Training Swarm                ║${NC}"
    echo -e "${BOLD}║  Date: ${DATE_TODAY} | Agents: 15 parallel                       ║${NC}"
    echo -e "${BOLD}╚═══════════════════════════════════════════════════════════════╝${NC}"
    echo ""

    local start_time=$SECONDS

    if [[ "$TRAIN_ONLY" != "true" ]]; then
        log_phase "PHASE 1: DISCOVER (15 agents parallel)"

        local pids=()
        for i in $(seq 1 15); do
            bash "/tmp/rv_agent_${i}.sh" > "/tmp/rv_agent_${i}.log" 2>&1 &
            pids+=($!)
        done

        log_info "Launched 15 agents, waiting..."

        local success=0 fail=0
        for pid in "${pids[@]}"; do
            if wait "$pid" 2>/dev/null; then
                success=$((success + 1))
            else
                fail=$((fail + 1))
            fi
        done

        local discover_time=$((SECONDS - start_time))
        log_ok "Discovery: ${success} ok, ${fail} failed in ${discover_time}s"

        # Show agent results
        for i in $(seq 1 15); do
            local result
            result=$(grep "AGENT_${i}_DONE" "/tmp/rv_agent_${i}.log" 2>/dev/null | tail -1) || true
            local count
            count=$(echo "$result" | awk '{print $2}') || count=0
            [[ -z "$count" || "$count" == "0" ]] && count=0
            local names=("" "NEOs" "Earthquakes" "Solar" "GravWaves" "CMEs" "GeoStorms" "Genomics" "Proteins" "CrossRef" "Economics" "CERN" "Materials" "Medical" "IPS" "SEP")
            echo -e "  Agent ${i} (${names[$i]:-?}): ${count} entries"
        done
    fi

    if [[ "$DISCOVER_ONLY" != "true" ]]; then
        log_phase "PHASE 2: TRAIN (upload to brain)"

        local total_discoveries=0
        for f in "$OUTPUT_DIR"/swarm_*.json; do
            [[ ! -f "$f" ]] && continue
            local c
            c=$(jq 'length' "$f" 2>/dev/null) || c=0
            total_discoveries=$((total_discoveries + c))
            [[ "$c" -gt 0 ]] && log_info "  $(basename "$f"): $c entries"
        done

        log_info "Total to train: $total_discoveries discoveries"

        # Train 5 files concurrently
        local batch=0
        local train_pids=()
        for f in "$OUTPUT_DIR"/swarm_*.json; do
            [[ ! -f "$f" ]] && continue
            local flen
            flen=$(jq 'length' "$f" 2>/dev/null) || flen=0
            [[ "$flen" -eq 0 ]] && continue

            batch=$((batch + 1))
            train_file "$f" "T${batch}" &
            train_pids+=($!)

            if [[ $((batch % 5)) -eq 0 ]]; then
                for tp in "${train_pids[@]}"; do wait "$tp" 2>/dev/null || true; done
                train_pids=()
            fi
        done
        for tp in "${train_pids[@]}"; do wait "$tp" 2>/dev/null || true; done

        log_ok "Training complete"
    fi

    # ─────────────────────────────────────────────────────────────
    # SUMMARY
    # ─────────────────────────────────────────────────────────────
    log_phase "SWARM COMPLETE"

    echo -e "${BOLD}Domain Coverage:${NC}"
    local grand_total=0
    for f in "$OUTPUT_DIR"/swarm_*.json; do
        [[ ! -f "$f" ]] && continue
        local name count
        name=$(basename "$f" .json | sed 's/swarm_//')
        count=$(jq 'length' "$f" 2>/dev/null) || count=0
        grand_total=$((grand_total + count))
        local bar=""
        for ((i=0; i<count && i<30; i++)); do bar+="█"; done
        printf "  %-20s %3d %s\n" "$name" "$count" "$bar"
    done

    echo ""
    log_ok "Grand total: $grand_total new discoveries"
    log_ok "Elapsed: $((SECONDS - start_time))s"

    # Cleanup temp scripts
    rm -f /tmp/rv_agent_*.sh /tmp/rv_agent_*.log
}

main "$@"

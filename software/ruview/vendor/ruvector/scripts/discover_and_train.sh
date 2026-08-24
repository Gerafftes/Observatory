#!/usr/bin/env bash
#
# discover_and_train.sh - Back-and-forth discovery ↔ training feedback loop
#
# Cycle:
#   1. DISCOVER: Fetch fresh data from live open APIs
#   2. TRAIN:    Upload discoveries to pi.ruv.io brain
#   3. REFLECT:  Query brain for gaps & learned patterns
#   4. REDISCOVER: Target gaps with focused queries
#   5. RETRAIN:  Feed gap-filling discoveries back to brain
#
# Usage: ./scripts/discover_and_train.sh [--cycles N] [--output-dir DIR]
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="${REPO_ROOT}/examples/data/discoveries"
BRAIN_API="https://pi.ruv.io"
BRAIN_API_KEY="${BRAIN_API_KEY:-ruvector-discovery-trainer-benevolent}"
MAX_CYCLES=2
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
DATE_TODAY=$(date -u +"%Y-%m-%d")
DATE_WEEK_AGO=$(date -u -d "7 days ago" +"%Y-%m-%d" 2>/dev/null || date -u -v-7d +"%Y-%m-%d" 2>/dev/null || echo "2026-03-08")

# Colors
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; MAGENTA='\033[0;35m'; NC='\033[0m'

log_info()  { echo -e "${CYAN}[INFO]${NC}  $(date '+%H:%M:%S') $*"; }
log_ok()    { echo -e "${GREEN}[OK]${NC}    $(date '+%H:%M:%S') $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $(date '+%H:%M:%S') $*"; }
log_fail()  { echo -e "${RED}[FAIL]${NC}  $(date '+%H:%M:%S') $*"; }
log_phase() { echo -e "\n${MAGENTA}═══════════════════════════════════════${NC}"; echo -e "${MAGENTA}  $*${NC}"; echo -e "${MAGENTA}═══════════════════════════════════════${NC}\n"; }

# Parse args
while [[ $# -gt 0 ]]; do
    case $1 in
        --cycles) MAX_CYCLES="$2"; shift 2 ;;
        --output-dir) OUTPUT_DIR="$2"; shift 2 ;;
        *) shift ;;
    esac
done

mkdir -p "$OUTPUT_DIR"

for cmd in curl jq; do
    if ! command -v "$cmd" &>/dev/null; then
        log_fail "$cmd is required but not found"
        exit 1
    fi
done

# ─────────────────────────────────────────────────────────────
# Helper: merge multiple JSON arrays from temp files into one
# ─────────────────────────────────────────────────────────────
merge_json_arrays() {
    local output="$1"
    shift
    # Merge all input files (each a JSON array) into one flat array
    jq -s 'flatten | [.[] | select(. != null)]' "$@" > "$output" 2>/dev/null || echo "[]" > "$output"
}

# ─────────────────────────────────────────────────────────────
# DISCOVER FUNCTIONS
# ─────────────────────────────────────────────────────────────

discover_space() {
    log_info "Fetching NASA Exoplanet Archive (recent discoveries)..."
    local out_file="$OUTPUT_DIR/live_space_discoveries.json"
    local tmp_exo="/tmp/rv_exo_$$.json"
    local tmp_neo="/tmp/rv_neo_$$.json"
    local tmp_solar="/tmp/rv_solar_$$.json"
    echo "[]" > "$tmp_exo"
    echo "[]" > "$tmp_neo"
    echo "[]" > "$tmp_solar"

    # NASA Exoplanet Archive TAP
    local exo_data
    exo_data=$(curl -sf --max-time 30 \
        "https://exoplanetarchive.ipac.caltech.edu/TAP/sync?query=SELECT+pl_name,pl_bmassj,pl_orbper,pl_orbeccen,pl_eqt,disc_year,discoverymethod,sy_dist+FROM+ps+WHERE+disc_year>=2025+AND+pl_bmassj+IS+NOT+NULL+ORDER+BY+disc_year+DESC&format=json" 2>/dev/null) || true

    if [[ -n "$exo_data" ]] && echo "$exo_data" | jq -e 'length > 0' &>/dev/null; then
        local num_planets
        num_planets=$(echo "$exo_data" | jq 'length')
        log_ok "Got $num_planets exoplanets from NASA"

        # Compute mean and stddev, then find z-score outliers — all in one jq call
        echo "$exo_data" | jq --arg ts "$TIMESTAMP" --argjson np "$num_planets" '
            [.[].pl_bmassj | select(. != null and . > 0)] as $masses |
            ($masses | add / length) as $mean |
            ($masses | map(pow(. - $mean; 2)) | add / length | sqrt) as $sd |
            [
                .[] | select(.pl_bmassj != null and .pl_bmassj > 0) |
                ((if .pl_bmassj > $mean then .pl_bmassj - $mean else $mean - .pl_bmassj end) / (if $sd > 0.001 then $sd else 0.001 end)) as $z |
                select($z > 2.0) |
                {
                    title: ("Anomalous exoplanet: " + (.pl_name // "unknown") + " (" + ($z * 10 | floor / 10 | tostring) + "σ mass outlier)"),
                    content: ("Planet " + (.pl_name // "unknown") + " has mass " + (.pl_bmassj | tostring) + " Mj (" + ($z * 10 | floor / 10 | tostring) + "σ from mean " + ($mean * 100 | floor / 100 | tostring) + "±" + ($sd * 100 | floor / 100 | tostring) + "). Period: " + ((.pl_orbper // 0) | tostring) + "d. Ecc: " + ((.pl_orbeccen // 0) | tostring) + ". Teq: " + ((.pl_eqt // 0) | tostring) + "K. Method: " + (.discoverymethod // "unknown") + "."),
                    category: "anomaly",
                    tags: ["space", "exoplanet", "anomaly", "mass-outlier", (.discoverymethod // "unknown")],
                    domain: "space-science",
                    source_api: "NASA Exoplanet Archive TAP",
                    timestamp: $ts,
                    confidence: ([$z / 5.0, 0.99] | min),
                    data_points: $np
                }
            ]
        ' > "$tmp_exo" 2>/dev/null || echo "[]" > "$tmp_exo"

        local nexo
        nexo=$(jq 'length' "$tmp_exo")
        log_ok "  Found $nexo exoplanet anomalies"
    fi

    sleep 1

    # NASA NEO
    log_info "Fetching NASA Near-Earth Objects..."
    local neo_data
    neo_data=$(curl -sf --max-time 20 \
        "https://api.nasa.gov/neo/rest/v1/feed?start_date=${DATE_TODAY}&end_date=${DATE_TODAY}&api_key=DEMO_KEY" 2>/dev/null) || true

    if [[ -n "$neo_data" ]]; then
        echo "$neo_data" | jq --arg ts "$TIMESTAMP" '
            [
                .near_earth_objects[][] |
                select(.is_potentially_hazardous_asteroid == true or
                       (.close_approach_data[0].miss_distance.kilometers | tonumber) < 5000000) |
                {
                    title: ("NEO close approach: " + .name + (if .is_potentially_hazardous_asteroid then " [HAZARDOUS]" else "" end)),
                    content: ("Asteroid " + .name + " passes Earth at " + .close_approach_data[0].miss_distance.kilometers + " km (" + ((.close_approach_data[0].miss_distance.kilometers | tonumber / 384400 * 100 | floor / 100) | tostring) + " LD). Velocity: " + .close_approach_data[0].relative_velocity.kilometers_per_hour + " km/h. Diameter: " + (.estimated_diameter.meters.estimated_diameter_max | tostring) + "m."),
                    category: "anomaly",
                    tags: ["space", "neo", "asteroid", (if .is_potentially_hazardous_asteroid then "hazardous" else "close-approach" end)],
                    domain: "space-science",
                    source_api: "NASA NEO API",
                    timestamp: $ts,
                    confidence: (if .is_potentially_hazardous_asteroid then 0.95 else 0.80 end),
                    data_points: 1
                }
            ]
        ' > "$tmp_neo" 2>/dev/null || echo "[]" > "$tmp_neo"

        local nneo
        nneo=$(jq 'length' "$tmp_neo")
        log_ok "  Found $nneo NEO entries"
    fi

    sleep 1

    # NOAA solar flares
    log_info "Fetching NOAA solar weather..."
    local solar_data
    solar_data=$(curl -sf --max-time 15 \
        "https://services.swpc.noaa.gov/json/goes/primary/xray-flares-latest.json" 2>/dev/null) || true

    if [[ -n "$solar_data" ]]; then
        echo "$solar_data" | jq --arg ts "$TIMESTAMP" '
            [
                .[] | select(.max_class != null) |
                select(.max_class | startswith("M") or startswith("X")) |
                {
                    title: ("Solar flare: " + .max_class + "-class event"),
                    content: (.max_class + "-class solar X-ray flare. Begin: " + (.begin_time // "unknown") + ", peak: " + (.max_time // "unknown") + ". Flux: " + ((.max_xrlong // 0) | tostring) + " W/m2. " + (if (.max_class | startswith("X")) then "X-class: disrupts HF radio, GPS, power grids." else "M-class: brief HF radio blackouts at high latitudes." end)),
                    category: "anomaly",
                    tags: ["space", "solar", "flare", (.max_class | ascii_downcase)],
                    domain: "space-science",
                    source_api: "NOAA SWPC",
                    timestamp: $ts,
                    confidence: (if (.max_class | startswith("X")) then 0.98 else 0.85 end),
                    data_points: 1
                }
            ]
        ' > "$tmp_solar" 2>/dev/null || echo "[]" > "$tmp_solar"

        local nsolar
        nsolar=$(jq 'length' "$tmp_solar")
        log_ok "  Found $nsolar solar flare entries"
    fi

    merge_json_arrays "$out_file" "$tmp_exo" "$tmp_neo" "$tmp_solar"
    rm -f "$tmp_exo" "$tmp_neo" "$tmp_solar"

    local total
    total=$(jq 'length' "$out_file" 2>/dev/null || echo 0)
    log_ok "Space discoveries total: $total entries"
}

discover_earth() {
    log_info "Fetching USGS significant earthquakes..."
    local out_file="$OUTPUT_DIR/live_earth_discoveries.json"
    local tmp_quake="/tmp/rv_quake_$$.json"
    local tmp_storm="/tmp/rv_storm_$$.json"
    echo "[]" > "$tmp_quake"
    echo "[]" > "$tmp_storm"

    local quake_data
    quake_data=$(curl -sf --max-time 20 \
        "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/significant_month.geojson" 2>/dev/null) || true

    if [[ -n "$quake_data" ]]; then
        local num_quakes
        num_quakes=$(echo "$quake_data" | jq '.features | length' 2>/dev/null) || num_quakes=0
        log_ok "Got $num_quakes significant earthquakes"

        echo "$quake_data" | jq --arg ts "$TIMESTAMP" --argjson nq "$num_quakes" '
            [
                .features[] |
                select(.properties.mag >= 5.0) |
                {
                    title: ("M" + (.properties.mag | tostring) + " earthquake: " + (.properties.place // "unknown")),
                    content: ("Significant M" + (.properties.mag | tostring) + " at " + (.properties.place // "unknown") + ", depth " + ((.geometry.coordinates[2] // 0) | tostring) + " km. " + (if (.geometry.coordinates[2] // 0) > 300 then "Deep-focus: subduction zone dynamics." else "Shallow: higher surface impact." end) + " Tsunami: " + ((.properties.tsunami // 0) | tostring) + "."),
                    category: "anomaly",
                    tags: ["earth", "seismic", "earthquake", (if (.geometry.coordinates[2] // 0) > 300 then "deep-focus" else "shallow" end)],
                    domain: "earth-science",
                    source_api: "USGS Earthquake Hazards",
                    timestamp: $ts,
                    confidence: ([(.properties.mag / 10.0), 0.99] | min),
                    data_points: $nq
                }
            ]
        ' > "$tmp_quake" 2>/dev/null || echo "[]" > "$tmp_quake"
    fi

    sleep 1

    # DONKI geomagnetic storms
    log_info "Fetching NOAA DONKI geomagnetic storms..."
    local donki_data
    donki_data=$(curl -sf --max-time 15 \
        "https://api.nasa.gov/DONKI/GST?startDate=${DATE_WEEK_AGO}&endDate=${DATE_TODAY}&api_key=DEMO_KEY" 2>/dev/null) || true

    if [[ -n "$donki_data" ]] && echo "$donki_data" | jq -e 'type == "array" and length > 0' &>/dev/null; then
        log_ok "Got $(echo "$donki_data" | jq 'length') geomagnetic storms"

        echo "$donki_data" | jq --arg ts "$TIMESTAMP" '
            [
                .[] |
                {
                    title: ("Geomagnetic storm: " + (.gstID // "unknown")),
                    content: ("Storm " + (.gstID // "unknown") + ". Start: " + (.startTime // "unknown") + ". " + (if .allKpIndex then ("Peak Kp: " + ([.allKpIndex[].kpIndex] | max | tostring) + ". ") else "" end) + "Linked CMEs: " + (if .linkedEvents then ([.linkedEvents[].activityID] | join(", ")) else "none" end) + "."),
                    category: "anomaly",
                    tags: ["earth", "geomagnetic", "storm", "space-weather"],
                    domain: "earth-science",
                    source_api: "NASA DONKI",
                    timestamp: $ts,
                    confidence: 0.90,
                    data_points: 1
                }
            ]
        ' > "$tmp_storm" 2>/dev/null || echo "[]" > "$tmp_storm"
    fi

    merge_json_arrays "$out_file" "$tmp_quake" "$tmp_storm"
    rm -f "$tmp_quake" "$tmp_storm"

    local total
    total=$(jq 'length' "$out_file" 2>/dev/null || echo 0)
    log_ok "Earth discoveries total: $total entries"
}

discover_academic() {
    log_info "Fetching arXiv recent papers..."
    local out_file="$OUTPUT_DIR/live_academic_discoveries.json"
    local all_entries="[]"

    # Try arxiv.org directly (export.arxiv.org may have CDN issues)
    for category in "astro-ph" "cs.AI" "physics.gen-ph" "q-bio"; do
        local arxiv_data
        arxiv_data=$(curl -sf --max-time 20 \
            "https://arxiv.org/api/query?search_query=cat:${category}&sortBy=submittedDate&sortOrder=descending&max_results=3" 2>/dev/null) || \
        arxiv_data=$(curl -sf --max-time 20 \
            "http://export.arxiv.org/api/query?search_query=cat:${category}&sortBy=submittedDate&sortOrder=descending&max_results=3" 2>/dev/null) || true

        if [[ -n "$arxiv_data" ]] && echo "$arxiv_data" | grep -q '<entry>'; then
            local i=0

            while IFS= read -r title; do
                local summary link
                summary=$(echo "$arxiv_data" | grep -oP '<summary>\K[^<]+' | sed -n "$((i+1))p" | tr '\n' ' ' | sed 's/  */ /g' | head -c 400)
                link=$(echo "$arxiv_data" | grep -oP '<id>\K[^<]+' | tail -n +2 | sed -n "$((i+1))p")

                if [[ -n "$title" && "$title" != "ArXiv Query"* ]]; then
                    all_entries=$(echo "$all_entries" | jq \
                        --arg t "arXiv [$category]: $title" \
                        --arg c "Recent ${category} paper: ${title}. ${summary} URL: ${link}" \
                        --arg cat "$category" \
                        --arg ts "$TIMESTAMP" \
                        '. + [{
                            title: $t,
                            content: $c,
                            category: "pattern",
                            tags: ["academic", "arxiv", $cat, "research"],
                            domain: "academic-research",
                            source_api: "arXiv API",
                            timestamp: $ts,
                            confidence: 0.80,
                            data_points: 1
                        }]')
                fi
                i=$((i + 1))
                [[ $i -ge 3 ]] && break
            done < <(echo "$arxiv_data" | grep -oP '<title>\K[^<]+' | tail -n +2)
        else
            log_warn "  arXiv $category: no data (CDN/DNS issue)"
        fi
        sleep 1
    done

    echo "$all_entries" | jq '.' > "$out_file"
    local total
    total=$(jq 'length' "$out_file" 2>/dev/null || echo 0)
    log_ok "Academic discoveries: $total entries"
}

discover_economics() {
    log_info "Fetching FRED economic indicators..."
    local out_file="$OUTPUT_DIR/live_economics_discoveries.json"
    local entries="[]"
    local fred_key="${FRED_API_KEY:-}"

    # FRED requires a real 32-char API key (get free at fred.stlouisfed.org/docs/api/api_key.html)
    if [[ -z "$fred_key" ]]; then
        log_warn "  FRED_API_KEY not set — using World Bank API fallback"
        # Fallback: World Bank indicators
        local wb_data
        wb_data=$(curl -sf --max-time 15 \
            "https://api.worldbank.org/v2/country/US/indicator/NY.GDP.MKTP.CD?format=json&date=2023:2025&per_page=3" 2>/dev/null) || true

        if [[ -n "$wb_data" ]] && echo "$wb_data" | jq -e '.[1] | length > 0' &>/dev/null; then
            entries=$(echo "$wb_data" | jq --arg ts "$TIMESTAMP" '
                [
                    .[1][] | select(.value != null) |
                    {
                        title: ("World Bank: US GDP " + (.date // "unknown")),
                        content: ("US GDP (current USD): " + (.value | tostring) + " for " + (.date // "unknown") + ". Source: World Bank Development Indicators."),
                        category: "pattern",
                        tags: ["economics", "worldbank", "gdp", "indicator"],
                        domain: "economics-finance",
                        source_api: "World Bank API",
                        timestamp: $ts,
                        confidence: 0.90,
                        data_points: 1
                    }
                ]
            ' 2>/dev/null) || entries="[]"
        fi
    fi

    for series in "DGS10" "UNRATE" "CPIAUCSL" "GDP" "FEDFUNDS"; do
        [[ -z "$fred_key" ]] && continue

        local fred_data
        fred_data=$(curl -sf --max-time 15 \
            "https://api.stlouisfed.org/fred/series/observations?series_id=${series}&sort_order=desc&limit=2&file_type=json&api_key=${fred_key}" 2>/dev/null) || true

        if [[ -n "$fred_data" ]] && echo "$fred_data" | jq -e '.observations | length > 0' &>/dev/null; then
            local latest_val latest_date prev_val series_title
            latest_val=$(echo "$fred_data" | jq -r '.observations[0].value // "N/A"')
            latest_date=$(echo "$fred_data" | jq -r '.observations[0].date // "unknown"')
            prev_val=$(echo "$fred_data" | jq -r '.observations[1].value // "N/A"')

            case $series in
                DGS10) series_title="10-Year Treasury Yield" ;;
                UNRATE) series_title="US Unemployment Rate" ;;
                CPIAUCSL) series_title="Consumer Price Index" ;;
                GDP) series_title="US Gross Domestic Product" ;;
                FEDFUNDS) series_title="Federal Funds Rate" ;;
            esac

            if [[ "$latest_val" != "." && "$latest_val" != "N/A" ]]; then
                entries=$(echo "$entries" | jq \
                    --arg t "Economic indicator: ${series_title} (${series})" \
                    --arg c "${series_title}: latest ${latest_val} as of ${latest_date}. Previous: ${prev_val}. Source: FRED." \
                    --arg s "$series" \
                    --arg ts "$TIMESTAMP" \
                    '. + [{
                        title: $t,
                        content: $c,
                        category: "pattern",
                        tags: ["economics", "fred", $s, "indicator"],
                        domain: "economics-finance",
                        source_api: "FRED API",
                        timestamp: $ts,
                        confidence: 0.92,
                        data_points: 1
                    }]')
            fi
        fi
        sleep 1
    done

    echo "$entries" | jq '.' > "$out_file"
    local total
    total=$(jq 'length' "$out_file" 2>/dev/null || echo 0)
    log_ok "Economics discoveries: $total entries"
}

# ─────────────────────────────────────────────────────────────
# TRAIN - Upload discoveries to brain
# ─────────────────────────────────────────────────────────────

get_nonce() {
    curl -sf --max-time 10 "${BRAIN_API}/v1/challenge" 2>/dev/null | jq -r '.nonce // empty'
}

train_brain() {
    local file_pattern="${1:-live_*_discoveries.json}"
    local trained=0
    local failed=0

    shopt -s nullglob
    local files=("${OUTPUT_DIR}"/${file_pattern})
    shopt -u nullglob

    if [[ ${#files[@]} -eq 0 ]]; then
        log_warn "No discovery files matching: $file_pattern"
        return
    fi

    for filepath in "${files[@]}"; do
        local filename
        filename=$(basename "$filepath")
        local file_len
        file_len=$(jq 'length' "$filepath" 2>/dev/null) || file_len=0

        if [[ "$file_len" -eq 0 ]]; then
            log_warn "Skipping $filename - empty"
            continue
        fi

        log_info "Training from: $filename ($file_len entries)"

        local idx=0
        while [[ $idx -lt $file_len ]]; do
            local title content tags_json
            title=$(jq -r ".[$idx].title // \"Discovery $idx\"" "$filepath")
            content=$(jq -r ".[$idx].content // (.[$idx] | tostring)" "$filepath")
            tags_json=$(jq -c ".[$idx].tags // [\"discovery\"]" "$filepath")

            local nonce
            nonce=$(get_nonce) || { log_warn "No nonce"; idx=$((idx + 1)); continue; }

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

            if [[ "$http_code" =~ ^2 ]]; then
                trained=$((trained + 1))
                log_ok "  [${trained}] $title"
            else
                failed=$((failed + 1))
                log_fail "  ✗ $title (HTTP $http_code)"
            fi

            sleep 1
            idx=$((idx + 1))
        done
    done

    log_info "Training batch: $trained trained, $failed failed"
}

# ─────────────────────────────────────────────────────────────
# REFLECT - Query brain for patterns and gaps
# ─────────────────────────────────────────────────────────────

query_brain_patterns() {
    log_info "Querying brain for learned patterns..."
    local gaps_file="$OUTPUT_DIR/brain_gaps.json"

    local nonce
    nonce=$(get_nonce) || { log_warn "Cannot get nonce for reflection"; echo '{"underrepresented":["medical","materials","genomics","gravitational-wave"],"gap_analysis":"Using defaults"}' > "$gaps_file"; return; }

    local memories
    memories=$(curl -sf --max-time 15 \
        "${BRAIN_API}/v1/memories" \
        -H "Authorization: Bearer ${BRAIN_API_KEY}" \
        -H "X-Challenge-Nonce: ${nonce}" 2>/dev/null) || true

    if [[ -n "$memories" ]]; then
        local total_memories
        total_memories=$(echo "$memories" | jq 'if type == "array" then length else (.memories // []) | length end' 2>/dev/null) || total_memories=0
        log_ok "Brain has $total_memories total memories"

        # Analyze tag frequency to find underrepresented domains
        echo "$memories" | jq '
            (if type == "array" then . else (.memories // []) end) as $mems |
            [$mems[].tags // [] | .[]] |
            group_by(.) |
            map({tag: .[0], count: length}) |
            sort_by(.count) as $sorted |
            {
                total_memories: ($mems | length),
                underrepresented: [$sorted[:5][].tag],
                well_covered: [$sorted[-5:][].tag],
                gap_analysis: "Domains with fewest entries need more discovery focus"
            }
        ' > "$gaps_file" 2>/dev/null || echo '{"underrepresented":["medical","materials","genomics"],"gap_analysis":"Parse error, using defaults"}' > "$gaps_file"

        log_ok "Gap analysis:"
        jq '.' "$gaps_file" 2>/dev/null || true
    else
        log_warn "Brain unreachable — using default gap targets"
        echo '{"underrepresented":["medical","materials","genomics","gravitational-wave","deep-focus"],"gap_analysis":"Brain unreachable, using defaults"}' > "$gaps_file"
    fi
}

# ─────────────────────────────────────────────────────────────
# REDISCOVER - Targeted discovery based on gaps
# ─────────────────────────────────────────────────────────────

discover_gaps() {
    log_info "Running targeted gap-filling discovery..."
    local gaps_file="$OUTPUT_DIR/brain_gaps.json"
    local out_file="$OUTPUT_DIR/live_gap_discoveries.json"
    local entries="[]"

    local gaps
    gaps=$(jq -r '.underrepresented // [] | .[]' "$gaps_file" 2>/dev/null) || gaps="medical materials genomics"

    # PubMed gap fill
    if echo "$gaps" | grep -qiE "medical|genomics|pubmed|bio"; then
        log_info "Gap-fill: PubMed trending research..."
        local pubmed_ids
        pubmed_ids=$(curl -sf --max-time 20 \
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=pubmed&term=breakthrough+OR+novel+discovery&retmax=5&sort=date&retmode=json" 2>/dev/null | jq -r '.esearchresult.idlist[]' 2>/dev/null) || true

        for pmid in $pubmed_ids; do
            local article_data
            article_data=$(curl -sf --max-time 15 \
                "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id=${pmid}&retmode=json" 2>/dev/null) || true

            if [[ -n "$article_data" ]]; then
                local art_title art_source
                art_title=$(echo "$article_data" | jq -r ".result.\"${pmid}\".title // empty" 2>/dev/null)
                art_source=$(echo "$article_data" | jq -r ".result.\"${pmid}\".source // \"unknown\"" 2>/dev/null)

                if [[ -n "$art_title" ]]; then
                    entries=$(echo "$entries" | jq \
                        --arg t "PubMed: $art_title" \
                        --arg c "Medical/genomics paper: ${art_title}. Journal: ${art_source}. PMID: ${pmid}." \
                        --arg ts "$TIMESTAMP" \
                        '. + [{
                            title: $t,
                            content: $c,
                            category: "pattern",
                            tags: ["medical", "pubmed", "research", "gap-fill"],
                            domain: "medical-genomics",
                            source_api: "PubMed E-utilities",
                            timestamp: $ts,
                            confidence: 0.82,
                            data_points: 1
                        }]')
                fi
            fi
            sleep 0.5
        done
    fi

    sleep 1

    # Deep earthquake gap fill
    if echo "$gaps" | grep -qiE "earth|seismic|deep-focus|volcano"; then
        log_info "Gap-fill: USGS M4.5+ earthquakes (7 days)..."
        local eq_data
        eq_data=$(curl -sf --max-time 20 \
            "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/4.5_week.geojson" 2>/dev/null) || true

        if [[ -n "$eq_data" ]]; then
            local gap_quakes
            gap_quakes=$(echo "$eq_data" | jq --arg ts "$TIMESTAMP" '
                [
                    .features[] |
                    select((.geometry.coordinates[2] // 0) > 200 or (.properties.mag // 0) >= 6.0) |
                    {
                        title: ("M" + (.properties.mag | tostring) + " earthquake: " + (.properties.place // "unknown")),
                        content: ("M" + (.properties.mag | tostring) + " at " + (.properties.place // "unknown") + ". Depth: " + ((.geometry.coordinates[2] // 0) | tostring) + " km. " + (if (.geometry.coordinates[2] // 0) > 300 then "Deep-focus subduction event." elif (.properties.mag // 0) >= 7.0 then "Major earthquake." else "Significant event." end)),
                        category: "anomaly",
                        tags: ["earth", "seismic", "gap-fill", (if (.geometry.coordinates[2] // 0) > 300 then "deep-focus" else "significant" end)],
                        domain: "earth-science",
                        source_api: "USGS Earthquake Hazards",
                        timestamp: $ts,
                        confidence: 0.88,
                        data_points: 1
                    }
                ] | .[:5]
            ' 2>/dev/null) || gap_quakes="[]"

            entries=$(echo "$entries" "$gap_quakes" | jq -s 'flatten')
            log_ok "  Added $(echo "$gap_quakes" | jq 'length') deep/major earthquake entries"
        fi
    fi

    sleep 1

    # Gravitational wave gap fill
    if echo "$gaps" | grep -qiE "gravitational|wave|ligo|gw"; then
        log_info "Gap-fill: LIGO GraceDB..."
        local gw_data
        gw_data=$(curl -sf --max-time 15 \
            "https://gracedb.ligo.org/api/superevents/?query=far+%3C+1e-6&format=json&count=5" 2>/dev/null) || true

        if [[ -n "$gw_data" ]]; then
            local gw_entries
            gw_entries=$(echo "$gw_data" | jq --arg ts "$TIMESTAMP" '
                [
                    .superevents[]? |
                    {
                        title: ("Gravitational wave: " + (.superevent_id // "unknown")),
                        content: ("GW superevent " + (.superevent_id // "unknown") + " (category: " + (.category // "unknown") + "). FAR: " + ((.far // 0) | tostring) + " Hz. Preferred: " + (.preferred_event // "unknown") + ". LIGO/Virgo/KAGRA detection."),
                        category: "anomaly",
                        tags: ["space", "gravitational-wave", "ligo", "gap-fill"],
                        domain: "space-science",
                        source_api: "LIGO GraceDB",
                        timestamp: $ts,
                        confidence: 0.90,
                        data_points: 1
                    }
                ]
            ' 2>/dev/null) || gw_entries="[]"

            entries=$(echo "$entries" "$gw_entries" | jq -s 'flatten')
        fi
    fi

    echo "$entries" | jq '.' > "$out_file"
    local total
    total=$(jq 'length' "$out_file" 2>/dev/null || echo 0)
    log_ok "Gap-fill discoveries: $total entries"
}

# ─────────────────────────────────────────────────────────────
# MAIN FEEDBACK LOOP
# ─────────────────────────────────────────────────────────────

main() {
    echo ""
    echo "╔═══════════════════════════════════════════════════════════╗"
    echo "║  RuVector Discovery ↔ Training Feedback Loop             ║"
    echo "║  Cycles: ${MAX_CYCLES} | Date: ${DATE_TODAY}                        ║"
    echo "╚═══════════════════════════════════════════════════════════╝"
    echo ""

    for cycle in $(seq 1 "$MAX_CYCLES"); do
        log_phase "CYCLE ${cycle}/${MAX_CYCLES}"

        if [[ $cycle -eq 1 ]]; then
            # Phase 1: DISCOVER
            log_phase "Phase 1: DISCOVER (live API fetch)"
            discover_space
            discover_earth
            discover_academic
            discover_economics

            # Phase 2: TRAIN
            log_phase "Phase 2: TRAIN (upload to brain)"
            train_brain "live_*_discoveries.json"
        else
            # Phase 3: REFLECT
            log_phase "Phase 3: REFLECT (query brain for gaps)"
            query_brain_patterns

            # Phase 4: REDISCOVER
            log_phase "Phase 4: REDISCOVER (targeted gap-filling)"
            discover_gaps

            # Phase 5: RETRAIN
            log_phase "Phase 5: RETRAIN (gap-fill → brain)"
            train_brain "live_gap_discoveries.json"
        fi
    done

    # Final summary
    log_phase "FEEDBACK LOOP COMPLETE"

    local total_discoveries=0
    for f in "$OUTPUT_DIR"/live_*_discoveries.json; do
        if [[ -f "$f" ]]; then
            local c
            c=$(jq 'length' "$f" 2>/dev/null) || c=0
            total_discoveries=$((total_discoveries + c))
            log_info "  $(basename "$f"): $c entries"
        fi
    done

    log_ok "Total discoveries: $total_discoveries"

    if [[ -f "$OUTPUT_DIR/brain_gaps.json" ]]; then
        log_ok "Brain gap analysis:"
        jq '.' "$OUTPUT_DIR/brain_gaps.json" 2>/dev/null || true
    fi
}

main "$@"

#!/usr/bin/env bash
# =============================================================================
# RuVector Training Orchestrator
# Interactive CLI for managing the pi.ruv.io brain API
#
# Provides 6 modes:
#   1. Discovery Scanner      - Scan local discovery JSON files
#   2. Brain Gap Analysis      - Query brain for high-novelty domains
#   3. Batch Upload            - Upload discoveries with nonce auth + PII strip
#   4. Training & Optimization - Trigger training, view SONA stats
#   5. Cross-Domain Discovery  - Find connections via drift & partition
#   6. Interactive Explorer    - Search brain memories
#
# Usage:
#   PI=<token> ./scripts/training_orchestrator.sh [--help] [--dry-run]
#
# Environment:
#   PI              Bearer token for brain API authentication
#   DISCOVERIES_DIR Override default discoveries directory
# =============================================================================
set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
BRAIN_API="https://pi.ruv.io"
DISCOVERIES_DIR="${DISCOVERIES_DIR:-$(cd "$(dirname "$0")/.." && pwd)/examples/data/discoveries}"
DRY_RUN=false

# ---------------------------------------------------------------------------
# ANSI color codes
# ---------------------------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

# ---------------------------------------------------------------------------
# Logging helpers
# ---------------------------------------------------------------------------
log_info()  { echo -e "  ${CYAN}[INFO]${NC}  $(date '+%H:%M:%S') $*"; }
log_ok()    { echo -e "  ${GREEN}[ OK ]${NC}  $(date '+%H:%M:%S') $*"; }
log_fail()  { echo -e "  ${RED}[FAIL]${NC}  $(date '+%H:%M:%S') $*"; }
log_warn()  { echo -e "  ${YELLOW}[WARN]${NC}  $(date '+%H:%M:%S') $*"; }
log_head()  { echo -e "\n  ${BOLD}${MAGENTA}=== $* ===${NC}\n"; }

# ---------------------------------------------------------------------------
# CLI flags
# ---------------------------------------------------------------------------
show_help() {
    cat <<'HELPTEXT'
RuVector Training Orchestrator - Interactive brain training CLI

USAGE:
    PI=<token> ./scripts/training_orchestrator.sh [OPTIONS]

OPTIONS:
    --help, -h   Show this help message and exit
    --dry-run    Simulate uploads without sending data to the API

ENVIRONMENT:
    PI               Bearer token for API authentication (required for modes 2-6)
    DISCOVERIES_DIR  Override the default discoveries directory

MODES (interactive menu):
    1  Discovery Scanner       Scan local JSON files, count entries, show domain coverage
    2  Brain Gap Analysis      Query /v1/explore for curiosity/novelty, find gaps
    3  Batch Upload            Upload discovery entries via /v1/memories with nonce auth
    4  Training & Optimization Trigger /v1/train, display SONA stats from /v1/sona/stats
    5  Cross-Domain Discovery  Query /v1/drift and /v1/partition for cross-domain links
    6  Interactive Explorer    Search brain memories via /v1/memories/search?q=QUERY

FEATURES:
    - PII stripping: emails, phone numbers, SSNs removed before upload
    - Progress bar for batch uploads
    - Colored terminal output
    - Graceful error handling on all API calls

EXAMPLES:
    PI=my-secret-token ./scripts/training_orchestrator.sh
    PI=my-secret-token ./scripts/training_orchestrator.sh --dry-run
HELPTEXT
    exit 0
}

for arg in "$@"; do
    case "$arg" in
        --help|-h) show_help ;;
        --dry-run)  DRY_RUN=true ;;
        *)          echo "Unknown option: $arg" >&2; show_help ;;
    esac
done

# ---------------------------------------------------------------------------
# Dependency checks
# ---------------------------------------------------------------------------
check_deps() {
    local missing=()
    for cmd in curl jq; do
        command -v "$cmd" &>/dev/null || missing+=("$cmd")
    done
    if [[ ${#missing[@]} -gt 0 ]]; then
        log_fail "Missing required tools: ${missing[*]}"
        echo "  Install with: sudo apt-get install -y ${missing[*]}"
        exit 1
    fi
}

# ---------------------------------------------------------------------------
# API helper - wraps curl with Bearer auth and error handling
# Returns the response body on success, prints error and returns 1 on failure
# ---------------------------------------------------------------------------
api_call() {
    local method="$1" endpoint="$2"
    shift 2
    local url="${BRAIN_API}${endpoint}"
    local -a headers=(-H "Content-Type: application/json")

    if [[ -n "${PI:-}" ]]; then
        headers+=(-H "Authorization: Bearer ${PI}")
    fi

    local response http_code body
    response=$(curl -s -w "\n%{http_code}" --max-time 15 \
        "${headers[@]}" -X "$method" "$@" "$url" 2>/dev/null) || {
        log_fail "Network error calling $method $url"
        return 1
    }

    http_code=$(echo "$response" | tail -1)
    body=$(echo "$response" | sed '$d')

    if [[ "$http_code" -ge 200 && "$http_code" -lt 300 ]]; then
        echo "$body"
        return 0
    else
        log_fail "HTTP $http_code on $method $endpoint"
        echo "$body" | jq . 2>/dev/null || echo "$body"
        return 1
    fi
}

# Checks that PI token is set; prints instructions if not
require_token() {
    if [[ -z "${PI:-}" ]]; then
        log_fail "PI environment variable not set."
        echo -e "  Export your bearer token first: ${BOLD}export PI=your-token${NC}"
        return 1
    fi
}

# ---------------------------------------------------------------------------
# PII stripping - remove emails, phone numbers, SSNs from text
# ---------------------------------------------------------------------------
strip_pii() {
    sed -E \
        -e 's/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/[REDACTED_EMAIL]/g' \
        -e 's/(\+?1?[-. ]?\(?[0-9]{3}\)?[-. ]?[0-9]{3}[-. ]?[0-9]{4})/[REDACTED_PHONE]/g' \
        -e 's/[0-9]{3}-[0-9]{2}-[0-9]{4}/[REDACTED_SSN]/g'
}

# ---------------------------------------------------------------------------
# Progress bar: progress_bar <current> <total>
# ---------------------------------------------------------------------------
progress_bar() {
    local current="$1" total="$2" width=40
    local pct=0 filled=0 empty=0
    if (( total > 0 )); then
        pct=$(( current * 100 / total ))
        filled=$(( current * width / total ))
    fi
    empty=$(( width - filled ))
    local bar_full="" bar_empty=""
    for (( i=0; i<filled; i++ )); do bar_full+="█"; done
    for (( i=0; i<empty; i++ )); do bar_empty+="░"; done
    printf "\r  ${GREEN}[%s${DIM}%s${NC}${GREEN}]${NC} %3d%% (%d/%d)" \
        "$bar_full" "$bar_empty" "$pct" "$current" "$total"
}

# ===========================================================================
# Mode 1: Discovery Scanner
# ===========================================================================
mode_discovery_scanner() {
    log_head "Discovery Scanner"

    if [[ ! -d "$DISCOVERIES_DIR" ]]; then
        log_fail "Discoveries directory not found: $DISCOVERIES_DIR"
        return 1
    fi

    local total_files=0 total_entries=0
    # Associative array for domain counts
    declare -A domain_counts

    echo -e "  ${BOLD}Scanning:${NC} ${DIM}${DISCOVERIES_DIR}${NC}\n"
    printf "  ${BOLD}%-45s %8s  %-30s${NC}\n" "FILE" "ENTRIES" "DOMAINS"
    printf "  %s\n" "$(printf '%.0s-' {1..85})"

    for f in "$DISCOVERIES_DIR"/*.json; do
        [[ -f "$f" ]] || continue
        local fname count domains_str
        fname=$(basename "$f")

        # Count entries (works for arrays and single objects)
        count=$(jq 'if type == "array" then length else 1 end' "$f" 2>/dev/null || echo 0)

        # Extract unique domains
        domains_str=$(jq -r '
            if type == "array" then
                [.[].domain // "unknown"] | unique | join(", ")
            else
                .domain // "unknown"
            end
        ' "$f" 2>/dev/null || echo "parse-error")

        # Track domain counts per file
        while IFS=',' read -ra doms; do
            for d in "${doms[@]}"; do
                d=$(echo "$d" | xargs)  # trim whitespace
                [[ -n "$d" ]] && domain_counts["$d"]=$(( ${domain_counts["$d"]:-0} + 1 ))
            done
        done <<< "$domains_str"

        printf "  ${CYAN}%-45s${NC} ${GREEN}%8d${NC}  %s\n" "$fname" "$count" "$domains_str"
        total_files=$((total_files + 1))
        total_entries=$((total_entries + count))
    done

    printf "  %s\n" "$(printf '%.0s-' {1..85})"
    echo -e "\n  ${BOLD}Summary:${NC}"
    echo -e "    Files scanned:  ${GREEN}${total_files}${NC}"
    echo -e "    Total entries:  ${GREEN}${total_entries}${NC}"
    echo -e "    Unique domains: ${GREEN}${#domain_counts[@]}${NC}"

    if [[ ${#domain_counts[@]} -gt 0 ]]; then
        echo -e "\n  ${BOLD}Domain Coverage:${NC}"
        # Sort domains by count descending
        for domain in $(
            for k in "${!domain_counts[@]}"; do
                echo "${domain_counts[$k]} $k"
            done | sort -rn | awk '{print $2}'
        ); do
            local cnt=${domain_counts[$domain]}
            printf "    ${BLUE}%-30s${NC} %d file(s)\n" "$domain" "$cnt"
        done
    fi
}

# ===========================================================================
# Mode 2: Brain Gap Analysis
# ===========================================================================
mode_gap_analysis() {
    log_head "Brain Gap Analysis"
    require_token || return 1

    log_info "Querying brain exploration data from /v1/explore..."
    local explore_data
    explore_data=$(api_call GET "/v1/explore") || {
        log_fail "Could not reach /v1/explore"
        return 1
    }

    echo -e "\n  ${BOLD}Curiosity & Novelty Landscape:${NC}\n"

    # Parse the explore response - handles various response shapes
    # Extract domain/novelty/curiosity tuples and display as bar chart
    echo "$explore_data" | jq -r '
        # Normalize different response shapes into lines of "domain\tnovelty\tcuriosity"
        if type == "object" then
            if .domains then
                .domains | to_entries[] |
                "\(.key)\t\(.value.novelty // .value.score // 0)\t\(.value.curiosity // 0)"
            elif .explorations then
                .explorations[] |
                "\(.domain // .topic // "unknown")\t\(.novelty // 0)\t\(.curiosity // 0)"
            elif .clusters then
                .clusters[] |
                "\(.label // .category // "unknown")\t\(.novelty // .coherence // 0)\t\(.curiosity // 0)"
            else
                to_entries[] |
                "\(.key)\t\(if (.value | type) == "number" then .value else 0 end)\t0"
            end
        elif type == "array" then
            .[] |
            "\(.domain // .topic // .label // "unknown")\t\(.novelty // 0)\t\(.curiosity // 0)"
        else empty end
    ' 2>/dev/null | sort -t$'\t' -k2 -rn | head -20 | while IFS=$'\t' read -r domain novelty curiosity; do
        # Build a visual bar proportional to novelty score
        local bar_len color
        bar_len=$(printf '%.0f' "$(echo "$novelty * 30" | bc -l 2>/dev/null || echo 5)")
        [[ "$bar_len" -gt 30 ]] && bar_len=30
        [[ "$bar_len" -lt 1 ]] && bar_len=1
        local bar=""
        for (( i=0; i<bar_len; i++ )); do bar+="█"; done

        # Color by novelty threshold
        if (( $(echo "$novelty > 0.7" | bc -l 2>/dev/null || echo 0) )); then
            color="$RED"
        elif (( $(echo "$novelty > 0.4" | bc -l 2>/dev/null || echo 0) )); then
            color="$YELLOW"
        else
            color="$GREEN"
        fi
        printf "    ${BOLD}%-25s${NC} ${color}%-30s${NC} novelty=%-6s curiosity=%s\n" \
            "$domain" "$bar" "$novelty" "$curiosity"
    done

    echo ""
    echo -e "  ${BOLD}Legend:${NC}"
    echo -e "    ${RED}█${NC} High novelty (>0.7) = domain needs more content"
    echo -e "    ${YELLOW}█${NC} Medium novelty (0.4-0.7) = partially covered"
    echo -e "    ${GREEN}█${NC} Low novelty (<0.4) = well covered"
}

# ===========================================================================
# Mode 3: Batch Upload
# ===========================================================================
mode_batch_upload() {
    log_head "Batch Upload to Brain"
    require_token || return 1

    if $DRY_RUN; then
        echo -e "  ${YELLOW}*** DRY-RUN MODE -- no data will be sent ***${NC}\n"
    fi

    if [[ ! -d "$DISCOVERIES_DIR" ]]; then
        log_fail "Discoveries directory not found: $DISCOVERIES_DIR"
        return 1
    fi

    # Collect all entries into a temp file (one JSON object per line)
    local entries_file
    entries_file=$(mktemp /tmp/ruv_upload.XXXXXX)
    trap "rm -f '$entries_file'" RETURN

    for f in "$DISCOVERIES_DIR"/*.json; do
        [[ -f "$f" ]] || continue
        jq -c 'if type == "array" then .[] else . end' "$f" 2>/dev/null >> "$entries_file"
    done

    local total
    total=$(wc -l < "$entries_file")
    if [[ "$total" -eq 0 ]]; then
        log_warn "No discovery entries found in $DISCOVERIES_DIR"
        return 0
    fi

    log_info "Found $total entries to upload"
    echo ""

    local success=0 fail=0 skipped=0 current=0

    while IFS= read -r entry; do
        current=$((current + 1))
        progress_bar "$current" "$total"

        # Extract fields
        local title content tags domain
        title=$(echo "$entry" | jq -r '.title // "untitled"')
        content=$(echo "$entry" | jq -r '.content // ""')
        tags=$(echo "$entry" | jq -c '.tags // []')
        domain=$(echo "$entry" | jq -r '.domain // "general"')

        # Skip entries missing content
        if [[ -z "$content" || "$content" == "null" ]]; then
            skipped=$((skipped + 1))
            continue
        fi

        # Strip PII from title and content
        title=$(echo "$title" | strip_pii)
        content=$(echo "$content" | strip_pii)

        # In dry-run mode, skip actual upload
        if $DRY_RUN; then
            skipped=$((skipped + 1))
            continue
        fi

        # Step 1: Get challenge nonce
        local nonce_resp nonce
        nonce_resp=$(api_call GET "/v1/challenge" 2>/dev/null) || { fail=$((fail + 1)); continue; }
        nonce=$(echo "$nonce_resp" | jq -r '.nonce // .challenge // empty' 2>/dev/null)
        if [[ -z "$nonce" ]]; then
            fail=$((fail + 1))
            continue
        fi

        # Step 2: Build payload with nonce
        local payload
        payload=$(jq -n \
            --arg t "$title" \
            --arg c "$content" \
            --arg d "$domain" \
            --arg n "$nonce" \
            --argjson tags "$tags" \
            '{title: $t, content: ($c | .[:2000]), domain: $d, tags: $tags, nonce: $n}')

        # Step 3: POST to /v1/memories
        if api_call POST "/v1/memories" -d "$payload" &>/dev/null; then
            success=$((success + 1))
        else
            fail=$((fail + 1))
        fi

        # Brief rate-limit pause
        sleep 0.3
    done < "$entries_file"

    echo ""  # clear progress bar line
    echo ""
    echo -e "  ${BOLD}Upload Summary:${NC}"
    echo -e "    Total processed: $total"
    echo -e "    ${GREEN}Uploaded:  $success${NC}"
    [[ $fail -gt 0 ]]    && echo -e "    ${RED}Failed:    $fail${NC}"
    [[ $skipped -gt 0 ]] && echo -e "    ${YELLOW}Skipped:   $skipped${NC}"
    $DRY_RUN && echo -e "    ${DIM}(dry-run -- nothing was sent)${NC}"
}

# ===========================================================================
# Mode 4: Training & Optimization
# ===========================================================================
mode_training() {
    log_head "Training & Optimization"
    require_token || return 1

    # Trigger training
    log_info "Triggering training via POST /v1/train..."
    local train_result
    train_result=$(api_call POST "/v1/train" -d '{}') || {
        log_warn "Training endpoint returned an error (may still have triggered)"
    }

    if [[ -n "${train_result:-}" ]]; then
        echo -e "\n  ${BOLD}Training Response:${NC}"
        echo "$train_result" | jq -r '
            to_entries[] |
            "    \(.key): \(.value)"
        ' 2>/dev/null || echo "  $train_result"
    fi

    # Fetch SONA stats
    echo ""
    log_info "Fetching SONA stats from GET /v1/sona/stats..."
    local sona_stats
    sona_stats=$(api_call GET "/v1/sona/stats") || {
        log_warn "Could not retrieve SONA stats"
        return 0
    }

    echo -e "\n  ${BOLD}SONA Statistics:${NC}"
    echo "$sona_stats" | jq -r '
        if type == "object" then
            to_entries[] |
            if (.value | type) == "object" then
                "\n    \(.key):",
                (.value | to_entries[] | "      \(.key): \(.value)")
            else
                "    \(.key): \(.value)"
            end
        else
            "    \(.)"
        end
    ' 2>/dev/null || echo "  $sona_stats"
}

# ===========================================================================
# Mode 5: Cross-Domain Discovery
# ===========================================================================
mode_cross_domain() {
    log_head "Cross-Domain Discovery"
    require_token || return 1

    # Query semantic drift
    log_info "Querying semantic drift via GET /v1/drift..."
    local drift_data
    drift_data=$(api_call GET "/v1/drift" 2>/dev/null) || true

    if [[ -n "${drift_data:-}" ]]; then
        echo -e "\n  ${BOLD}Semantic Drift:${NC}"
        echo "$drift_data" | jq -r '
            if type == "array" then
                .[] |
                "    [\(.from // .source // "?")] --> [\(.to // .target // "?")] drift=\(.score // .magnitude // "n/a")"
            elif type == "object" then
                if .drifts then
                    .drifts[] |
                    "    [\(.from // .source)] --> [\(.to // .target)] drift=\(.score // .magnitude // "n/a")"
                else
                    to_entries[] | "    \(.key): \(.value)"
                end
            else "    \(.)" end
        ' 2>/dev/null || echo "$drift_data" | jq . 2>/dev/null || echo "  $drift_data"
    else
        log_warn "Drift endpoint unavailable"
    fi

    echo ""

    # Query domain partitions
    log_info "Querying domain partitions via GET /v1/partition..."
    local partition_data
    partition_data=$(api_call GET "/v1/partition" 2>/dev/null) || true

    if [[ -n "${partition_data:-}" ]]; then
        echo -e "\n  ${BOLD}Domain Partitions:${NC}"
        echo "$partition_data" | jq -r '
            if type == "array" then
                .[] |
                "    Cluster: \(.id // .name // "?") | Members: \(.members // .domains // [] | join(", ")) | Size: \(.size // (.members // [] | length))"
            elif type == "object" then
                if .partitions then
                    .partitions[] |
                    "    Cluster: \(.id // .name) | Size: \(.size // "?") | Members: \(.members // .domains // [] | join(", "))"
                else
                    to_entries[] | "    \(.key): \(.value)"
                end
            else "    \(.)" end
        ' 2>/dev/null || echo "$partition_data" | jq . 2>/dev/null || echo "  $partition_data"
    else
        log_warn "Partition endpoint unavailable"
    fi

    # Show cross-domain insight
    if [[ -n "${drift_data:-}" && -n "${partition_data:-}" ]]; then
        echo ""
        echo -e "  ${BOLD}Insight:${NC} Domains with high drift and small partition size"
        echo -e "  are the best candidates for cross-domain knowledge transfer."
    fi
}

# ===========================================================================
# Mode 6: Interactive Explorer
# ===========================================================================
mode_explorer() {
    log_head "Interactive Explorer"
    require_token || return 1

    echo -e "  Search the brain for memories. Type ${BOLD}q${NC} to return to menu.\n"

    while true; do
        echo -ne "  ${CYAN}search>${NC} "
        read -r query || break
        [[ -z "$query" ]] && continue
        [[ "$query" == "q" || "$query" == "quit" || "$query" == "exit" ]] && break

        # URL-encode the query
        local encoded_query
        encoded_query=$(printf '%s' "$query" | jq -sRr @uri 2>/dev/null || echo "$query")

        local results
        results=$(api_call GET "/v1/memories/search?q=${encoded_query}") || {
            log_fail "Search failed"
            continue
        }

        # Count results (handle array or wrapper object)
        local count
        count=$(echo "$results" | jq '
            if type == "array" then length
            elif .results then .results | length
            elif .memories then .memories | length
            else 0 end
        ' 2>/dev/null || echo 0)
        echo -e "  ${GREEN}Found $count result(s)${NC}\n"

        # Display results
        echo "$results" | jq -r '
            (if type == "array" then .
             elif .results then .results
             elif .memories then .memories
             else [.] end)[:10][] |
            "  \u001b[1m\(.title // .key // "untitled")\u001b[0m",
            "    Domain: \(.domain // "unknown") | Score: \(.score // .similarity // "n/a")",
            "    \(.content // .value // "" | if length > 120 then .[:120] + "..." else . end)",
            ""
        ' 2>/dev/null || echo "$results" | jq . 2>/dev/null || echo "  $results"
    done
}

# ===========================================================================
# Banner and main menu
# ===========================================================================
show_banner() {
    echo -e "${BOLD}${MAGENTA}"
    echo "  ╔═══════════════════════════════════════════════════════════╗"
    echo "  ║         RuVector Training Orchestrator v1.0              ║"
    echo "  ║         Brain API: pi.ruv.io                             ║"
    echo "  ╚═══════════════════════════════════════════════════════════╝"
    echo -ne "${NC}"
    if $DRY_RUN; then
        echo -e "  ${YELLOW}[DRY-RUN MODE ACTIVE]${NC}"
    fi
    if [[ -n "${PI:-}" ]]; then
        echo -e "  ${GREEN}API token: configured${NC}"
    else
        echo -e "  ${YELLOW}API token: not set (export PI=your-token)${NC}"
    fi
    echo ""
}

main_menu() {
    echo -e "  ${BOLD}Select a mode:${NC}\n"
    echo -e "    ${CYAN}1${NC}  Discovery Scanner        ${DIM}Scan local JSON files for entries and domains${NC}"
    echo -e "    ${CYAN}2${NC}  Brain Gap Analysis        ${DIM}Query /v1/explore for novelty gaps${NC}"
    echo -e "    ${CYAN}3${NC}  Batch Upload              ${DIM}Upload entries via /v1/memories with nonce auth${NC}"
    echo -e "    ${CYAN}4${NC}  Training & Optimization   ${DIM}POST /v1/train + GET /v1/sona/stats${NC}"
    echo -e "    ${CYAN}5${NC}  Cross-Domain Discovery    ${DIM}GET /v1/drift + /v1/partition${NC}"
    echo -e "    ${CYAN}6${NC}  Interactive Explorer       ${DIM}Search brain with /v1/memories/search${NC}"
    echo -e "    ${CYAN}q${NC}  Quit"
    echo ""
    echo -ne "  ${BOLD}Choice [1-6/q]:${NC} "
}

# ===========================================================================
# Entry point
# ===========================================================================
check_deps
show_banner

while true; do
    main_menu
    read -r choice || break
    case "$choice" in
        1) mode_discovery_scanner ;;
        2) mode_gap_analysis ;;
        3) mode_batch_upload ;;
        4) mode_training ;;
        5) mode_cross_domain ;;
        6) mode_explorer ;;
        q|Q|quit|exit) echo -e "\n  ${GREEN}Goodbye.${NC}\n"; exit 0 ;;
        *) log_warn "Invalid choice: '$choice'. Enter 1-6 or q." ;;
    esac
    echo ""
done

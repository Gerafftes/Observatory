# ADR-096: Cloud-Native Data Pipeline, Real-Time Injection & Automated Optimization

**Status**: Accepted
**Date**: 2026-03-16
**Authors**: RuVector Team
**Deciders**: ruv
**Supersedes**: None
**Related**: ADR-059 (Shared Brain Google Cloud), ADR-060 (Shared Brain Capabilities), ADR-093 (Daily Discovery Training), ADR-094 (Shared Web Memory), ADR-095 (API v2 Capabilities), ADR-077 (Midstream Platform)

## 1. Context

The π.ruv.io brain server (ADR-059/060) currently operates as a request-response system: agents push memories, query knowledge, and trigger training manually. While a 5-minute background training loop exists (`main.rs`), the system lacks:

1. **Event-driven ingestion** — No way to push data in real-time from external systems (webhooks, IoT, CI/CD, crawlers)
2. **Automated optimization** — Training, drift monitoring, and graph rebalancing are ad-hoc
3. **Feed ingestion** — No mechanism to poll RSS/Atom/API feeds and ingest new knowledge automatically
4. **Pipeline observability** — No unified metrics for throughput, latency, and pipeline health
5. **Cloud-native scheduling** — Background tasks run in-process; scaling and reliability depend on a single Cloud Run instance

This ADR introduces a Google Cloud-native data pipeline with Pub/Sub for event-driven flow, Cloud Scheduler for periodic optimization, and new REST endpoints for injection, batch processing, and pipeline management.

## 2. Decision

### 2.1 Architecture Overview

```
                    ┌─────────────────────────────────────────────────────────┐
                    │                    π.ruv.io (Cloud Run)                  │
                    │                                                         │
 External Sources   │  ┌──────────┐   ┌──────────┐   ┌──────────────────┐    │
 ─────────────────► │  │ Pipeline │──►│ Embedding│──►│ Knowledge Graph  │    │
 Pub/Sub Push       │  │ Inject   │   │ Engine   │   │ + Witness Chain  │    │
 REST API           │  └──────────┘   └──────────┘   └──────────────────┘    │
 RSS/Atom Feeds     │       │                                │               │
                    │       ▼                                ▼               │
                    │  ┌──────────┐   ┌──────────┐   ┌──────────────────┐    │
                    │  │ Pipeline │   │ Firestore│   │  GCS (RVF        │    │
                    │  │ Metrics  │   │ (durable)│   │  containers)     │    │
                    │  └──────────┘   └──────────┘   └──────────────────┘    │
                    │       ▲                                                 │
                    │       │         ┌──────────┐                           │
 Cloud Scheduler ──►│───────┼────────►│ Optimize │──► train / drift /       │
 (periodic jobs)    │       │         │ Handler  │    transfer / graph /     │
                    │       │         └──────────┘    cleanup / attractor    │
                    └───────┼─────────────────────────────────────────────────┘
                            │
                    ┌───────┴─────────┐
                    │ Cloud Monitoring│
                    │ Dashboard       │
                    └─────────────────┘
```

### 2.2 Component Summary

| Component | GCP Service | Purpose |
|-----------|-------------|---------|
| Real-time injection | Cloud Run (existing) | New `/v1/pipeline/*` endpoints |
| Event bus | Cloud Pub/Sub | `brain-inject`, `brain-events`, `brain-optimize` topics |
| Periodic optimization | Cloud Scheduler | 7 jobs: train, drift, transfer, graph, attractor, full, cleanup |
| Observability | Cloud Monitoring | 10-tile dashboard: latency, throughput, drift, memory, graph |
| Persistence | Firestore + GCS | Write-through cache pattern (existing) |
| Authentication | OIDC + Bearer | Scheduler uses OIDC; API clients use Bearer token |

## 3. New REST API Endpoints

### 3.1 Injection Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/v1/pipeline/inject` | Bearer | Inject a single item (real-time) |
| POST | `/v1/pipeline/inject/batch` | Bearer | Inject up to 100 items per batch |
| POST | `/v1/pipeline/pubsub` | OIDC | Receive Pub/Sub push messages |

### 3.2 Optimization Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/v1/pipeline/optimize` | Bearer | Trigger optimization actions |
| GET | `/v1/pipeline/metrics` | Bearer | Pipeline health and throughput |
| GET | `/v1/pipeline/scheduler/status` | Bearer | Scheduler job states |

### 3.3 Feed Management Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/v1/pipeline/feeds` | Bearer | Add an RSS/Atom feed source |
| GET | `/v1/pipeline/feeds` | Bearer | List configured feeds |

## 4. Injection Pipeline

### 4.1 Single Item Flow

```
POST /v1/pipeline/inject
{
  "source": "arxiv-crawler",
  "title": "Quantum Error Correction via Surface Codes",
  "content": "We present a novel approach to...",
  "tags": ["quantum", "error-correction", "surface-codes"],
  "category": "architecture",
  "metadata": {"arxiv_id": "2503.12345", "authors": ["A. Researcher"]}
}
```

Processing stages:

1. **Validate** — Check title/content length, source field, rate limits
2. **PII Strip** — Regex-based PII detection and redaction (ADR-075 Phase 2)
3. **Embed** — Generate 128-dim RLM embedding via `EmbeddingEngine::embed_for_storage()`
4. **Dedup** — Content hash (SHA3-256) check against existing memories
5. **Witness Chain** — Build 3-entry SHAKE-256 chain (PII → embed → content)
6. **Store** — Write to Firestore via DashMap write-through cache
7. **Graph Update** — Add node to `KnowledgeGraph`, compute similarity edges
8. **Cognitive Store** — Store in Hopfield network, HDC memory, DentateGyrus
9. **Temporal Track** — Record embedding delta in `DeltaStream`
10. **Metrics Update** — Increment pipeline counters
11. **SSE Broadcast** — Notify active SSE sessions of new memory

Response:
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "quality_score": 0.5,
  "witness_hash": "a1b2c3d4...",
  "graph_edges_added": 47
}
```

### 4.2 Batch Injection

```
POST /v1/pipeline/inject/batch
{
  "source": "daily-crawl",
  "items": [
    {"title": "...", "content": "...", "tags": [...], "category": "solution"},
    {"title": "...", "content": "...", "tags": [...], "category": "pattern"},
    ...
  ]
}
```

- Maximum 100 items per batch (enforced server-side)
- Each item processed independently — failures don't block others
- Returns aggregate counts plus individual error messages

Response:
```json
{
  "accepted": 95,
  "rejected": 5,
  "memory_ids": ["...", "...", ...],
  "errors": ["item 3: duplicate content hash", "item 17: content too short", ...]
}
```

### 4.3 Pub/Sub Push Integration

Cloud Pub/Sub delivers messages directly to the brain via HTTP push:

```
POST /v1/pipeline/pubsub
{
  "message": {
    "data": "<base64-encoded InjectRequest JSON>",
    "attributes": {"source": "iot-sensor-grid"},
    "messageId": "1234567890",
    "publishTime": "2026-03-16T00:00:00Z"
  },
  "subscription": "projects/ruv-dev/subscriptions/brain-inject-push"
}
```

Design decisions:
- **No Bearer auth required** — Cloud Run validates the Pub/Sub OIDC service account token automatically
- **200 OK = acknowledgment** — Pub/Sub retries on non-2xx responses
- **Idempotent** — Content hash dedup prevents duplicate ingestion from retries
- **60-second ack deadline** — Allows time for embedding + storage

### 4.4 Feed Ingestion

```
POST /v1/pipeline/feeds
{
  "url": "https://arxiv.org/rss/cs.AI",
  "name": "arXiv CS.AI",
  "category": {"custom": "academic"},
  "tags": ["arxiv", "ai", "research"],
  "poll_interval_secs": 3600
}
```

Feed processing:
1. Poll RSS/Atom URL at configured interval
2. Extract title, content, publication date from each entry
3. Content hash dedup against existing memories
4. Embed and store as new memories
5. Track feed cursor to avoid re-processing

## 5. Optimization System

### 5.1 Available Actions

| Action | What It Does | Typical Duration |
|--------|-------------|-----------------|
| `train` | SONA `force_learn()` + domain `evolve_population()` | 100-500ms |
| `drift_check` | Compute embedding drift per domain, flag anomalies | 50-200ms |
| `transfer_all` | Cross-domain knowledge transfer between category pairs | 200-1000ms |
| `rebuild_graph` | Rebuild CSR matrix + MinCut structure for optimal queries | 500-2000ms |
| `cleanup` | Prune memories with quality < 0.15 after 10+ votes | 100-500ms |
| `attractor_analysis` | Lyapunov exponent estimation per category | 200-800ms |

### 5.2 Optimization Request

```
POST /v1/pipeline/optimize
{
  "actions": ["train", "drift_check", "attractor_analysis"]
}
```

If `actions` is omitted or empty, all 6 actions run in sequence.

Response:
```json
{
  "results": [
    {"action": "train", "success": true, "message": "SONA: 12 patterns, Pareto 56→58", "duration_ms": 234},
    {"action": "drift_check", "success": true, "message": "3 domains checked, 0 drifting", "duration_ms": 87},
    {"action": "attractor_analysis", "success": true, "message": "4 categories analyzed, 3 stable (λ<0)", "duration_ms": 412}
  ],
  "total_duration_ms": 733
}
```

### 5.3 Cloud Scheduler Jobs

Seven recurring jobs target the `/v1/pipeline/optimize` endpoint:

| Job Name | Schedule | Actions | Rationale |
|----------|----------|---------|-----------|
| `brain-train` | `*/5 * * * *` | `["train"]` | Consolidate new knowledge frequently |
| `brain-drift` | `*/15 * * * *` | `["drift_check"]` | Detect poisoning or rapid domain shift |
| `brain-transfer` | `*/30 * * * *` | `["transfer_all"]` | Cross-pollinate growing domains |
| `brain-graph` | `0 * * * *` | `["rebuild_graph"]` | Optimal CSR/MinCut for search quality |
| `brain-attractor` | `*/20 * * * *` | `["attractor_analysis"]` | Lyapunov stability tracking |
| `brain-full-optimize` | `0 3 * * *` | all 6 | Complete daily sweep at low-traffic time |
| `brain-cleanup` | `0 4 * * *` | `["cleanup"]` | Remove low-quality memories |

All jobs use OIDC authentication via the `ruvbrain-scheduler` service account with `roles/run.invoker`.

## 6. Cloud Pub/Sub Topology

### 6.1 Topics

| Topic | Retention | Purpose |
|-------|-----------|---------|
| `brain-inject` | 24h | Real-time data injection from external sources |
| `brain-events` | 24h | Brain events (new memories, training, drift alerts) |
| `brain-optimize` | 1h | Optimization triggers |

### 6.2 Subscriptions

| Subscription | Type | Topic | Purpose |
|-------------|------|-------|---------|
| `brain-inject-push` | Push | `brain-inject` | Delivers to Cloud Run `/v1/pipeline/pubsub` |
| `brain-inject-pull` | Pull | `brain-inject` | Batch processing fallback (72h retention) |
| `brain-events-monitor` | Pull | `brain-events` | Monitoring and alerting |

### 6.3 Publishing Pattern

External systems publish to `brain-inject`:

```bash
gcloud pubsub topics publish brain-inject \
  --project=ruv-dev \
  --message='{"source":"sensor","title":"Temperature Anomaly","content":"Detected +3σ deviation...","tags":["iot","anomaly"]}'
```

The brain can also publish to `brain-events` to notify downstream systems:

```json
{
  "event": "memory_created",
  "memory_id": "550e8400-...",
  "category": "pattern",
  "timestamp": "2026-03-16T00:00:00Z"
}
```

## 7. Pipeline Metrics & Observability

### 7.1 Pipeline State

Tracked in-memory via atomic counters (zero-allocation, lock-free):

| Metric | Type | Description |
|--------|------|-------------|
| `messages_received` | Counter | Total Pub/Sub + API inject requests |
| `messages_processed` | Counter | Successfully stored memories |
| `messages_failed` | Counter | Rejected or errored injections |
| `optimization_cycles` | Counter | Completed optimization runs |
| `last_training` | Timestamp | Most recent training cycle |
| `last_drift_check` | Timestamp | Most recent drift check |
| `last_injection` | Timestamp | Most recent successful injection |
| `injections_per_minute` | Gauge | Rolling throughput estimate |

### 7.2 Metrics Endpoint

```
GET /v1/pipeline/metrics
```

```json
{
  "messages_received": 12847,
  "messages_processed": 12691,
  "messages_failed": 156,
  "memory_count": 2049,
  "graph_nodes": 2049,
  "graph_edges": 412983,
  "last_training": "2026-03-16T01:55:00Z",
  "last_drift_check": "2026-03-16T01:50:00Z",
  "optimization_cycles": 287,
  "uptime_seconds": 86400,
  "injections_per_minute": 8.9
}
```

### 7.3 Cloud Monitoring Dashboard

10-tile mosaic dashboard tracking:

| Tile | Metric | Visualization |
|------|--------|--------------|
| Request Latency | p50/p95/p99 response times | Line chart |
| Request Count | Requests/sec by status | Stacked bar |
| Error Rate | 4xx + 5xx / total | Line with threshold |
| Memory Count | Total memories over time | Scalar + sparkline |
| Graph Edges | Edge count over time | Scalar + sparkline |
| Training Frequency | Training cycles per hour | Bar chart |
| Drift Coefficient | CV per domain, threshold 0.15 | Line with alert |
| Injection Throughput | Messages processed per minute | Line chart |
| Optimization Duration | Cycle duration distribution | Heatmap |
| Resource Utilization | CPU + memory % | Dual line |

## 8. Security Considerations

### 8.1 Authentication Matrix

| Endpoint Group | Auth Method | Who Uses It |
|---------------|-------------|-------------|
| Pipeline inject/batch | Bearer token | API clients, agents, crawlers |
| Pipeline Pub/Sub push | OIDC (Cloud Run auto-validates) | Cloud Pub/Sub service |
| Pipeline optimize | Bearer token | Cloud Scheduler (via headers) |
| Pipeline metrics/feeds | Bearer token | Monitoring, dashboards |

### 8.2 Rate Limiting

- **Per-source injection**: Max 100 items/minute per source identifier
- **Batch size**: Hard cap at 100 items per batch request
- **Pub/Sub**: Controlled by subscription ack deadline (60s) and flow control
- **Optimize**: No rate limit (scheduler controls frequency)

### 8.3 Data Safety

- **PII stripping** on all injected content (15 compiled regexes)
- **Differential privacy** noise on embeddings when `RVF_DP_ENABLED=true`
- **Content hash dedup** prevents replay attacks via Pub/Sub retries
- **Witness chains** provide full provenance for every injected memory
- **Cleanup action** only prunes memories below quality threshold with sufficient votes (not arbitrary deletion)

## 9. Deployment

### 9.1 Infrastructure Setup

```bash
# 1. Setup Pub/Sub topics, subscriptions, IAM
./crates/mcp-brain-server/cloud/setup-pubsub.sh ruv-dev

# 2. Deploy scheduler jobs
./crates/mcp-brain-server/cloud/deploy-scheduler.sh ruv-dev

# 3. Full deployment (build + deploy + setup)
./crates/mcp-brain-server/cloud/deploy-all.sh ruv-dev
```

### 9.2 Cloud Run Configuration

| Setting | Value | Rationale |
|---------|-------|-----------|
| Memory | 2 Gi | Knowledge graph + Hopfield + embeddings in-memory |
| CPU | 2 | Parallel embedding + graph operations |
| Min instances | 1 | Avoid cold starts on Pub/Sub push |
| Max instances | 10 | Handle burst injection traffic |
| Timeout | 300s | Long-running batch injections |
| Concurrency | 80 | High throughput for independent requests |

### 9.3 Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `FIRESTORE_URL` | (required) | Firestore REST endpoint for persistence |
| `GCS_BUCKET` | `ruvector-brain-dev` | RVF container storage |
| `GWT_ENABLED` | `true` | Global Workspace Theory attention |
| `TEMPORAL_ENABLED` | `true` | Temporal delta tracking |
| `META_LEARNING_ENABLED` | `true` | Domain expansion meta-learning |
| `SONA_ENABLED` | `true` | SONA pattern learning |
| `RVF_PII_STRIP` | `true` | PII detection and redaction |
| `RVF_DP_ENABLED` | `false` | Differential privacy noise |
| `MIDSTREAM_ATTRACTOR` | `false` | Lyapunov attractor analysis |

## 10. Common Crawl / Open Crawl Data Integration

### 10.1 Overview

Common Crawl (commoncrawl.org) provides petabyte-scale web crawl data freely available on AWS S3. The brain pipeline integrates this as a massive knowledge source using a tiered approach — from targeted extraction to full corpus processing.

### 10.2 Data Sources

| Source | Format | Size | Update Frequency | Access |
|--------|--------|------|-----------------|--------|
| **Common Crawl WARC** | WARC (Web ARChive) | ~3.5 PB total, ~250 TB/crawl | Monthly | `s3://commoncrawl/` (free, requester-pays) |
| **Common Crawl Index** | CDX (columnar index) | ~300 GB/crawl | Monthly | `s3://commoncrawl/cc-index/` |
| **Common Crawl WET** | Plain text extract | ~15 TB/crawl | Monthly | `s3://commoncrawl/crawl-data/` |
| **CC-MAIN metadata** | JSON/WARC metadata | ~2 TB/crawl | Monthly | WARC headers |
| **OSCAR** (Open Super-large Crawled Aggregated corpus) | Deduplicated text | ~6 TB | Periodic | HuggingFace |
| **C4** (Colossal Clean Crawled Corpus) | Cleaned text | ~750 GB | Static (2019) | GCS / TensorFlow |
| **RefinedWeb** | Quality-filtered CC | ~5 TB | Periodic | HuggingFace |
| **Dolma** | Curated multi-source | ~3 TB | Periodic | HuggingFace |

### 10.3 Integration Architecture

```
 ┌──────────────────────────────────────────────────────────────────┐
 │                  Common Crawl Pipeline                          │
 │                                                                 │
 │  S3 (commoncrawl)    GCS (staging)    Cloud Run (brain)        │
 │  ┌──────────┐        ┌──────────┐     ┌──────────────────┐     │
 │  │ CC Index │──CDX──►│ Filtered │──►  │ /v1/pipeline/    │     │
 │  │ (query)  │ query  │ segments │     │ inject/batch     │     │
 │  └──────────┘        └──────────┘     └──────────────────┘     │
 │                           │                    │                │
 │  ┌──────────┐        ┌────▼─────┐     ┌───────▼────────┐      │
 │  │ WET text │──S3──►│ Content  │     │ Dedup + Embed  │      │
 │  │ extracts │  GET   │ Extract  │     │ + Store        │      │
 │  └──────────┘        └──────────┘     └────────────────┘      │
 │                                                                 │
 │  Cloud Dataflow / Cloud Run Job (batch processing)             │
 └──────────────────────────────────────────────────────────────────┘
```

### 10.4 Three-Tier Processing Strategy

#### Tier 1: Targeted CDX Queries (Real-Time)

Query the Common Crawl index for specific domains or URL patterns, then fetch individual pages:

```bash
# Query CC index for a specific domain
curl "https://index.commoncrawl.org/CC-MAIN-2026-13-index?url=arxiv.org/abs/*&output=json&limit=100"
```

Each CDX result includes WARC offset/length, enabling surgical S3 range-GET for individual pages. This tier integrates directly with the pipeline inject endpoint:

```
CDX query → filter by domain/date → range-GET from S3 → text extract → POST /v1/pipeline/inject
```

**Volume**: 10-1000 pages per query, sub-second per page
**Use case**: Targeted enrichment of specific knowledge domains

#### Tier 2: WET Segment Processing (Batch)

Download pre-extracted text (WET files) and process in batches via Cloud Run Jobs:

```
gs://commoncrawl-staging/ ← download WET segments (~150MB each)
    ↓
Cloud Run Job: extract → filter by language/quality → chunk → deduplicate
    ↓
POST /v1/pipeline/inject/batch (100 items per batch)
```

**Volume**: ~100K pages per WET segment, ~56K segments per crawl
**Use case**: Broad knowledge acquisition across all domains

Quality filters applied:
- Language detection (English primary, multi-language secondary)
- Content length ≥ 200 characters
- Boilerplate ratio < 0.5 (ads, navigation, footers removed)
- Perplexity scoring (reject incoherent text)
- Domain blocklist (spam, porn, malware sites)

#### Tier 3: Full Corpus Analytics (Offline)

Process entire CC crawls via Cloud Dataflow for statistical analysis:

```
S3 (commoncrawl) → Cloud Dataflow (Apache Beam) → BigQuery (analytics)
                                                  → GCS (filtered corpus)
                                                  → Brain (top-k by domain)
```

**Volume**: Billions of pages per crawl
**Use case**: Corpus-level statistics, domain coverage analysis, embedding space mapping

### 10.5 Common Crawl Adapter

New pipeline adapter for CC data:

```rust
/// Common Crawl CDX index query result
struct CdxRecord {
    url: String,
    timestamp: String,
    status: u16,
    mime: String,
    length: u64,
    offset: u64,
    filename: String,  // WARC file path on S3
}

/// Fetch a single page from Common Crawl via WARC range-GET
async fn fetch_cc_page(record: &CdxRecord) -> Result<String, Error> {
    let warc_url = format!("https://data.commoncrawl.org/{}", record.filename);
    // HTTP Range request for exact byte range
    let response = client.get(&warc_url)
        .header("Range", format!("bytes={}-{}", record.offset, record.offset + record.length - 1))
        .send().await?;
    // Parse WARC record, extract HTML, convert to text
    extract_text_from_warc(response.bytes().await?)
}
```

### 10.6 Quality & Deduplication Strategy

| Stage | Method | Purpose |
|-------|--------|---------|
| URL dedup | Bloom filter (1M URLs, 0.1% FPR) | Prevent re-fetching known URLs |
| Content dedup | SHA3-256 normalized hash | Prevent storing duplicate content |
| Embedding dedup | Cosine similarity > 0.95 threshold | Prevent near-duplicate semantic content |
| Quality filter | Perplexity + boilerplate ratio | Reject low-quality text |
| PII strip | 15 compiled regexes | Remove emails, phones, SSNs from CC data |
| Novelty scoring | 1.0 - max(cosine_sim to existing) | Prioritize novel content |

### 10.7 Scheduling

| Job | Schedule | Volume | Duration |
|-----|----------|--------|----------|
| CDX targeted queries | Every 6 hours | ~500 pages/run | ~5 min |
| WET segment batch | Daily 2 AM | ~10K pages/run | ~30 min |
| Full crawl processing | Monthly (after CC release) | ~1M pages | ~24h (Dataflow) |
| Domain coverage report | Weekly | Analytics only | ~10 min |

### 10.8 Cost Estimates

| Component | Monthly Cost | Notes |
|-----------|-------------|-------|
| S3 egress (CC data) | $0 | Free via requester-pays (AWS covers CC hosting) |
| GCS staging | ~$5 | Temporary storage for WET segments |
| Cloud Run Jobs | ~$10 | Batch processing compute |
| Cloud Dataflow | ~$50 | Full crawl processing (monthly) |
| Pub/Sub throughput | ~$2 | Message delivery for injection |
| **Total** | **~$67/month** | For moderate-volume CC integration |

### 10.9 Other Open Data Sources

The pipeline also supports these open data sources via the feed/inject system:

| Source | Type | Content | Integration |
|--------|------|---------|-------------|
| **Wikipedia dumps** | XML/SQL | 6M+ articles | Monthly batch via WET-style processing |
| **arXiv** | RSS + API | Scientific papers | Feed ingestion (`/v1/pipeline/feeds`) |
| **PubMed** | XML + API | Medical literature | Existing `pubmed.rs` adapter |
| **OpenAlex** | REST API | Academic graph (250M+ works) | Batch inject via API polling |
| **Semantic Scholar** | REST API | CS papers + citations | Feed + batch inject |
| **Project Gutenberg** | Text | Public domain books | One-time batch import |
| **Stack Overflow** | Data dump | Programming Q&A | Quarterly batch via archive.org |
| **HuggingFace Datasets** | Parquet/JSON | ML datasets | Selective batch import |
| **GDELT** | CSV/BigQuery | Global events database | Real-time via Pub/Sub |
| **NOAA** | API/CSV | Climate + weather data | Feed ingestion |
| **USPTO** | XML/API | Patent filings | Weekly batch import |
| **SEC EDGAR** | XML/API | Financial filings | Daily feed ingestion |

## 11. File Inventory

| File | Purpose |
|------|---------|
| `src/pipeline.rs` | Data injection pipeline, Pub/Sub client, feed ingestion, metrics |
| `src/routes.rs` | 8 new `/v1/pipeline/*` endpoint handlers |
| `src/types.rs` | `PipelineState`, `InjectRequest`, `BatchInjectRequest`, `PubSubPushMessage`, `OptimizeRequest`, `FeedConfig`, response types |
| `cloud/scheduler-jobs.yaml` | 7 Cloud Scheduler job definitions |
| `cloud/setup-pubsub.sh` | Pub/Sub topic/subscription/IAM setup |
| `cloud/deploy-scheduler.sh` | Scheduler job deployment script |
| `cloud/deploy-all.sh` | Full end-to-end deployment |
| `cloud/monitoring-dashboard.json` | Cloud Monitoring 10-tile dashboard |

## 12. Acceptance Criteria

1. `POST /v1/pipeline/inject` stores a memory with witness chain and returns 201
2. `POST /v1/pipeline/inject/batch` processes 100 items and returns aggregate results
3. `POST /v1/pipeline/pubsub` accepts Pub/Sub push format, decodes base64, stores memory
4. `POST /v1/pipeline/optimize` runs all 6 actions and returns per-action results
5. `GET /v1/pipeline/metrics` returns current pipeline counters
6. Cloud Scheduler jobs fire on schedule and hit optimize endpoint
7. Pub/Sub push subscription delivers to Cloud Run endpoint
8. `cargo check` passes with all new code
9. No new dependencies added to `Cargo.toml`

## 13. Consequences

### Positive

- **Real-time ingestion**: External systems can push data via Pub/Sub without polling
- **Automated optimization**: Brain self-maintains without manual intervention
- **Observability**: Pipeline metrics + Cloud Monitoring provide full visibility
- **Scalability**: Pub/Sub handles backpressure; Cloud Run scales 1-10 instances
- **Reliability**: Pub/Sub retry + content hash dedup = at-least-once delivery without duplicates

### Negative

- **Operational complexity**: 7 scheduler jobs + 3 Pub/Sub topics to manage
- **Cost**: Cloud Scheduler ($0.10/job/month), Pub/Sub ($0.04/GB), min-instances=1 on Cloud Run
- **In-memory state**: Pipeline metrics reset on Cloud Run instance restart (acceptable — Firestore has durable data)

### Risks

- **Pub/Sub push storms**: If a topic has a burst of messages, Cloud Run may scale rapidly — mitigated by concurrency=80 and rate limiting
- **Optimization contention**: Concurrent optimize requests could conflict — mitigated by `parking_lot::RwLock` on shared state
- **Feed polling overhead**: Many feeds could consume significant HTTP bandwidth — mitigated by configurable poll intervals and per-feed dedup

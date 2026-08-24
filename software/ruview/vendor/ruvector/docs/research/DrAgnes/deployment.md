# DrAgnes Google Cloud Deployment Plan

**Status**: Research & Planning
**Date**: 2026-03-21

## Overview

DrAgnes leverages the existing pi.ruv.io Google Cloud infrastructure, extending it with dermatology-specific services. The deployment follows a multi-region, HIPAA-compliant architecture using Google Cloud's BAA-covered services.

## Architecture Overview

```
                        ┌─────────────────────────────────┐
                        │        Cloud CDN + LB            │
                        │   (Global, HTTPS termination)    │
                        └──────────┬──────────────────────┘
                                   │
                    ┌──────────────┼──────────────┐
                    │              │              │
              ┌─────┴─────┐ ┌─────┴─────┐ ┌─────┴─────┐
              │ us-east1  │ │ us-west1  │ │ europe-w1 │
              │ (primary) │ │ (failover)│ │ (EU data) │
              └─────┬─────┘ └─────┬─────┘ └─────┬─────┘
                    │              │              │
         ┌──────────┴──────────────┴──────────────┴──────────┐
         │                  Service Mesh                      │
         │                                                    │
         │  ┌────────────┐  ┌────────────┐  ┌────────────┐   │
         │  │ DrAgnes    │  │ Brain      │  │ CNN Model  │   │
         │  │ API        │  │ Server     │  │ Server     │   │
         │  │ (Cloud Run)│  │ (Cloud Run)│  │ (Cloud Run)│   │
         │  └─────┬──────┘  └─────┬──────┘  └─────┬──────┘   │
         │        │               │               │           │
         │  ┌─────┴───────────────┴───────────────┴─────┐     │
         │  │              Data Layer                    │     │
         │  │                                            │     │
         │  │  Firestore │ GCS │ Memorystore │ BigQuery  │     │
         │  └────────────────────────────────────────────┘     │
         │                                                    │
         │  ┌────────────────────────────────────────────┐     │
         │  │           Event Layer                      │     │
         │  │                                            │     │
         │  │  Pub/Sub │ Cloud Scheduler │ Cloud Tasks   │     │
         │  └────────────────────────────────────────────┘     │
         │                                                    │
         │  ┌────────────────────────────────────────────┐     │
         │  │           Security Layer                   │     │
         │  │                                            │     │
         │  │  IAM │ Secret Manager │ CMEK │ VPC-SC     │     │
         │  └────────────────────────────────────────────┘     │
         └────────────────────────────────────────────────────┘
```

## Service Configuration

### 1. DrAgnes API Service (Cloud Run)

Primary API service for classification requests and practice management.

```yaml
# dragnes-api.yaml
apiVersion: serving.knative.dev/v1
kind: Service
metadata:
  name: dragnes-api
  annotations:
    run.googleapis.com/launch-stage: GA
    run.googleapis.com/ingress: internal-and-cloud-load-balancing
spec:
  template:
    metadata:
      annotations:
        autoscaling.knative.dev/minInstances: "2"
        autoscaling.knative.dev/maxInstances: "100"
        run.googleapis.com/cpu-throttling: "false"
        run.googleapis.com/execution-environment: gen2
    spec:
      containerConcurrency: 80
      timeoutSeconds: 300
      containers:
        - image: gcr.io/ruvector-brain-dev/dragnes-api:latest
          ports:
            - containerPort: 8080
          resources:
            limits:
              cpu: "2"
              memory: 2Gi
          env:
            - name: BRAIN_URL
              value: "https://brain-server-internal.run.app"
            - name: MODEL_BUCKET
              value: "gs://dragnes-models"
            - name: RUST_LOG
              value: "info"
          startupProbe:
            httpGet:
              path: /health
            initialDelaySeconds: 5
            periodSeconds: 5
```

### 2. CNN Model Server (Cloud Run)

Server-side CNN inference for practices without WASM capability.

```yaml
# dragnes-cnn.yaml
apiVersion: serving.knative.dev/v1
kind: Service
metadata:
  name: dragnes-cnn
spec:
  template:
    metadata:
      annotations:
        autoscaling.knative.dev/minInstances: "1"
        autoscaling.knative.dev/maxInstances: "50"
        run.googleapis.com/cpu-throttling: "false"
        run.googleapis.com/execution-environment: gen2
    spec:
      containerConcurrency: 20
      timeoutSeconds: 30
      containers:
        - image: gcr.io/ruvector-brain-dev/dragnes-cnn:latest
          ports:
            - containerPort: 8080
          resources:
            limits:
              cpu: "4"
              memory: 4Gi
          env:
            - name: MODEL_PATH
              value: "/models/mobilenetv3_small_int8.bin"
            - name: SIMD_ENABLED
              value: "true"
```

**Performance Notes**:
- Cloud Run gen2 provides AVX2 SIMD acceleration
- INT8 quantized model fits in <5MB memory
- Target: <50ms inference per image
- Concurrency limited to 20 (CPU-bound workload)

### 3. Brain Server (Existing)

The existing pi.ruv.io brain server at `brain-server-*.run.app` handles:
- Knowledge graph management (316K edges)
- HNSW search (128-dim, PiQ3 quantized)
- PubMed integration
- Sparsifier analytics (ADR-116)
- Witness chain management

**DrAgnes-specific extensions**:
- New memory namespace: `dragnes-dermatology`
- Custom similarity threshold for dermoscopic embeddings
- Dermoscopy-specific PubMed search templates
- Classification feedback ingestion endpoint

### 4. PWA Frontend (Firebase Hosting)

```
Firebase Hosting Configuration
    │
    ├── Hosting
    │       ├── SPA routing (all paths → index.html)
    │       ├── CDN caching (immutable assets: 1 year)
    │       ├── WASM files: Cache-Control: public, max-age=31536000
    │       ├── Model weights: Cache-Control: public, max-age=86400
    │       └── API proxy: /api/** → Cloud Run dragnes-api
    │
    ├── Service Worker (Workbox)
    │       ├── Precache: app shell, WASM module, model weights
    │       ├── Runtime cache: brain search results (stale-while-revalidate)
    │       ├── Background sync: diagnosis submissions
    │       └── Offline fallback page
    │
    └── PWA Manifest
            ├── name: "DrAgnes"
            ├── display: "standalone"
            ├── orientation: "portrait"
            ├── theme_color: "#1a365d"
            └── icons: 192x192, 512x512 (maskable)
```

## Data Storage

### Firestore (De-Identified Metadata)

```
Firestore Collections
    │
    ├── /practices/{practiceId}
    │       ├── name: string
    │       ├── region: string
    │       ├── modelVersion: string
    │       ├── totalClassifications: number
    │       ├── dpBudgetUsed: number
    │       └── createdAt: timestamp
    │
    ├── /classifications/{classificationId}
    │       ├── practiceId: string (hashed)
    │       ├── lesionClass: string
    │       ├── confidence: number
    │       ├── abcdeTotal: number
    │       ├── sevenPointScore: number
    │       ├── riskLevel: string
    │       ├── clinicianAction: string
    │       ├── fitzpatrickType: number (I-VI)
    │       ├── bodyLocationCategory: string
    │       ├── ageDecade: number
    │       ├── witnessHash: string
    │       └── createdAt: timestamp
    │       NOTE: No patient identifiers. No raw images.
    │
    ├── /feedback/{feedbackId}
    │       ├── classificationId: string
    │       ├── clinicianReview: string
    │       ├── correctedClass: string (optional)
    │       ├── histopathResult: string (optional)
    │       └── createdAt: timestamp
    │
    └── /modelVersions/{versionId}
            ├── version: string (semver)
            ├── trainedOn: number (embedding count)
            ├── accuracy: number
            ├── sensitivityMelanoma: number
            ├── specificityMelanoma: number
            ├── fairnessScore: number
            └── releasedAt: timestamp
```

**Firestore Security Rules**:
- Practice-level tenant isolation
- Write access: authenticated clinicians only
- Read access: same practice only
- Admin access: platform operators only
- No cross-practice data access

### Google Cloud Storage (GCS)

```
GCS Buckets
    │
    ├── gs://dragnes-models/
    │       ├── mobilenetv3_small_int8.bin          (INT8 model, ~5MB)
    │       ├── mobilenetv3_small_fp32.bin           (FP32 model, ~15MB)
    │       ├── mobilenetv3_small.wasm               (WASM module, ~2MB)
    │       ├── lora_weights/{practiceId}/latest.bin (per-practice LoRA)
    │       └── reference_embeddings/top1000.bin     (offline cache)
    │       Encryption: CMEK (AES-256)
    │       Access: dragnes-api service account only
    │
    ├── gs://dragnes-rvf/
    │       ├── {contributorHash}/{memoryId}.rvf     (RVF containers)
    │       Encryption: CMEK (AES-256)
    │       Access: brain server service account only
    │       Lifecycle: Archive after 90 days, delete after 7 years
    │
    └── gs://dragnes-audit/
            ├── access_logs/YYYY/MM/DD/*.jsonl
            ├── classification_logs/YYYY/MM/DD/*.jsonl
            └── security_events/YYYY/MM/DD/*.jsonl
            Encryption: CMEK (AES-256)
            Retention: 6 years (HIPAA minimum)
            Access: Security team only
```

### Memorystore (Redis) -- Optional Performance Layer

```
Redis Instance (Basic tier, 1GB)
    │
    ├── Session cache (15-min TTL)
    ├── Rate limiting counters (per-practice, per-hour)
    ├── HNSW search result cache (5-min TTL)
    └── Model version cache (1-hour TTL)
```

## Event Architecture

### Pub/Sub Topics

```
Pub/Sub Configuration
    │
    ├── dragnes-classification (new classification events)
    │       ├── Publisher: dragnes-api
    │       ├── Subscriber: brain-server (brain ingestion)
    │       ├── Subscriber: dragnes-analytics (BigQuery sink)
    │       └── Subscriber: dragnes-alerts (monitoring)
    │
    ├── dragnes-feedback (clinician feedback events)
    │       ├── Publisher: dragnes-api
    │       ├── Subscriber: brain-server (model improvement)
    │       └── Subscriber: dragnes-analytics (accuracy tracking)
    │
    ├── dragnes-model-update (model version events)
    │       ├── Publisher: dragnes-training (Cloud Run job)
    │       ├── Subscriber: dragnes-api (hot-reload)
    │       └── Subscriber: dragnes-cnn (hot-reload)
    │
    └── dragnes-alerts (monitoring alerts)
            ├── Publisher: various services
            └── Subscriber: Cloud Monitoring → PagerDuty
```

### Cloud Scheduler Jobs

```
Scheduled Jobs
    │
    ├── dragnes-model-retrain
    │       ├── Schedule: Weekly (Sunday 02:00 UTC)
    │       ├── Action: Trigger Cloud Run job for model retraining
    │       ├── Input: New feedback + brain embeddings since last train
    │       └── Output: New model version to GCS
    │
    ├── dragnes-drift-check
    │       ├── Schedule: Daily (06:00 UTC)
    │       ├── Action: Brain drift analysis on dermoscopy namespace
    │       └── Alert: If drift > 0.15, trigger early retrain
    │
    ├── dragnes-fairness-audit
    │       ├── Schedule: Weekly (Monday 08:00 UTC)
    │       ├── Action: Compute accuracy by Fitzpatrick type
    │       └── Alert: If disparity > 5%, flag for investigation
    │
    ├── dragnes-privacy-audit
    │       ├── Schedule: Daily (04:00 UTC)
    │       ├── Action: Verify no PII in Firestore/GCS
    │       └── Alert: Any PII detection triggers incident
    │
    └── dragnes-backup
            ├── Schedule: Daily (00:00 UTC)
            ├── Action: Firestore export to GCS
            └── Retention: 30 daily + 12 monthly + 7 yearly
```

## Security Configuration

### Google Secrets Manager

```
Secrets (extending existing pi.ruv.io secrets)
    │
    ├── dragnes-api-key              (API authentication key)
    ├── dragnes-jwt-signing-key      (JWT token signing)
    ├── dragnes-cmek-key-id          (CMEK key reference)
    ├── dragnes-oauth-client-id      (Google OAuth client)
    ├── dragnes-oauth-client-secret  (Google OAuth secret)
    ├── dragnes-firebase-config      (Firebase project config)
    └── dragnes-pubmed-api-key       (NCBI E-utilities key)

    Existing secrets reused:
    ├── ANTHROPIC_API_KEY            (for chat interface LLM)
    └── huggingface-token            (for model downloads)
```

### IAM Configuration

```
Service Accounts
    │
    ├── dragnes-api@ruvector-brain-dev.iam.gserviceaccount.com
    │       ├── roles/run.invoker (invoke brain server)
    │       ├── roles/datastore.user (Firestore read/write)
    │       ├── roles/storage.objectViewer (model bucket)
    │       ├── roles/pubsub.publisher (classification events)
    │       └── roles/secretmanager.secretAccessor (secrets)
    │
    ├── dragnes-cnn@ruvector-brain-dev.iam.gserviceaccount.com
    │       ├── roles/storage.objectViewer (model bucket)
    │       └── roles/secretmanager.secretAccessor (secrets)
    │
    └── dragnes-training@ruvector-brain-dev.iam.gserviceaccount.com
            ├── roles/storage.objectAdmin (model bucket, write new versions)
            ├── roles/datastore.viewer (read feedback data)
            ├── roles/pubsub.publisher (model update events)
            └── roles/bigquery.dataViewer (analytics queries)
```

### VPC Service Controls

```
VPC-SC Perimeter: dragnes-perimeter
    │
    ├── Protected Services
    │       ├── firestore.googleapis.com
    │       ├── storage.googleapis.com
    │       ├── bigquery.googleapis.com
    │       └── secretmanager.googleapis.com
    │
    ├── Access Levels
    │       ├── Corporate network CIDR ranges
    │       ├── Cloud Run service accounts (internal)
    │       └── Emergency break-glass accounts
    │
    └── Ingress Rules
            ├── Allow: Cloud Run → Firestore/GCS (internal)
            ├── Allow: Cloud Scheduler → Cloud Run (internal)
            └── Deny: All other access to protected services
```

## Multi-Region Deployment

### Region Selection

| Region | Role | Justification |
|--------|------|---------------|
| us-east1 (South Carolina) | Primary | Low latency to East Coast US; HIPAA eligible |
| us-west1 (Oregon) | Failover | West Coast coverage; disaster recovery |
| europe-west1 (Belgium) | EU Data Residency | GDPR compliance for EU practices |
| asia-southeast1 (Singapore) | Future | APAC coverage (Phase 4) |

### Cross-Region Data Flow

```
Data Residency Rules
    │
    ├── Patient metadata: Region-locked (US data stays in US, EU in EU)
    ├── De-identified brain embeddings: Global (privacy-preserving)
    ├── Model weights: Global (no PHI)
    ├── Audit logs: Region-locked
    └── WASM/PWA assets: Global CDN
```

## Monitoring & Observability

### Cloud Monitoring Dashboard

```
DrAgnes Operations Dashboard
    │
    ├── Service Health
    │       ├── API latency (p50, p95, p99)
    │       ├── CNN inference latency
    │       ├── Error rate by endpoint
    │       ├── Active instances per region
    │       └── Request volume (per hour, per practice)
    │
    ├── Classification Metrics
    │       ├── Classifications per hour (global)
    │       ├── Distribution by lesion class
    │       ├── Average confidence score
    │       ├── Clinician override rate
    │       └── Sensitivity/specificity (rolling 30-day)
    │
    ├── Brain Health
    │       ├── Memory count (dermatology namespace)
    │       ├── Drift status
    │       ├── Embedding quality score
    │       └── Sync latency
    │
    ├── Privacy & Compliance
    │       ├── PII scan results (should always be 0)
    │       ├── DP budget consumption per practice
    │       ├── Access audit anomalies
    │       └── Witness chain verification failures
    │
    └── Cost Tracking
            ├── Cloud Run cost by service
            ├── Storage cost by bucket
            ├── Network egress cost
            └── Total monthly cost vs. budget
```

### Alert Policies

| Alert | Condition | Severity | Action |
|-------|-----------|----------|--------|
| API error rate > 1% | 5-min window | P2 | PagerDuty notification |
| CNN latency > 500ms (p95) | 15-min window | P3 | Slack notification |
| PII detected in cloud | Any occurrence | P1 | Immediate incident response |
| Melanoma sensitivity < 90% | 7-day rolling | P1 | Model freeze + investigation |
| Fairness disparity > 5% | Weekly audit | P2 | Investigation within 24 hours |
| Brain drift > 0.15 | Daily check | P3 | Trigger early retrain |
| DP budget > 80% for practice | Per check | P3 | Notify practice admin |

## Cost Projections

### Monthly Cost Estimates (by Scale)

| Component | 10 Practices | 100 Practices | 1,000 Practices |
|-----------|-------------|--------------|-----------------|
| Cloud Run (API) | $50 | $200 | $1,500 |
| Cloud Run (CNN) | $30 | $150 | $1,000 |
| Brain Server (shared) | $150 (existing) | $150 | $300 |
| Firestore | $10 | $50 | $300 |
| GCS (models + RVF) | $5 | $20 | $100 |
| Cloud CDN | $10 | $30 | $150 |
| Firebase Hosting | $0 (free tier) | $25 | $100 |
| Memorystore (Redis) | $0 (skip) | $50 | $100 |
| Cloud Monitoring | $0 (free tier) | $50 | $200 |
| Secret Manager | $1 | $1 | $5 |
| Pub/Sub | $1 | $5 | $30 |
| Cloud Scheduler | $1 | $1 | $5 |
| BigQuery (analytics) | $0 (free tier) | $20 | $100 |
| **Total Monthly** | **~$258** | **~$752** | **~$3,890** |
| **Per Practice/Month** | **$25.80** | **$7.52** | **$3.89** |

### Revenue Model

| Tier | Price | Features |
|------|-------|---------|
| Starter | $99/mo/practice | 500 classifications/mo, WASM offline, basic brain |
| Professional | $199/mo/practice | Unlimited, LoRA adaptation, full brain, teledermatology |
| Enterprise | Custom | Multi-practice, EHR integration, dedicated support, SLA |
| Academic | Free | Research use, data contribution agreement |
| Underserved | Free | Qualifying community health centers |

**Break-even**: approximately 30 practices on Professional tier covers infrastructure costs at the 100-practice scale.

## Deployment Pipeline

```
Deployment Pipeline (Cloud Build)
    │
    ├── Source: GitHub (ruvector/dragnes)
    ├── Trigger: Push to main branch
    │
    ├── Build Stage
    │       ├── Rust compilation (--release --target x86_64-unknown-linux-gnu)
    │       ├── WASM compilation (--target wasm32-unknown-unknown)
    │       ├── Docker image build (distroless base)
    │       └── SvelteKit build (npm run build)
    │
    ├── Test Stage
    │       ├── Unit tests (cargo test)
    │       ├── Integration tests (against staging brain)
    │       ├── WASM inference accuracy test (reference images)
    │       ├── Security scan (cargo audit + npm audit)
    │       └── HIPAA compliance checks (PII scanner)
    │
    ├── Deploy Stage (Canary)
    │       ├── Deploy to staging (full test suite)
    │       ├── Canary deployment (5% traffic for 30 minutes)
    │       ├── Monitor error rate and latency
    │       ├── Auto-rollback if error rate > 0.5%
    │       └── Promote to 100% if healthy
    │
    └── Post-Deploy
            ├── Smoke tests against production
            ├── Notify operations channel
            ├── Update model version registry
            └── Archive previous version artifacts
```

## Disaster Recovery

| Scenario | RTO | RPO | Recovery Procedure |
|----------|-----|-----|-------------------|
| Single region outage | 5 minutes | 0 (multi-region) | Automatic failover via Cloud LB |
| Firestore corruption | 1 hour | 24 hours | Restore from daily export |
| Model corruption | 10 minutes | N/A | Roll back to previous model version |
| Brain server outage | 5 minutes | 0 | Existing brain HA (pi.ruv.io) |
| Complete GCP outage | 4 hours | 24 hours | Multi-cloud DR (backup to AWS S3) |
| Security breach | 1 hour | N/A | Incident response plan activation |

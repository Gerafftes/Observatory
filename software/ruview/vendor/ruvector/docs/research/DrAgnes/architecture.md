# DrAgnes System Architecture

**Status**: Research & Planning
**Date**: 2026-03-21

## Overview

DrAgnes is a layered architecture that connects dermoscopic imaging hardware through a mobile-first web application to a CNN classification engine and collective intelligence brain. The design prioritizes offline capability, privacy preservation, and continuous learning.

## High-Level Architecture

```
                    ┌─────────────────────────────────────────────────────────┐
                    │                    DrAgnes Platform                     │
                    │                                                         │
  ┌──────────┐     │  ┌──────────────┐    ┌──────────────┐    ┌───────────┐ │
  │ DermLite │────▶│  │   RuVocal    │───▶│  CNN Engine   │───▶│  Brain    │ │
  │ HUD/DL5  │     │  │   PWA UI     │    │  (WASM)       │    │ pi.ruv.io │ │
  └──────────┘     │  └──────────────┘    └──────────────┘    └───────────┘ │
                    │        │                    │                   │       │
  ┌──────────┐     │        ▼                    ▼                   ▼       │
  │ Phone    │────▶│  ┌──────────────┐    ┌──────────────┐    ┌───────────┐ │
  │ Camera   │     │  │ Image Capture│    │  HNSW Search  │    │ PubMed    │ │
  └──────────┘     │  │ & Preprocess │    │  + GNN Topo   │    │ Enrichment│ │
                    │  └──────────────┘    └──────────────┘    └───────────┘ │
                    │                                                         │
                    │  ┌──────────────────────────────────────────────────┐   │
                    │  │              Privacy & Compliance Layer           │   │
                    │  │  PII Strip │ Diff. Privacy │ Witness Chain │ BAA  │   │
                    │  └──────────────────────────────────────────────────┘   │
                    │                                                         │
                    │  ┌──────────────────────────────────────────────────┐   │
                    │  │              Google Cloud Infrastructure          │   │
                    │  │  Cloud Run │ Firestore │ GCS │ Pub/Sub │ CDN    │   │
                    │  └──────────────────────────────────────────────────┘   │
                    └─────────────────────────────────────────────────────────┘
```

## Component Architecture

### 1. DermLite Device Integration Layer

DermLite devices attach to smartphones and provide standardized dermoscopic imaging.

**Supported Devices**:
- **DermLite HUD** (Heads-Up Display): Hands-free dermoscopy with built-in camera. Connects via Bluetooth for metadata. Captures 1920x1080 polarized and non-polarized images.
- **DermLite DL5**: Flagship handheld dermatoscope. 10x magnification, hybrid polarized/non-polarized mode. USB-C or Lightning adapter for phone attachment.
- **DermLite DL4**: Compact pocket dermatoscope. Smartphone adapter available. LED illumination with polarization.
- **DermLite DL200 Hybrid**: Contact and non-contact dermoscopy. Magnetic phone adapter.

**Image Capture Flow**:
```
DermLite Adapter
    │
    ├── Phone Camera (MediaStream API)
    │       │
    │       ▼
    │   getUserMedia({ video: { facingMode: 'environment',
    │                           width: 1920, height: 1080 } })
    │       │
    │       ▼
    │   Canvas capture (ImageData → Uint8Array)
    │       │
    │       ▼
    │   Preprocessing Pipeline
    │       ├── Color normalization (Shades of Gray)
    │       ├── Hair removal (DullRazor algorithm via WASM)
    │       ├── Lesion segmentation (Otsu + GrabCut via WASM)
    │       ├── Resize to 224x224 (bilinear interpolation)
    │       └── ImageNet normalization (mean=[0.485,0.456,0.406],
    │                                   std=[0.229,0.224,0.225])
    │
    ▼
Preprocessed Tensor [1, 3, 224, 224] float32
```

**DermLite-Specific Processing**:
- Auto-detect polarization mode from EXIF metadata
- Calibrate white balance using DermLite's known LED spectrum (4500K)
- Extract measurement scale from DermLite's ruler overlay
- Compensate for contact plate reflection artifacts in contact dermoscopy mode

### 2. CNN Classification Engine

Built on `ruvector-cnn` with MobileNetV3 Small backbone, compiled to WASM for browser execution.

**Architecture**:
```
Input [1, 3, 224, 224]
    │
    ▼
MobileNetV3 Small Backbone
    │   ├── Conv2D layers with SE (Squeeze-Excite) blocks
    │   ├── Inverted residuals with h-swish activation
    │   └── SIMD128 accelerated (AVX2 on server, WASM SIMD in browser)
    │
    ▼
Feature Vector [576-dim] (fp32 or INT8 quantized)
    │
    ├──▶ HNSW Search (k=5 nearest neighbors in brain)
    │       │
    │       ▼
    │     Reference cases with known diagnoses
    │
    ├──▶ SONA MicroLoRA Classifier (rank-2)
    │       │
    │       ├── Online adaptation per practice
    │       ├── EWC++ (lambda=2000) catastrophic forgetting prevention
    │       └── 7-class output probabilities
    │
    ├──▶ Grad-CAM Heatmap Generation
    │       │
    │       └── Spatial attention overlay on original image
    │
    └──▶ ABCDE Risk Scoring Module
            │
            ├── Asymmetry score (contour analysis)
            ├── Border irregularity (fractal dimension)
            ├── Color variance (histogram analysis across 6 color channels)
            ├── Diameter estimation (calibrated from DermLite scale)
            └── Evolution tracking (temporal comparison with prior images)
```

**Classification Taxonomy** (7 classes, aligned with HAM10000):
| Class | Label | Risk Level |
|-------|-------|-----------|
| akiec | Actinic keratosis / Bowen's | Medium-High |
| bcc | Basal cell carcinoma | High |
| bkl | Benign keratosis (solar lentigo, seborrheic keratosis) | Low |
| df | Dermatofibroma | Low |
| mel | Melanoma | Critical |
| nv | Melanocytic nevus (mole) | Low |
| vasc | Vascular lesion (angioma, angiokeratoma, pyogenic granuloma) | Low |

**Performance Targets**:
| Metric | Target | Notes |
|--------|--------|-------|
| Inference latency (WASM) | <200ms | On mid-range phone (Snapdragon 778G) |
| Inference latency (server) | <50ms | Cloud Run with AVX2 |
| Melanoma sensitivity | >95% | Critical -- minimize false negatives |
| Melanoma specificity | >85% | Balance against unnecessary biopsies |
| Model size (INT8) | <5MB | For offline PWA cache |
| Embedding dimension | 576 | MobileNetV3 Small penultimate layer |

### 3. Brain Integration Layer

The pi.ruv.io brain serves as the collective intelligence backbone.

**Data Flow**:
```
Diagnosis Complete
    │
    ▼
PII Stripping Pipeline
    │   ├── Remove patient identifiers
    │   ├── Remove GPS/location from EXIF
    │   ├── Remove device serial numbers
    │   ├── Generalize age to decade bracket
    │   ├── Generalize skin type to Fitzpatrick scale
    │   └── Hash remaining quasi-identifiers (k-anonymity, k>=5)
    │
    ▼
Differential Privacy Layer (epsilon=1.0)
    │   ├── Laplace noise on continuous features
    │   ├── Randomized response on categorical features
    │   └── Privacy budget tracking per practice per epoch
    │
    ▼
RVF Cognitive Container
    │   ├── Segment 0: 576-dim embedding (no raw image)
    │   ├── Segment 1: Classification probabilities
    │   ├── Segment 2: ABCDE scores
    │   ├── Segment 3: De-identified metadata
    │   │       ├── Fitzpatrick type (I-VI)
    │   │       ├── Body location (categorical)
    │   │       ├── Age decade
    │   │       ├── Lesion diameter (mm, bucketed)
    │   │       └── Dermoscopic features present
    │   └── Segment 4: Witness chain (SHAKE-256)
    │
    ▼
Brain Memory Insert
    │   ├── HNSW index update (128-dim projected via RlmEmbedder)
    │   ├── Knowledge graph edge creation
    │   ├── Sparsifier incremental update (ADR-116)
    │   └── GNN topology enrichment
    │
    ▼
Cross-Practice Learning
    │   ├── PageRank-weighted similarity across all practices
    │   ├── SONA meta-learning for population-level patterns
    │   ├── PubMed enrichment for newly observed lesion subtypes
    │   └── Federated model update (no raw data exchange)
```

**Brain Endpoints Used**:
| Endpoint | Purpose |
|----------|---------|
| `brain_share` | Submit de-identified diagnosis embedding to collective |
| `brain_search` | Find similar historical cases by embedding similarity |
| `brain_page_create` | Create structured dermatology knowledge pages |
| `brain_page_evidence` | Attach PubMed evidence to diagnostic findings |
| `brain_drift` | Monitor embedding space drift as new lesion types emerge |
| `brain_partition` | Cluster lesion subtypes via MinCut partitioning |
| `brain_sync` | Sync local model updates with collective |

### 4. RuVocal Chat Interface

The existing RuVocal SvelteKit application serves as the user interface, extended with dermatology-specific components.

**UI Components**:
```
RuVocal DrAgnes Mode
    │
    ├── Camera Capture Panel
    │       ├── Live viewfinder with DermLite overlay
    │       ├── Capture button (high-res still)
    │       ├── Image quality indicator
    │       └── Body location selector (anatomical diagram)
    │
    ├── Analysis Dashboard
    │       ├── Classification probabilities (bar chart)
    │       ├── Grad-CAM heatmap overlay (toggle)
    │       ├── ABCDE score breakdown (radar chart)
    │       ├── Similar cases panel (from brain search)
    │       └── Risk assessment summary (traffic light)
    │
    ├── Clinical Decision Support
    │       ├── Recommended action (monitor / biopsy / refer)
    │       ├── 7-point checklist auto-scoring
    │       ├── Menzies method evaluation
    │       ├── PubMed literature links
    │       └── Clinical guidelines citations (AAD, BAD)
    │
    ├── Patient Timeline
    │       ├── Lesion evolution tracking
    │       ├── Side-by-side comparison (temporal)
    │       ├── ABCDE score trend graphs
    │       └── Dermoscopic feature change detection
    │
    └── Chat Interface
            ├── Natural language queries about lesion
            ├── Differential diagnosis discussion
            ├── Literature search via brain
            └── Clinical note generation
```

### 5. Offline Architecture

**Service Worker Strategy**:
```
Service Worker (Workbox)
    │
    ├── Cache-First Strategy
    │       ├── CNN model weights (.onnx → WASM, ~5MB)
    │       ├── Application shell (HTML, CSS, JS)
    │       ├── WASM module (ruvector-cnn-wasm)
    │       └── Reference image embeddings (top-1000 from brain)
    │
    ├── Network-First Strategy
    │       ├── Brain search queries
    │       ├── PubMed enrichment
    │       └── Cross-practice sync
    │
    └── Background Sync
            ├── Queue diagnosis submissions for brain
            ├── Sync model updates when online
            └── Pull new reference embeddings nightly
```

**Offline Capabilities**:
- Full CNN inference (WASM, no server needed)
- ABCDE scoring (local computation)
- Grad-CAM visualization (local computation)
- HNSW search against cached reference embeddings
- Queue-and-sync for brain submissions

### 6. Multi-Practice Knowledge Sharing

**Privacy-Preserving Federation**:
```
Practice A                    pi.ruv.io Brain                  Practice B
    │                              │                               │
    ├── De-identified ────────────▶│◀──────────── De-identified ───┤
    │   embedding                  │              embedding        │
    │                              │                               │
    │                    ┌─────────┴─────────┐                     │
    │                    │  Collective Model  │                     │
    │                    │  ── ── ── ── ── ─  │                     │
    │                    │  No raw images     │                     │
    │                    │  No patient IDs    │                     │
    │                    │  No practice IDs   │                     │
    │                    │  Only: embeddings  │                     │
    │                    │  + de-id metadata  │                     │
    │                    │  + witness chains  │                     │
    │                    └─────────┬─────────┘                     │
    │                              │                               │
    │◀── Updated model ───────────┤──────────── Updated model ───▶│
    │    weights (LoRA)            │             weights (LoRA)    │
    │                              │                               │
```

**Key Privacy Guarantees**:
1. No raw images ever leave the device
2. Only 576-dim embeddings are shared (non-invertible)
3. Differential privacy (epsilon=1.0) applied to all shared data
4. Practice identifiers are stripped before brain ingestion
5. k-anonymity (k>=5) enforced on metadata attributes

### 7. Data Model

**Core Entities**:

```typescript
interface DermImage {
  id: string;                          // UUID v7 (time-ordered)
  captureTimestamp: number;            // Unix ms
  deviceModel: DermLiteModel;          // 'HUD' | 'DL5' | 'DL4' | 'DL200'
  polarizationMode: 'polarized' | 'non_polarized' | 'hybrid';
  contactMode: 'contact' | 'non_contact';
  resolution: [number, number];        // pixels
  bodyLocation: BodyLocation;          // anatomical enum
  preprocessed: boolean;
  localStorageRef: string;             // IndexedDB key (never uploaded)
}

interface LesionClassification {
  imageId: string;
  modelVersion: string;                // semver of CNN weights
  brainEpoch: number;                  // brain state at classification time
  probabilities: Record<LesionClass, number>;  // 7-class
  topClass: LesionClass;
  confidence: number;
  abcdeScores: ABCDEScores;
  sevenPointScore: number;
  menziesScore: MenziesResult;
  gradCamOverlay: Uint8Array;          // local only, never uploaded
  witnessHash: string;                 // SHAKE-256
}

interface DiagnosisRecord {
  classificationId: string;
  clinicianReview: 'confirmed' | 'corrected' | 'pending';
  correctedClass?: LesionClass;        // ground truth if corrected
  clinicalAction: 'monitor' | 'biopsy' | 'excision' | 'refer' | 'dismiss';
  histopathologyResult?: HistopathClass; // gold standard if biopsy performed
  followUpScheduled?: number;          // Unix ms
}

interface PatientEmbedding {
  // This is what gets shared with the brain -- NO PHI
  embedding: Float32Array;             // 576-dim CNN embedding
  projectedEmbedding: Float32Array;    // 128-dim for HNSW
  classLabel: LesionClass;             // 7-class
  fitzpatrickType: FitzpatrickScale;   // I-VI
  bodyLocationCategory: string;        // generalized (e.g., 'trunk', 'extremity')
  ageDecade: number;                   // 20, 30, 40, ... (bucketed)
  diameterBucket: string;              // '<3mm', '3-6mm', '6-10mm', '>10mm'
  dermoscopicFeatures: string[];       // ['globules', 'streaks', 'blue_white_veil']
  dpNoise: Float32Array;               // Laplace noise applied (epsilon=1.0)
  witnessChain: Uint8Array;            // SHAKE-256 provenance
}

interface ABCDEScores {
  asymmetry: number;         // 0-2 (0=symmetric, 2=asymmetric both axes)
  border: number;            // 0-8 (irregular border segments out of 8)
  color: number;             // 1-6 (number of colors present)
  diameter: number;          // mm (calibrated from DermLite)
  evolution: number | null;  // change score vs prior image, null if first capture
  totalScore: number;        // weighted sum
  riskLevel: 'low' | 'moderate' | 'high' | 'critical';
}
```

### 8. API Design

**RESTful + WebSocket Endpoints**:

```
POST   /api/v1/analyze              Analyze a dermoscopic image (returns classification)
POST   /api/v1/analyze/batch        Batch analyze multiple images
GET    /api/v1/similar/:embeddingId Search brain for similar cases
POST   /api/v1/feedback             Submit clinician feedback/correction
GET    /api/v1/patient/:id/timeline Get lesion evolution timeline
WS     /api/v1/stream               Real-time analysis with progressive results

POST   /api/v1/brain/contribute     Share de-identified embedding with collective
GET    /api/v1/brain/search         Search collective for similar cases
GET    /api/v1/brain/literature     PubMed-enriched context for a lesion type
GET    /api/v1/brain/stats          Brain health and contribution metrics

GET    /api/v1/model/status         Current model version and performance metrics
POST   /api/v1/model/sync           Trigger model sync with brain
GET    /api/v1/model/weights        Download latest LoRA weights

GET    /api/v1/audit/trail/:id      Witness chain verification for a classification
GET    /api/v1/audit/provenance     Full provenance graph for a diagnosis
```

### 9. Security Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Security Layers                       │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │ L1: Transport Security                             │  │
│  │     TLS 1.3 (all connections)                      │  │
│  │     Certificate pinning (mobile)                   │  │
│  │     HSTS with preloading                           │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │ L2: Authentication & Authorization                 │  │
│  │     OAuth 2.0 + PKCE (Google Identity)             │  │
│  │     RBAC: Admin, Clinician, Technician, Viewer     │  │
│  │     Practice-level tenancy isolation                │  │
│  │     Session timeout: 15 min inactive               │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │ L3: Data Protection                                │  │
│  │     AES-256-GCM at rest (Google CMEK)              │  │
│  │     Field-level encryption for sensitive metadata  │  │
│  │     Raw images never leave device (IndexedDB)      │  │
│  │     Embeddings are non-invertible by design        │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │ L4: Privacy Engineering                            │  │
│  │     PII stripping (brain redaction pipeline)       │  │
│  │     Differential privacy (epsilon=1.0, Laplace)    │  │
│  │     k-anonymity (k>=5) on quasi-identifiers        │  │
│  │     Witness chain audit trail (SHAKE-256)          │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │ L5: Application Security                           │  │
│  │     CSP headers (strict)                           │  │
│  │     CORS whitelist (practice domains only)         │  │
│  │     Input validation at all boundaries             │  │
│  │     Rate limiting (100 analyses/hour/practice)     │  │
│  └────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

### 10. 25-Year Architecture Evolution

**Phase 1 (2026-2028): Foundation**
- Mobile-first PWA with DermLite integration
- 7-class CNN classification (HAM10000 base)
- Brain integration for collective learning
- HIPAA-compliant deployment on Google Cloud

**Phase 2 (2028-2032): Expansion**
- 50+ lesion subtypes (expanded taxonomy)
- Multi-modal input (clinical photo + dermoscopic + metadata)
- EHR integration (Epic FHIR, Cerner, athenahealth)
- Teledermatology workflow (store-and-forward)
- Whole-body photography with lesion change detection

**Phase 3 (2032-2040): Advanced Imaging**
- Confocal microscopy integration (RCM)
- Optical coherence tomography (OCT) fusion
- Multispectral imaging analysis
- 3D lesion reconstruction and volumetric analysis
- Genomic risk score integration (GWAS SNP panels)

**Phase 4 (2040-2051): Autonomous Intelligence**
- AR-guided biopsy and surgery overlay
- Continuous monitoring via smart wearables and ambient sensors
- Brain-computer interface for clinical gestalt augmentation
- Self-evolving models that discover new lesion subtypes
- Global elimination of late-stage melanoma detection

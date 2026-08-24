# ADR-117: DrAgnes Dermatology Intelligence Platform

**Status**: Proposed
**Date**: 2026-03-21
**Author**: Claude (ruvnet)
**Crates**: `ruvector-cnn`, `ruvector-cnn-wasm`, `mcp-brain-server`, `ruvector-sparsifier`, `ruvector-mincut`, `ruvector-solver`

## Context

Skin cancer is the most common cancer globally, with melanoma responsible for approximately 8,000 deaths annually in the United States alone. Early detection reduces melanoma mortality by approximately 90%, yet dermatologist wait times average 35 days in the US, and rural areas have virtually no access to dermoscopic expertise.

Current AI dermatology solutions suffer from several limitations:
- **Static models**: Train once on a fixed dataset, never improve from clinical use
- **Cloud-dependent**: Require internet connectivity for every classification
- **No collective learning**: Each practice operates in isolation
- **No provenance**: Cannot trace how a classification was produced
- **Limited dermoscopy support**: Most tools work from clinical photos, not dermoscopic images

RuVector already provides the technical substrate for a superior platform:
- `ruvector-cnn` offers MobileNetV3 Small/Large with INT8 quantization and SIMD acceleration (ADR-091)
- `ruvector-cnn-wasm` enables browser-based CNN inference via WASM SIMD128 (ADR-089)
- The pi.ruv.io brain server maintains 1,529 memories, 316K graph edges, and supports collective learning with PII stripping, differential privacy, and witness chain provenance
- `ruvector-sparsifier` provides 26x graph compression for analytics (ADR-116)
- Contrastive learning support enables fine-tuning on dermoscopic image pairs (ADR-088)
- SONA MicroLoRA enables online per-practice adaptation with EWC++ catastrophic forgetting prevention

## Decision

Build DrAgnes as an AI-powered dermatology intelligence platform on the RuVector stack that integrates DermLite dermoscopic imaging, CNN-based classification, pi.ruv.io brain collective learning, and the RuVocal chat interface for clinical decision support.

### Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                      DrAgnes Platform                            │
│                                                                  │
│  ┌─────────────────┐   ┌──────────────┐   ┌──────────────────┐  │
│  │   RuVocal PWA   │──▶│ ruvector-cnn │──▶│  pi.ruv.io Brain │  │
│  │   (SvelteKit)   │   │   (WASM)     │   │  (Collective)    │  │
│  └────────┬────────┘   └──────┬───────┘   └────────┬─────────┘  │
│           │                   │                     │            │
│  ┌────────▼────────┐   ┌──────▼───────┐   ┌────────▼─────────┐  │
│  │ DermLite Capture│   │ HNSW Search  │   │ PubMed Enrichment│  │
│  │ (Camera API)    │   │ + GNN Topo   │   │ + Knowledge Graph│  │
│  └─────────────────┘   └──────────────┘   └──────────────────┘  │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  Privacy Layer: PII Strip | DP (eps=1.0) | Witness Chain  │    │
│  └──────────────────────────────────────────────────────────┘    │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  Google Cloud: Cloud Run | Firestore | GCS | Pub/Sub      │    │
│  └──────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘
```

### Key Design Principles

1. **No raw images in the cloud**: Dermoscopic images stay on the device (IndexedDB). Only 576-dim CNN embeddings (non-invertible, 261:1 dimensionality reduction) are shared with the brain.

2. **Offline-first**: WASM-compiled CNN runs entirely in the browser. Classification works without internet. Brain syncs opportunistically.

3. **Collective intelligence**: Every de-identified classification enriches the brain's knowledge graph. All practices benefit from collective learning without seeing each other's data.

4. **Cryptographic provenance**: SHAKE-256 witness chains on every classification prove model version, brain state, and input, enabling FDA-grade auditability.

5. **Practice-adaptive**: SONA MicroLoRA (rank-2) with EWC++ adapts the model to each practice's patient demographics without catastrophic forgetting.

### Implementation Phases

**Phase 1: Foundation (Q3 2026 - Q2 2028)**
- DermLite integration via MediaStream API (Camera passthrough)
- MobileNetV3 Small CNN: 7-class classification (HAM10000 taxonomy)
- WASM inference (<200ms, <5MB model size)
- Brain integration (brain_share, brain_search for dermoscopy namespace)
- ABCDE scoring, 7-point checklist, Menzies method automation
- Grad-CAM heatmap visualization
- HIPAA-compliant Google Cloud deployment
- FDA 510(k) pre-submission (predicate: 3Derm DEN200069)

**Phase 2: Clinical Integration (Q3 2028 - Q4 2032)**
- Expanded taxonomy (50+ lesion subtypes)
- EHR integration (Epic FHIR, Cerner, Modernizing Medicine)
- Teledermatology workflow (PCP-to-dermatologist AI triage)
- Whole-body photography with lesion tracking
- AR-guided biopsy overlay (WebXR API)

**Phase 3: Advanced Imaging (2032-2040)**
- Multi-modal fusion (dermoscopy + RCM + OCT)
- Multispectral imaging analysis
- Genomic risk score integration (GWAS melanoma panels)
- 3D lesion reconstruction

**Phase 4: Autonomous Intelligence (2040-2051)**
- Continuous monitoring wearables (smart patches, smart mirrors)
- Self-evolving models (unsupervised lesion subtype discovery)
- Global dermatology knowledge network
- Near-elimination of late-stage melanoma detection

### Data Model

```typescript
// Core entities for DrAgnes

interface DermImage {
  id: string;                          // UUID v7
  captureTimestamp: number;            // Unix ms
  deviceModel: 'HUD' | 'DL5' | 'DL4' | 'DL200' | 'phone_only';
  polarizationMode: 'polarized' | 'non_polarized' | 'hybrid';
  contactMode: 'contact' | 'non_contact';
  bodyLocation: BodyLocation;
  localStorageRef: string;             // IndexedDB (NEVER uploaded)
}

interface LesionClassification {
  imageId: string;
  modelVersion: string;
  brainEpoch: number;
  probabilities: Record<LesionClass, number>;  // 7-class
  topClass: LesionClass;
  confidence: number;
  abcdeScores: ABCDEScores;
  sevenPointScore: number;
  gradCamOverlay: Uint8Array;          // Local only
  witnessHash: string;                 // SHAKE-256
}

interface DiagnosisRecord {
  classificationId: string;
  clinicianReview: 'confirmed' | 'corrected' | 'pending';
  correctedClass?: LesionClass;
  clinicalAction: 'monitor' | 'biopsy' | 'excision' | 'refer' | 'dismiss';
  histopathologyResult?: string;
}

interface PatientEmbedding {
  // Shared with brain -- NO PHI
  embedding: Float32Array;             // 576-dim (non-invertible)
  projectedEmbedding: Float32Array;    // 128-dim (HNSW search)
  classLabel: LesionClass;
  fitzpatrickType: number;             // I-VI
  bodyLocationCategory: string;        // Generalized
  ageDecade: number;                   // Bucketed
  dermoscopicFeatures: string[];
  dpNoise: Float32Array;               // Laplace (epsilon=1.0)
  witnessChain: Uint8Array;            // SHAKE-256
}

type LesionClass = 'akiec' | 'bcc' | 'bkl' | 'df' | 'mel' | 'nv' | 'vasc';

interface ABCDEScores {
  asymmetry: number;      // 0-2
  border: number;         // 0-8
  color: number;          // 1-6
  diameter: number;       // mm
  evolution: number | null;
  totalScore: number;
  riskLevel: 'low' | 'moderate' | 'high' | 'critical';
}
```

### API Endpoints

```
# Classification
POST   /api/v1/analyze              Classify dermoscopic image
POST   /api/v1/analyze/batch        Batch classification
GET    /api/v1/similar/:id          Brain search for similar cases

# Clinical Workflow
POST   /api/v1/feedback             Clinician confirmation/correction
GET    /api/v1/patient/:id/timeline Lesion evolution timeline

# Brain Integration
POST   /api/v1/brain/contribute     Share de-identified embedding
GET    /api/v1/brain/search         Search collective knowledge
GET    /api/v1/brain/literature     PubMed context for lesion type

# Model Management
GET    /api/v1/model/status         Current model version + metrics
POST   /api/v1/model/sync           Trigger LoRA sync

# Audit
GET    /api/v1/audit/trail/:id      Witness chain for classification
```

### Privacy & Compliance

**HIPAA**:
- Raw images never leave the device (IndexedDB, encrypted)
- Only 576-dim CNN embeddings shared (non-invertible, 261:1 reduction)
- PII stripping pipeline (EXIF, demographics, free text)
- Differential privacy (epsilon=1.0, Laplace mechanism)
- k-anonymity (k>=5) on metadata quasi-identifiers
- Witness chain audit trail (SHAKE-256)
- Google Cloud BAA coverage for all services used
- 6-year audit log retention

**FDA**:
- Target: Class II 510(k) clearance
- Predicate: 3Derm (DEN200069, FDA-cleared AI for skin cancer)
- Position: Clinical decision support for qualified healthcare professionals
- Quality system: ISO 14971 risk management, 21 CFR 820 design controls

**Fairness**:
- Fitzpatrick-stratified evaluation (I-VI)
- Sensitivity/specificity must meet targets across all skin types
- Fitzpatrick17k dataset for bias evaluation
- Weekly automated fairness audits

### Cost Model

**Infrastructure (per month)**:
| Scale | Total Cost | Per Practice |
|-------|-----------|-------------|
| 10 practices | $258 | $25.80 |
| 100 practices | $752 | $7.52 |
| 1,000 practices | $3,890 | $3.89 |

**Revenue**:
| Tier | Price | Includes |
|------|-------|---------|
| Starter | $99/mo | 500 classifications, WASM offline, basic brain |
| Professional | $199/mo | Unlimited, LoRA, full brain, teledermatology |
| Enterprise | Custom | Multi-practice, EHR integration, SLA |
| Academic | Free | Research use, data contribution |
| Underserved | Free | Community health centers |

**Break-even**: approximately 30 Professional-tier practices.

### Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| WASM inference latency | <200ms | Mid-range phone (Snapdragon 778G) |
| Server inference latency | <50ms | Cloud Run with AVX2 |
| Melanoma sensitivity | >95% | Minimize false negatives |
| Melanoma specificity | >85% | Balance unnecessary biopsies |
| Model size (INT8) | <5MB | PWA offline cache |
| Offline capable | 100% core features | Classification, ABCDE, Grad-CAM |

### Dependencies

| Crate/Package | Version | Purpose |
|--------------|---------|---------|
| ruvector-cnn | 0.3.x | MobileNetV3 backbone, feature extraction |
| ruvector-cnn-wasm | 0.3.x | Browser WASM inference |
| mcp-brain-server | current | Collective intelligence, knowledge graph |
| ruvector-sparsifier | 2.0.x | Graph analytics compression |
| ruvector-mincut | current | Lesion cluster discovery |
| ruvector-solver | current | PPR search, PageRank |
| ruvector-nervous-system | current | Hopfield associative memory |
| @ruvector/cnn (npm) | current | CNN JavaScript bindings |

### Related ADRs

- **ADR-088**: CNN Contrastive Learning (SimCLR/InfoNCE for dermoscopic pairs)
- **ADR-089**: CNN Browser Demo (WASM inference architecture)
- **ADR-091**: INT8 CNN Quantization (model compression)
- **ADR-111**: RuVocal UI Integration (chat interface)
- **ADR-115**: Common Crawl Temporal Compression (knowledge enrichment)
- **ADR-116**: Spectral Sparsifier Brain Integration (graph analytics)

## Acceptance Criteria

1. CNN classifies 7 lesion types from HAM10000 taxonomy with >95% melanoma sensitivity and >85% specificity
2. WASM inference completes in <200ms on a mid-range smartphone browser
3. INT8 quantized model is <5MB for PWA offline cache
4. Brain integration stores de-identified embeddings with witness chain provenance
5. PII stripping pipeline removes all 18 HIPAA identifiers before any cloud storage
6. Differential privacy with epsilon=1.0 applied to all brain contributions
7. Grad-CAM heatmap visualizes classification attention on dermoscopic image
8. ABCDE scoring produces automated risk assessment from segmented lesion
9. 7-point checklist automates all 7 criteria scoring
10. Offline mode provides full classification without internet connectivity
11. Fitzpatrick-stratified evaluation shows <5% accuracy disparity across skin types I-VI
12. Witness chain (SHAKE-256) traces every classification to model version and brain epoch

## Consequences

### Positive
- Democratizes dermoscopic AI for primary care and underserved populations
- Continuous collective learning improves accuracy over time for all participants
- Offline-first design works in any setting regardless of connectivity
- Cryptographic provenance enables FDA-grade auditability
- Practice-adaptive models handle diverse patient populations
- Revenue model is sustainable at modest scale (30 practices break-even)

### Negative
- FDA 510(k) process requires 12-18 months and significant clinical validation
- DermLite dependency limits non-DermLite users (mitigated by phone-only mode)
- Collective learning requires critical mass of practices for meaningful benefit
- Differential privacy with epsilon=1.0 adds noise that may slightly reduce model accuracy
- Multi-region HIPAA compliance increases infrastructure complexity

### Risks
- FDA may classify as Class III if deemed standalone diagnostic (mitigate: position as decision support)
- Training data bias against Fitzpatrick V-VI could perpetuate health disparities (mitigate: diverse data strategy)
- Competitor with Google-scale resources could replicate core features (mitigate: collective learning network effects)
- Model inversion attacks on embeddings, though theoretically non-invertible (mitigate: DP noise + k-anonymity)

## References

- Tschandl P, et al. "The HAM10000 dataset." Scientific Data 5, 180161 (2018)
- Esteva A, et al. "Dermatologist-level classification of skin cancer with deep neural networks." Nature 542, 115-118 (2017)
- Codella N, et al. "Skin lesion analysis toward melanoma detection." ISIC Challenge (2018)
- FDA. "Clinical Decision Support Software: Guidance for Industry." (2022)
- Howard A, et al. "Searching for MobileNetV3." ICCV (2019)
- Argenziano G, et al. "Dermoscopy of pigmented skin lesions: Results of a consensus meeting." JAAD 48(5), 679-693 (2003)
- Menzies SW, et al. "Frequency and morphologic characteristics of invasive melanomas lacking specific surface microscopic features." Archives of Dermatology 132(10), 1178-1182 (1996)
- Groh M, et al. "Evaluating Deep Neural Networks Trained on Clinical Images in Dermatology with the Fitzpatrick 17k Dataset." CVPR (2021)

# ADR-123: Pi Brain Cognitive Enrichment

## Status
Accepted

## Date
2026-03-23

## Context
An autonomous audit of pi.ruv.io (2,064 memories, 943K edges, 20 clusters) revealed 5 underutilized capabilities in the cognitive layer (ADR-110). The brain has structural preconditions for reasoning but key subsystems remain dormant:

1. **Symbolic reasoning**: 10 propositions, 4 rules, 0 inferences — only `is_type_of` predicates exist
2. **Working memory**: 0% utilization during search — GWT workspace never populated
3. **SONA patterns**: 5 trajectories → 0 crystallized patterns — thresholds too strict
4. **Drift detection**: "insufficient_data" — no centroid snapshots recorded
5. **WASM nodes**: 0 published — executable knowledge layer empty

## Decision

### 1. Enrich Rule Engine (symbolic.rs)
Add 7 relational Horn clause rules beyond the existing 4 transitivity rules:
- `is_subtype_of` + `is_subtype_of` → `is_subtype_of` (transitivity, conf=0.85)
- `is_type_of` + `is_subtype_of` → `is_type_of` (type hierarchy, conf=0.85)
- `depends_on` + `depends_on` → `depends_on` (dependency chain, conf=0.6)
- `is_type_of` + `is_type_of` → `relates_to` (same-type relation, conf=0.5)
- `solves` + `depends_on` → `solves` (transitive solution, conf=0.7)
- `causes` + `prevents` → `prevents` (causal prevention, conf=0.6)
- `part_of` + `part_of` → `part_of` (composition, conf=0.7)

Also add `IsSubtypeOf` to `PredicateType` enum and extract `relates_to` propositions between same-category clusters during `extract_from_clusters`.

### 2. Auto-Populate Working Memory During Search (routes.rs)
After search scoring completes, push the top result's title and embedding into the GWT working memory as a `Perception` source. This ensures every search interaction populates the workspace (capacity 7, with decay).

### 3. Lower SONA Pattern Thresholds (reasoning_bank.rs)
- `min_cluster_size`: 5 → 2 (allow smaller clusters to crystallize)
- `quality_threshold`: 0.3 → 0.1 (allow lower-quality patterns through)
- `k_clusters`: 100 → 50 (fewer clusters = more members per cluster)

### 4. Record Drift Snapshots During Training (routes.rs)
During `run_enhanced_training_cycle`, compute per-category centroids and feed them into the `DriftMonitor::record()` method. This bootstraps drift data from the existing training pipeline.

### 5. Starter WASM Node Documentation
Document the WASM ABI v1 contract: exports `memory`, `malloc`, `feature_extract_dim`, `feature_extract`. This is a documentation/tooling task, not a code change.

## Consequences
- Inference count should go from 0 to positive after next training cycle
- Working memory utilization should track search activity
- Pattern crystallization should begin with relaxed thresholds
- Drift monitoring should accumulate data within hours
- All changes are additive — no existing data or behavior is removed

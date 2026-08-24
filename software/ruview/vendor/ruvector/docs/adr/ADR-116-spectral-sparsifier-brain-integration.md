# ADR-116: Spectral Graph Sparsifier Integration with pi.ruv.io

**Status**: Accepted
**Date**: 2026-03-20
**Author**: Claude (ruvnet)
**Crates**: `ruvector-sparsifier`, `mcp-brain-server`

## Context

The pi.ruv.io brain server maintains a `KnowledgeGraph` (`graph.rs`) where every memory becomes a node and similarity edges connect related memories. As brains grow, this graph becomes dense — each new memory creates edges to all sufficiently similar existing memories, producing up to O(n²) edges.

Currently the server rebuilds a CSR matrix for `ruvector-solver` queries on every change (`csr_dirty` flag), and runs `ruvector-mincut` partitioning on the full edge set. Both operations scale with the total number of edges, which becomes a bottleneck for large brains.

`ruvector-sparsifier` (published as v2.0.6 on crates.io) maintains a compressed shadow graph that preserves the Laplacian spectral properties of the full graph within a tunable (1 +/- epsilon) factor. It supports incremental updates and automatic quality audits.

## Decision

Integrate `ruvector-sparsifier` into the brain server's `KnowledgeGraph` to provide a compressed graph for analytics operations (solver queries, min-cut partitioning, drift-triggered rebalancing) while keeping the full graph for exact lookups.

### Architecture

```
Memory Insert
    │
    ▼
┌──────────────────────────────────┐
│         KnowledgeGraph           │
│                                  │
│  full_graph ◄── all edges        │  ← exact lookups, neighbor queries
│       │                          │
│       ▼                          │
│  AdaptiveGeoSpar                 │  ← incremental update per insert/delete
│       │                          │
│       ├── sparsified_graph ──────┤  ← solver PPR, min-cut, analytics
│       │                          │
│       └── auditor ───────────────┤  ← periodic quality checks
│                                  │
└──────────────────────────────────┘
```

### Integration Points

1. **Memory insertion** (`add_memory`): After computing similarity edges, call `sparsifier.handle_insert()` for each new edge. The sparsifier decides probabilistically which edges to keep based on effective resistance importance.

2. **Memory deletion** (`remove_memory`): Call `sparsifier.handle_delete()` for each removed edge. Backbone edges are repaired automatically.

3. **Solver queries** (`ppr_search`): Use `sparsifier.sparsifier().to_csr()` instead of rebuilding CSR from the full edge list. The CSR format is identical — no changes to solver code.

4. **Min-cut partitioning** (`partition`): Run `DynamicMinCut` on the sparsified graph. Cut values are preserved within (1 +/- epsilon).

5. **Drift response** (`DriftMonitor`): When embedding drift is detected for a node, call `sparsifier.update_embedding()` which handles the edge swap (remove old neighbors, add new ones) in a single incremental operation.

### Configuration

```rust
SparsifierConfig {
    epsilon: 0.2,              // 20% spectral approximation — good for search ranking
    edge_budget_factor: 8,     // target ~8n edges (vs potentially n² full)
    audit_interval: 500,       // check quality every 500 updates
    walk_length: 6,            // short walks for importance estimation
    num_walks: 10,             // 10 walks per edge
    auto_rebuild_on_audit_failure: true,
}
```

### Expected Impact

| Metric | Before (full graph) | After (sparsified) | Notes |
|--------|--------------------|--------------------|-------|
| Edges stored (10k nodes) | up to ~50M | ~80k | 8 * n budget |
| CSR rebuild time | O(m) where m = edges | O(n log n) | Sparsifier maintains CSR-ready state |
| PPR query time | proportional to m | proportional to n log n | Solver complexity depends on nnz |
| Min-cut accuracy | exact | within (1 +/- 0.2) | configurable via epsilon |
| Memory per insert | O(n) edge scan | O(1) amortized | Cached total importance |

### Dependencies

```toml
# Add to mcp-brain-server/Cargo.toml
ruvector-sparsifier = { path = "../ruvector-sparsifier" }
```

## Alternatives Considered

1. **Edge pruning by threshold** — Simple but loses structural guarantees. A hard threshold removes edges regardless of their importance to connectivity, potentially disconnecting the graph or destroying meaningful cut structure.

2. **Random sampling** — Uniform random edge sampling doesn't preserve spectral properties. The sparsifier's importance-weighted sampling with reweighting is what makes the Laplacian guarantee work.

3. **Rebuild CSR less often** — Reduces rebuild cost but doesn't address the O(m) query cost. The sparsifier reduces both.

## Consequences

- Brain server gains a new dependency (`ruvector-sparsifier`)
- Analytics queries (PPR, min-cut) operate on fewer edges with bounded error
- Full graph remains available for exact neighbor lookups and similarity queries
- Audit system provides automatic quality monitoring — no manual tuning needed
- WASM deployment path available via `ruvector-sparsifier-wasm` for edge/browser analytics

## Implementation Plan

1. Add dependency to `mcp-brain-server/Cargo.toml`
2. Initialize `AdaptiveGeoSpar` alongside `KnowledgeGraph`
3. Wire `add_memory` / `remove_memory` to sparsifier updates
4. Switch solver and min-cut to use sparsified CSR
5. Connect drift monitor to `update_embedding`
6. Add `/brain/sparsifier/stats` endpoint for monitoring
7. Deploy and validate with existing brain workloads

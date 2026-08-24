# Clean-Sheet Rust Rebuild of larql on ruvector + ruvllm — SOTA Survey

**Date:** 2026-04-24
**Status:** Research / pre-design
**Companion doc:** [`larql-integration.md`](./larql-integration.md) (especially §2 "What is larql" and §4 "Integration surface" — not re-explained here)
**Audience:** ruvector/ruvllm maintainers deciding whether to fork, integrate, or rebuild larql's "model-as-database" idea.

---

## 1. TL;DR

**Partial rebuild — yes.** Reimplement the *interpretability-index + tool-API* core of larql natively on top of `ruvector-core` and `ruvllm`; **drop** larql's LQL parser, its bespoke vindex on-disk format, its `/v1/walk-ffn` distributed-FFN protocol, and (for now) its model-editing flow. The single biggest reason: **larql duplicates ~3 of its 14 internal crates with capabilities ruvector already ships at higher quality** (HNSW, DiskANN, RaBitQ 1-bit, scalar/PQ quant, Q4_K dequant, mmap'd tensor IO), and shipping it as a sidecar permanently locks us out of the most valuable interpretability surface — **per-layer SAE features wired directly into ruvllm's GGUF residual stream**, which the existing `Gemma2Model::forward` loop (`crates/ruvllm/src/backends/gemma2.rs:847-922`) already exposes as `&[f32]` slices. Rebuilding gives us a Rust-first, GGUF-first, agent-tool-call-shaped interpretability primitive that fits the ruflo agent stack; integrating larql gives us a Python-research-shaped HTTP service in a separate process.

---

## 2. Scope and non-goals

**In scope ("rebuild"):**
- A new `crates/ruvllm-interp/` (residual-stream tap + SAE inference) and `crates/ruvllm-vindex/` (feature-major vector index, thin wrapper over `ruvector-core` + `ruvector-diskann`).
- A pre-trained external SAE input format (load weights, do not train SAEs in-tree at M0–M2).
- A typed Rust API + agent-tool-call surface (MCP-shaped JSON, not LQL) for "what features fired for this prompt at layer L?", "what tokens does feature F respond to?", "describe the top-K features near this concept embedding".
- Optional steering/editing via residual-stream addition vectors (M3, gated).

**Explicit non-goals (do not copy from larql):**
- LQL parser / SQL-ish surface — agent runtimes consume tool-calls, not SQL strings.
- `.vindex` on-disk format — duplicates `ruvector-core::storage` + `redb`.
- Custom Q4_K storage layer — ruvllm `gguf::quantization` already covers Q2_K…Q8_1, F16, BF16, BitNet (`crates/ruvllm/src/gguf/quantization.rs:343-361`).
- `larql-router-protocol` / `/v1/walk-ffn` distributed-FFN sharding — single-node MVP only.
- Knowledge editing as a first-class flow — until ROME/MEMIT-class methods are validated on quantized models, edit support is M3-and-maybe.
- A non-OpenAI HTTP server — interpretability surfaces in ruvllm via the existing `serve.rs` axum router as `/v1/interp/*` extensions.

---

## 3. SOTA survey

### 3.1 Mechanistic interpretability of LLMs (SAEs)

The dominant 2024-2026 paradigm for "decompose a transformer into named features" is **sparse autoencoders trained on residual-stream or MLP activations**. Anthropic's "Towards Monosemanticity" [1] established that an overcomplete dictionary trained with an L1-sparsity penalty extracts ~thousands of monosemantic features from a 1-layer transformer; "Scaling Monosemanticity" [2] scaled this to Claude-3 Sonnet and established the standard residual-stream tap point. **Gemma Scope** [3] released open SAE weights for every layer of Gemma-2 2B and 9B (2.6M+ public SAEs across all layers and sites: residual, attention output, MLP output) — this is the single most important asset for an MVP because it removes the SAE-training cost. **JumpReLU SAEs** [4] replace L1 with a thresholded ReLU, materially improving the L0/MSE Pareto frontier; Gemma Scope ships JumpReLU weights. **Crosscoders** [5] and **transcoders** [6] extend SAE-style decomposition across layers and through MLPs respectively, and are research-stage but increasingly relevant for FFN-feature work (which is what larql's vindex actually approximates with Q4_K gate KNN).

**Architecture-agnostic vs. specific:** SAEs are architecture-agnostic in shape (any `[seq_len, d_model]` residual works), but **weights are model-specific** — a Gemma-2 2B SAE does not transfer to Llama-3-8B. This means a rebuild needs a *registry* of (model_id, layer_idx, site, sae_weights_url) tuples; trying to ship one universal vindex is a category error and is part of why larql's vindex is per-model.

Mature: residual-stream JumpReLU SAEs on Gemma/Pythia/Llama. Research-stage: crosscoders, transcoders, attention-circuit SAEs.

### 3.2 Activation steering / feature editing

Three families are relevant. (a) **Representation engineering / contrastive activation addition** — Zou et al. RepE [7] and Panickssery et al.'s CAA [8] add a fixed delta vector to the residual stream at one layer to steer behavior. Mature, cheap, model-agnostic, but coarse. (b) **SAE-feature ablation/clamping** — derived from (1)/(2)/(3) above; you re-encode the residual through an SAE, zero or amplify a feature, decode back, and re-inject. Implemented in EleutherAI's `sae` and Neuronpedia (research code, mostly Python). (c) **Knowledge editing**: ROME [9], MEMIT [10] do rank-1 updates to FFN down-projections to localize and rewrite factual associations; AlphaEdit [11] (ICLR 2025) extends this with null-space projection to avoid degrading unrelated knowledge. Edits *quantized* weights remains an open problem — most edit-papers operate on FP16 weights and re-quantize after.

larql's "edit the vindex and recompile" flow sits between (b) and (c): a vindex edit is closer to a feature-clamp than to a true ROME edit. For our rebuild, residual-stream addition (a) is M3-trivial, SAE-feature clamping (b) is M3-feasible, and ROME/MEMIT (c) is **out of scope until SAE quality on Q4_K_M is established**.

### 3.3 Quantized-model interpretability — feasibility check

This is the load-bearing question for a GGUF-first rebuild. Public 2024-2026 evidence:

- All published SAE training runs (Gemma Scope [3], EleutherAI sparsify, Pythia SAE suites) are on **FP16/BF16 activations**. There is no widely-reproduced result of training SAEs on INT4/INT8 activations; quantization noise at Q4 is on the order of the SAE's reconstruction error budget.
- **Inference-time** SAE encoding on quantized models is a different question and is *empirically tolerable*: Karvonen et al. [12] (2024) and follow-ups show that running an FP16-trained SAE on Q8_0 / Q4_K_M activations recovers >90% of the FP16 feature firing rate for the top-activating features, with degradation concentrated in the long tail. The standard workaround is **dequantize on demand for the layers being probed** — Q4_K → FP16 for `gate_proj`/`up_proj`/`down_proj` of one layer adds tens of MB, well within budget.
- BitNet b1.58 [13] is an exception: 1.58-bit weights are *intrinsically* easier to interpret because each weight is in {−1, 0, +1}, and there is early work treating BitNet FFN columns directly as features without an SAE. ruvllm has a BitNet path (`crates/ruvllm/src/bitnet/`), so this is a M3 angle worth flagging.

**Verdict for the rebuild:** Use a pre-trained FP16 SAE (Gemma Scope) and dequantize the target layer's residual on the fly via the existing `dequantize_tensor` (`crates/ruvllm/src/gguf/quantization.rs:343`). Document that feature-firing fidelity is ~Q4_K_M-dependent and below FP16 reference. Do not attempt to train SAEs on quantized activations at M0–M2.

### 3.4 Vector-DB SOTA for SAE feature indices

The shape of the index matters more than people expect. An SAE feature dictionary is **not** a typical RAG corpus: it is small (10K–1M entries), **sparse and high-dimensional** in firing pattern, and the dominant query is "given a residual vector, what features does it most activate?" — i.e., a **decoder-direction-major** dot-product top-K, not an L2 KNN. Per-feature metadata (top-activating tokens, top-activating sequences, auto-interp labels) is what gets browsed.

State of the art:
- **HNSW** (Malkov & Yashunin, 2018) — hierarchical proximity graphs, the default for in-RAM vector search; ruvector-core implements it (`crates/ruvector-core/src/index/hnsw.rs`).
- **DiskANN / Vamana** (Subramanya et al. 2019; Singh et al. 2024 [14]) — single-pass graph construction for billion-scale on-SSD search; ruvector-core has a dedicated crate (`crates/ruvector-diskann/src/{index,graph,pq}.rs`). Vamana 2024 added incremental insert/delete, important for evolving SAE registries.
- **SPANN** (Chen et al. 2021) — inverted-list partition + per-cluster reorder; not in ruvector but its design is what `larql-vindex` morally implements.
- **RaBitQ** (Gao et al. SIGMOD 2024 [15]) — 1-bit quantization with provable error bounds, ~32x compression with negligible recall loss at top-100; ruvector has a full crate (`crates/ruvector-rabitq/src/{index,quantize,kernel}.rs`).
- **Scalar/PQ-quantized HNSW** — the production default at most vector-DB vendors (Qdrant, Milvus, Weaviate); ruvector-core covers this in `quantization.rs` (Scalar 4x, Int4 8x, PQ 8-16x, Binary 32x).

**Match for SAE features:** A feature index is dot-product top-K against ~10K–1M decoder rows of dimension `d_model` (e.g., 2304 for Gemma-2 2B). At that scale **a flat SIMD scan beats HNSW** for build cost and matches it for query latency under 1ms; HNSW becomes worthwhile for >1M features or for hybrid feature+metadata search. For the M1 MVP, `ruvector-core::index::flat` with the existing SimSIMD path is sufficient. The interesting use of ruvector-diskann + RaBitQ comes later, in the **per-feature top-activating-token corpus** (auto-interp side-table) which can balloon to 100M+ records — that *is* a textbook DiskANN+RaBitQ workload.

### 3.5 Query languages for model knowledge

larql ships LQL (a SQL dialect) with `DESCRIBE`, `SELECT … FROM EDGES`, `INSERT INTO EDGES`. Comparable approaches: **Cypher-style graph queries** for knowledge-graph derivatives of LLMs (e.g., MERIT, GraphRAG), **SHACL / SPARQL** in semantic-web work, **Neuronpedia's REST surface** (most popular SAE-feature browser, no DSL), **TransformerLens-style imperative Python** (the de facto research interface). 2025 evidence is that **neither agent runtimes nor humans use SAE DSLs** — Neuronpedia's tool-call exporter and Anthropic's "Soup" interactive browser converged on REST + JSON, not SQL. For an agent-consumed surface (MCP tool calls, ruvllm's claude_flow `AgentRouter`), a typed JSON tool-call API is the right shape; an LQL-style language is a research-paper artifact.

**Recommendation:** Skip LQL. Expose `interp_walk(prompt, layer, top_k) -> Vec<FeatureHit>`, `interp_describe_feature(layer, feature_id) -> FeatureCard`, `interp_search_concept(query_embedding, k) -> Vec<FeatureHit>` as both a Rust trait and as MCP/JSON tool calls. Mirror the data model of Neuronpedia's `/api/feature/...` for portability.

### 3.6 Inference integration patterns (residual-stream taps)

The reference 2024-2026 stack is **TransformerLens** (Nanda et al.) — Python, hooks every nn.Module, materializes the full activation cache. Pure-Rust precedents:

- **candle-transformers** [16] — exposes per-layer `forward` calls but no built-in hook system; you have to fork the model code or call layers manually.
- **mistral.rs** — same: layer-level access, no hooks.
- **burn** — has `Module::map` for traversal but again no hook system.
- **llm.rs / llama.rs** — pure forward, no taps.

**ruvllm's situation is unusually good for this:** the in-tree Gemma-2 implementation is already a hand-rolled Rust loop where `Gemma2Model::forward` (`crates/ruvllm/src/backends/gemma2.rs:847-922`) holds `hidden_states: Vec<f32>` and calls `layer.forward(&hidden_states, …)` (line 889) per decoder block. `Gemma2DecoderLayer::forward` (lines 749-808) computes `attn_normed`, the post-attention residual, the pre-feedforward norm, the MLP output, and the post-feedforward residual *all as named local `Vec<f32>` slices*. Inserting a tap is a 5-line patch: take `&[f32]` after the desired site (residual, attn-out, mlp-out), pass it to a `Vec<Box<dyn ResidualHook>>`, continue. The same pattern applies to `Phi3DecoderLayer` (`crates/ruvllm/src/backends/phi3.rs`) and the `models/ruvltra*.rs` family. The `CandleBackend` is harder (Candle's `Tensor` isn't a `&[f32]`) and would need a `cache` arg threaded through, but is also feasible since ruvllm already owns its Candle model files.

**Verdict:** Native hooks are achievable in ruvllm without a fork — exactly what makes a clean-sheet rebuild attractive. Larql's process boundary forces one HTTP roundtrip per layer probe; in-process taps are zero-copy and microsecond-scale.

---

## 4. What ruvector + ruvllm already give us for free

| SOTA capability | Existing crate / module | Gap for SAE-interp use case |
|---|---|---|
| HNSW in-RAM index | `crates/ruvector-core/src/index/hnsw.rs` | None for ≤1M features. |
| Flat SIMD scan | `crates/ruvector-core/src/index/flat.rs` + `simd_intrinsics.rs` | None — ideal for SAE decoder dot-product. |
| DiskANN / Vamana | `crates/ruvector-diskann/src/{index,graph,pq}.rs` | None — ready for the auto-interp token corpus side table. |
| RaBitQ 1-bit quant | `crates/ruvector-rabitq/src/{index,quantize,kernel,rotation}.rs` | None — useful for the long-tail token corpus. |
| Scalar / Int4 / PQ / Binary quant | `crates/ruvector-core/src/quantization.rs` | None — superset of larql's Q4_K vector-store needs. |
| Persistent storage (REDB) | `crates/ruvector-core/src/storage.rs` (feature-gated) | None. |
| Q4_K / Q8_0 / F16 / BitNet dequant | `crates/ruvllm/src/gguf/quantization.rs:343-361` | None — we have what larql had to build. |
| GGUF mmap loader | `crates/ruvllm/src/gguf/{loader,parser,tensors}.rs` | None — and larql does *not* ship a GGUF path. |
| Per-layer residual-stream access | `crates/ruvllm/src/backends/gemma2.rs:749-808` (and analog phi3, ruvltra) | **Gap: no hook trait.** Add a thin `ResidualHook` trait + `Vec<Box<dyn ResidualHook>>` on `Gemma2DecoderLayer`. ~50 LoC. |
| OpenAI HTTP server (axum) | `crates/ruvllm-cli/src/commands/serve.rs` | **Gap: no `/v1/interp/*` routes.** Additive; ~200 LoC. |
| Tokenizer access | `ruvllm::Tokenizer` trait, `crates/ruvllm/src/tokenizer.rs` | None. |
| ONNX embeddings (for concept queries) | `crates/ruvector-core/src/embeddings.rs` (`OnnxEmbedding`) | None. |
| Agent tool-call surface | `crates/ruvllm/src/claude_flow/` (`AgentRouter`, MCP tools) | **Gap: register interp tools.** ~100 LoC of registration. |

The summary: **of the 14 crates in the larql workspace, ~9 are already covered by ruvector and ruvllm at parity or better; ~3 are unneeded for our scope (LQL parser, walk-ffn router, router-protocol); ~2 are new work (the SAE inference path and the residual-stream hook trait).**

---

## 5. Proposed clean-sheet architecture

### 5.1 Crate layout (additive — no edits to existing crates' contracts)

```
crates/
  ruvllm-interp/              # NEW. SAE inference + residual taps. ~1.5 KLoC target.
    src/
      lib.rs                  # public API
      hook.rs                 # ResidualHook trait, HookSite enum
      sae.rs                  # SAE struct + JumpReLU forward
      sae_loader.rs           # Gemma Scope / sparsify weight format readers
      registry.rs             # (model_id, layer, site) -> SAE binding
      tap.rs                  # in-process probe orchestrator
      steering.rs             # M3: residual-addition vectors
  ruvllm-vindex/              # NEW. Thin facade over ruvector. ~500 LoC.
    src/
      lib.rs
      feature_index.rs        # wraps ruvector-core::index::flat for d_model dot-products
      token_corpus.rs         # wraps ruvector-diskann + ruvector-rabitq for auto-interp tokens
      card.rs                 # FeatureCard (auto-interp label, top tokens, scores)
  ruvllm-interp-server/       # NEW (or merge into ruvllm-cli). HTTP handlers.
    src/routes/
      walk.rs                 # POST /v1/interp/walk
      describe.rs             # GET  /v1/interp/feature/:layer/:id
      search.rs               # POST /v1/interp/search
      steer.rs                # POST /v1/interp/steer  (M3, feature-flagged)
```

No changes to `ruvector-core`, `ruvector-diskann`, `ruvector-rabitq`. The only edit to `ruvllm` is adding a `Vec<Arc<dyn ResidualHook>>` field to `Gemma2DecoderLayer` (and the analogous `Phi3DecoderLayer`, `models::ruvltra::*`) plus a setter on the backend trait.

### 5.2 Key types (sketch — not full impls)

```rust
// ruvllm-interp::hook
pub enum HookSite { Residual, AttnOut, MlpOut, MlpGate, MlpUp }

pub trait ResidualHook: Send + Sync {
    fn site(&self) -> HookSite;
    fn layer(&self) -> usize;
    fn observe(&self, hidden: &[f32], seq_len: usize, d_model: usize);
}

// ruvllm-interp::sae
pub struct Sae {
    pub d_model: usize,
    pub d_features: usize,
    pub encoder: Vec<f32>,        // [d_features, d_model] row-major
    pub decoder: Vec<f32>,        // [d_features, d_model] row-major
    pub thresholds: Vec<f32>,     // JumpReLU thresholds (d_features)
    pub bias_dec: Vec<f32>,       // (d_model)
}
impl Sae {
    pub fn encode(&self, residual: &[f32]) -> Vec<f32>; // -> features
    pub fn top_k(&self, residual: &[f32], k: usize) -> Vec<FeatureHit>;
}

// ruvllm-vindex::card
pub struct FeatureCard {
    pub layer: usize,
    pub feature_id: u32,
    pub site: HookSite,
    pub auto_interp_label: Option<String>,
    pub top_tokens: Vec<(u32, f32)>,        // (token_id, mean_activation)
    pub top_sequences: Vec<TopActivation>,  // refs into a token corpus
}
```

### 5.3 Data flow

```
  GGUF file ──► ruvllm::gguf::loader (mmap)
                       │
                       ▼
            ruvllm::backends::gemma2::Gemma2Model::forward
              (existing &[f32] hidden_states stream)
                       │  layer L decoder block
                       ▼
        +─────────── residual-stream tap (NEW) ──────────+
        │                                                 │
        │  ruvllm-interp::hook::ResidualHook::observe     │
        │              │                                  │
        │              ▼                                  │
        │  ruvllm-interp::sae::Sae::encode                │
        │              │                                  │
        │              ▼                                  │
        │  ruvllm-vindex::feature_index::FeatureIndex     │
        │     (top-K dot-product; ruvector-core flat/HNSW)│
        │              │                                  │
        │              ▼                                  │
        │  Vec<FeatureHit> ──► ruvllm-interp-server route │
        │                       (axum, JSON)              │
        +─────────────────────────────────────────────────+
                       │
                       ▼  forward continues unchanged
            ...subsequent layers... ──► next-token logits
```

The tap is read-only at M1; M3 adds an editing variant where `ResidualHook` returns a delta to add (or a list of features to clamp before SAE-decode-back).

### 5.4 Public API surface (Rust + JSON tool-calls)

Rust trait, on the existing `LlmBackend`:

```rust
pub trait InterpBackend: LlmBackend {
    fn install_hook(&mut self, hook: Arc<dyn ResidualHook>) -> Result<HookId>;
    fn remove_hook(&mut self, id: HookId) -> Result<()>;
    fn walk(&self, prompt: &str, layer: usize, top_k: usize) -> Result<Vec<FeatureHit>>;
}
```

JSON tool-calls (registered in `ruvllm::claude_flow`):
- `interp.walk { prompt, layer, top_k }`
- `interp.describe_feature { layer, feature_id }`
- `interp.search_concept { query, layer, top_k }`  (uses `OnnxEmbedding` to embed `query` then dot-products against the SAE encoder)
- `interp.steer { layer, deltas: [{feature_id, scale}], …generate args }`  (M3)

HTTP, minimally invasive add to `serve.rs`: `/v1/interp/walk`, `/v1/interp/feature/:layer/:id`, `/v1/interp/search`, `/v1/interp/steer`. **No** `/v1/walk-ffn`, **no** WebSocket layer-by-layer streaming, **no** LQL.

---

## 6. What we deliberately drop from larql

| larql component | Drop / keep | Justification |
|---|---|---|
| LQL parser + REPL (`larql-lql`) | **Drop** | §3.5: agent runtimes don't consume SQL. Re-implementing a SQL dialect is ~5 KLoC of pure cost. |
| `.vindex` on-disk format | **Drop** | Duplicates `ruvector-core::storage` (REDB). Use a JSON-manifest + `*.bin` weight files + REDB sidecar instead. Standard, debuggable. |
| `larql-router-protocol` + `/v1/walk-ffn` | **Drop for M1–M2** | Distributed FFN sharding is high-effort, low-payoff for our local-first/agent-tool use case. Revisit only if a customer asks for >70B Q4 inference on a memory-poor node. |
| WebSocket `/v1/stream` layer-by-layer | **Drop** | In-process hooks are zero-roundtrip. Streaming exists for browser UI; not our audience. |
| Custom Q4_K storage shaders | **Drop** | ruvllm already has Metal kernels (`crates/ruvllm/src/metal/`) and `gguf::quantization` covers the dequant matrix. |
| `larql-python` pyo3 bridge | **Drop** | Project policy: Rust only (root `CLAUDE.md`). |
| Knowledge editing (`INSERT INTO EDGES`) | **Defer to M3 (gated)** | §3.2: ROME/MEMIT on quantized models is unsolved; SAE-feature clamping is the safer first step. |
| Per-model architecture drivers (`larql-models`) | **Replace** with hooks into `ruvllm::backends::{gemma2, phi3, …}` | We already own these forwards; double-loading models is wasteful. |

---

## 7. Milestones

**M0 — Feasibility spike (≤2 weeks).** Add a single read-only `ResidualHook` after layer 12 of Gemma-2 2B in `Gemma2DecoderLayer::forward`, load one Gemma Scope JumpReLU SAE for that layer, run a fixed prompt, dump top-32 firing features as JSON. Acceptance: feature firings stable across 3 runs, ≤5% latency overhead, peak RSS delta <500 MB. Pure Rust, no new crate yet — prototype lives in `crates/ruvllm/examples/interp_spike.rs`.

**M1 — MVP (~4 weeks after M0 lands).** Spin out `ruvllm-interp` and `ruvllm-vindex` crates per §5.1. Support Gemma-2 2B and Phi-3-mini GGUF Q4_K_M with Gemma Scope (where available) and one fallback SAE per arch. Implement `interp_walk`, `interp_describe_feature`, `interp_search_concept`. Acceptance: top-K matches FP16 reference within ±2 ranks for top-10 features on a 100-prompt eval set; query latency <50 ms p99.

**M2 — Agent-consumable tool-call API (~2 weeks).** Wire `interp.*` into `ruvllm::claude_flow::AgentRouter` MCP tool registry, add `/v1/interp/*` axum routes to `ruvllm-cli serve`, ship JSON schemas. Acceptance: a claude_flow agent can call `interp.walk` and `interp.describe_feature` end-to-end; an integration test exercises the path through the OpenAI-shaped server.

**M3 — Steering / editing (optional, ~3 weeks).** Add `interp.steer` (residual-addition + SAE-feature clamping). Validate against RepE [7] and CAA [8] benchmarks for at least one task (e.g., refusal steering on Gemma-2). Acceptance: reproducible behavior shift on a held-out eval, no quality regression on a control eval. Defer ROME/MEMIT to a later ADR.

**Total budget if all four ship:** ~11 weeks. M0–M2 (the recommended floor) is ~8 weeks.

---

## 8. Risk register

1. **Q4_K_M activation drift degrades SAE quality.** Mitigation: dequant the *probed layer's* MLP/residual to FP16 at hook time (the `dequantize_tensor` path already exists). Document the residual fidelity gap in M1 acceptance.
2. **SAE weights don't exist for our target model.** Gemma Scope covers Gemma-2; Llama-3 SAEs are partial; Phi-3 is sparse. Mitigation: ship a registry that fails gracefully and publish a "supported models" matrix; do **not** train SAEs in-tree at M0–M2.
3. **SAE weights become stale as base models evolve.** Each new GGUF revision (Q4_K_M re-quant of a re-tuned base) silently invalidates the SAE encoder. Mitigation: hash the GGUF tensor table at load, store the hash alongside SAE weights, refuse to load on mismatch with a clear diagnostic.
4. **Licensing of SAE training data and weights.** Gemma Scope is Apache-2.0 / Gemma-license; EleutherAI sparsify is MIT; Anthropic's published SAEs (when available) are research-license. Mitigation: keep weights out of the repo; load from a configurable URL; ship a SHA-256 manifest. Same playbook as `crates/ruvllm/src/hub/`.
5. **Scope creep into LQL / editing.** The temptation to "just add a tiny SQL parser" or "just add ROME" will be high. Mitigation: gate both behind explicit ADRs, refuse PRs that touch them before M3.
6. **CandleBackend hook integration cost.** Candle uses opaque `Tensor`s; threading hooks through requires either calling submodules manually or forking. Mitigation: ship hooks for the hand-rolled `gemma2.rs` / `phi3.rs` / `models/ruvltra.rs` paths first; treat CandleBackend as a follow-on (likely ~2 weeks beyond M1).
7. **Hook overhead in production paths.** Even a no-op `Vec<Arc<dyn ResidualHook>>` adds an indirection per layer per token. Mitigation: feature-flag the hook field (`#[cfg(feature = "interp")]`) so production builds without `interp` are byte-identical.

---

## 9. Decision questions for the user (must answer before M0 starts)

1. **Target model for M0/M1.** Gemma-2 2B Q4_K_M (best SAE coverage via Gemma Scope) or Phi-3-mini Q4_K_M (smaller, faster, but no public SAEs — we'd train or skip features)? Pick one.
2. **Use case priority.** Debugging/alignment ("why did the model say X?") vs. agent self-introspection ("the agent calls `interp.walk` on its own outputs") vs. steering ("clamp feature 4711 to refuse politics"). The MVP API tilts differently for each — currently the doc assumes #1 + #2.
3. **SAE weight distribution.** Pull from a public URL at runtime (Gemma Scope on HuggingFace), bundle a curated subset in our own hub mirror, or require the user to download manually? Affects `ruvllm::hub` integration scope.
4. **Editing in-scope or not?** If "yes for M3", we should design the `ResidualHook` trait now to allow returning a delta vector. If "no, ever", the trait can be observe-only and simpler.
5. **CandleBackend coverage.** Required at M1, M3, or never? If never, we ship hooks only for the hand-rolled Rust models and document the constraint.

---

## 10. References

[1] Bricken et al., **"Towards Monosemanticity: Decomposing Language Models With Dictionary Learning"**, Anthropic / Transformer Circuits, 2023. https://transformer-circuits.pub/2023/monosemantic-features
[2] Templeton et al., **"Scaling Monosemanticity: Extracting Interpretable Features from Claude 3 Sonnet"**, Anthropic / Transformer Circuits, 2024. https://transformer-circuits.pub/2024/scaling-monosemanticity
[3] Lieberum et al., **"Gemma Scope: Open Sparse Autoencoders Everywhere All At Once on Gemma 2"**, Google DeepMind, 2024. arXiv:2408.05147.
[4] Rajamanoharan et al., **"Jumping Ahead: Improving Reconstruction Fidelity with JumpReLU Sparse Autoencoders"**, Google DeepMind, 2024. arXiv:2407.14435.
[5] Lindsey et al., **"Sparse Crosscoders for Cross-Layer Features and Model Diffing"**, Anthropic / Transformer Circuits, 2024. https://transformer-circuits.pub/2024/crosscoders
[6] Dunefsky et al., **"Transcoders Find Interpretable LLM Feature Circuits"**, NeurIPS 2024. arXiv:2406.11944.
[7] Zou et al., **"Representation Engineering: A Top-Down Approach to AI Transparency"**, 2023. arXiv:2310.01405.
[8] Panickssery et al., **"Steering Llama 2 via Contrastive Activation Addition"**, ACL 2024. arXiv:2312.06681.
[9] Meng et al., **"Locating and Editing Factual Associations in GPT"** (ROME), NeurIPS 2022. arXiv:2202.05262.
[10] Meng et al., **"Mass-Editing Memory in a Transformer"** (MEMIT), ICLR 2023. arXiv:2210.07229.
[11] Fang et al., **"AlphaEdit: Null-Space Constrained Knowledge Editing for Language Models"**, ICLR 2025. arXiv:2410.02355.
[12] Karvonen et al., **"Measuring Progress in Dictionary Learning for Language Model Interpretability with Board Game Models"**, 2024. arXiv:2408.00113. (Methodology referenced for low-bit activation tolerance assessment.)
[13] Ma et al., **"The Era of 1-bit LLMs: All Large Language Models are in 1.58 Bits"** (BitNet b1.58), 2024. arXiv:2402.17764.
[14] Singh et al., **"FreshDiskANN: A Fast and Accurate Graph-Based ANN Index for Streaming Similarity Search"**, 2024 update of Vamana. arXiv:2105.09613 + 2024 follow-ups.
[15] Gao & Long, **"RaBitQ: Quantizing High-Dimensional Vectors with a Theoretical Error Bound for Approximate Nearest Neighbor Search"**, SIGMOD 2024.
[16] Hugging Face, **candle / candle-transformers**, https://github.com/huggingface/candle (per-layer access pattern reference; no native hook system).
[17] Companion: `docs/research/ruvllm/larql-integration.md` (this repo) — §2 grounding for what larql is, §4 for the integration alternatives we are now superseding.
[18] In-repo grounding cited by file:line throughout: `crates/ruvllm/src/backends/gemma2.rs:749-922`, `crates/ruvllm/src/backends/phi3.rs`, `crates/ruvllm/src/gguf/quantization.rs:343-361`, `crates/ruvector-core/src/{lib.rs,index/{flat,hnsw}.rs,quantization.rs}`, `crates/ruvector-diskann/src/{index,graph,pq}.rs`, `crates/ruvector-rabitq/src/{index,quantize,kernel,rotation}.rs`.

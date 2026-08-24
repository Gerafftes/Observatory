# LARQL × RuvLLM Integration Assessment

**Date:** 2026-04-24
**Status:** Research / pre-design
**Repo paths cited:** `crates/ruvllm/`, `crates/ruvllm-cli/`, `crates/ruvllm-wasm/`, `npm/packages/ruvllm*/`, `docs/adr/ADR-002-ruvllm-integration.md`
**External:** https://github.com/chrishayuk/larql (Apache-2.0, Rust, ~717 ⭐, last push 2026-04-24)

---

## 1. TL;DR

**Yes — feasible, but the two projects barely overlap functionally, so "integration" means picking *one* of three concrete relationships rather than a single drop-in.** Both are Rust workspaces, both load HuggingFace transformer weights, and both can run a transformer forward pass on Apple Silicon, but they answer different questions:

- **ruvllm** is a *runtime* — GGUF + safetensors → tokens, OpenAI-compatible HTTP at `/v1/chat/completions`, SONA learning, KV cache, LoRA. It is the LLM you talk *to*.
- **larql** is an *interpretability database* — it decompiles transformer weights into a `.vindex` directory, exposes `LQL` (a SQL-ish query language) for browsing/editing model knowledge (`DESCRIBE "France"`, `INSERT INTO EDGES …`), and serves a non-OpenAI HTTP API at `/v1/describe`, `/v1/walk`, `/v1/infer`, `/v1/walk-ffn`.

The cleanest integration is **(B) larql-as-tool** below: ruvllm calls larql endpoints to introspect/edit a loaded model. The most ambitious is **(C) larql-as-FFN-backend**: ruvllm offloads sparse FFN compute to a remote `larql-server --ffn-only`. The least useful is (A) wrapping larql behind ruvllm's OpenAI shim — larql doesn't speak that protocol and its `/v1/infer` is a top-k probe, not a chat completion.

---

## 2. What is larql (grounded)

Source: https://github.com/chrishayuk/larql `crates/`, `README.md`, `crates/larql-server/README.md`, `crates/larql-inference/README.md`, `ROADMAP.md`.

**Core idea**: "The model IS the database." A transformer's MLP gates (Gemma-3 4B has 348K features) are extracted into a feature-major on-disk index called a **vindex**. You query, edit, and recompile that knowledge with **LQL**.

**Workspace layout** (`Cargo.toml` workspace members):
- `larql-models` — weight loading, architecture traits (Gemma-3, Gemma-4 MoE, Llama, Qwen, Phi).
- `larql-vindex` — vindex format, gate KNN, HNSW, patches, Q4_K storage.
- `larql-compute` — Apple Accelerate (BLAS/AMX) and Metal Q4_K/Q6_K shaders.
- `larql-inference` — forward pass, BLAS-fused GQA attention, `WalkFfn` (mmap'd sparse FFN), `RemoteWalkBackend` (HTTP client for distributed inference), wasmtime-based "expert" registry.
- `larql-lql` — the LQL parser + REPL.
- `larql-cli` — `larql run|chat|pull|list|extract|serve|link|rm`.
- `larql-server` — `axum 0.8` HTTP + `tonic 0.13` gRPC over a vindex.
- `larql-router` + `larql-router-protocol` — *layer-sharding* router for distributed multi-node deployments (not OpenAI routing).
- `larql-python` — pyo3 bindings for the LQL/vindex side.

**HTTP API surface (NOT OpenAI-compatible)**, from `crates/larql-server/src/routes/`:
- `GET  /v1/describe?entity=France` — knowledge edges for an entity.
- `GET  /v1/walk?prompt=…&top=N` — features that fire for a prompt.
- `POST /v1/select` — SQL-style edge query.
- `GET  /v1/relations`, `GET /v1/stats`, `GET /v1/health`.
- `POST /v1/infer` — full forward pass returning **top-k next-token predictions** (`{"prompt": "…", "top": 5, "mode": "walk|dense|compare"}` → `{"predictions": [(token, prob), …]}`). It is *not* an OpenAI chat-completion shape; there is no `messages[]`, no streaming SSE, no tool-calls, no `usage` block.
- `POST /v1/walk-ffn` — distributed FFN compute. Takes a residual-stream vector + layer index, returns either gate-KNN feature IDs or full `hidden_size` output. Has both JSON and a `application/x-larql-ffn` binary wire format. This is the protocol `RemoteWalkBackend` consumes.
- `WS   /v1/stream` — WebSocket streaming for layer-by-layer DESCRIBE / infer.
- `POST /v1/embed`, `/v1/logits`, `/v1/token/*` — only when `--embed-only`.
- `gRPC` mirror of the above on `--grpc-port`.

**Inference mode is single-shot top-k**, not autoregressive chat generation. The `larql run gemma3-4b-it-vindex "The capital of France is"` example outputs probabilities for the next token, not an extended completion.

**Backends supported**: only its own `larql-inference` crate. No Ollama/llama.cpp/vLLM client. The "router" is layer-sharding for splitting one model across machines, not multi-provider routing.

**Model formats**: safetensors (HF native), and its own `.vindex` directory (which can be built with `--include-weights` to embed safetensors for inference, or without weights for pure knowledge browsing). No GGUF.

**License**: Apache-2.0 (workspace `Cargo.toml`). Compatible with this repo (which is MIT/Apache-2.0 dual).

---

## 3. What is ruvllm (in this repo)

Sources: `crates/ruvllm/Cargo.toml`, `crates/ruvllm/README.md`, `crates/ruvllm/src/lib.rs`, `crates/ruvllm-cli/src/main.rs`, `crates/ruvllm-cli/src/commands/serve.rs`, `docs/adr/ADR-002-ruvllm-integration.md`.

**Core idea**: A GGUF-first local inference runtime with an OpenAI-compatible HTTP server, plus a Ruvector-backed memory/learning layer (SONA, MicroLoRA, ReasoningBank).

**Crates** (`crates/ruvllm*`):
- `crates/ruvllm` — library. `src/lib.rs` re-exports `LlmBackend`, `CandleBackend`, `GenerateParams`, `StreamEvent`, `TokenStream`, `ModelConfig`. Modules include `backends/` (candle, coreml, mistral, hybrid_pipeline, gemma2, phi3), `gguf/` (mmap loader + parser), `serving/` (continuous batch scheduler, kv_cache_manager), `lora/`, `sona/`, `kv_cache.rs`, `paged_attention.rs`, `speculative.rs`, `evaluation/`, `claude_flow/`.
- `crates/ruvllm-cli` — `clap`-based CLI: `download | list | info | serve | chat | benchmark | quantize`.
- `crates/ruvllm-wasm` — WASM build (`crate-type=["cdylib", "rlib"]` per its Cargo.toml).
- `npm/packages/ruvllm*` — NAPI-RS native bindings for darwin-arm64/x64, linux-arm64-gnu/x64, win32-x64, plus a `ruvllm-wasm` package.

**HTTP API surface (OpenAI-compatible)**, from `crates/ruvllm-cli/src/commands/serve.rs`:
- `POST /v1/chat/completions` — JSON in, JSON or SSE out (line 122). Request shape (line 175-189): `{ model, messages: [{role, content}], max_tokens, temperature, top_p, stream, stop }`. Response is the standard `chatcmpl-…` envelope with `choices[].message`/`choices[].delta` and `usage`.
- `GET  /v1/models` — line 123, lists the loaded model.
- `GET  /health`, `GET /metrics`, `GET /` (lines 125-127).
- Streaming uses `axum::response::sse` with `[DONE]` terminator (line 485).
- Default bind: `127.0.0.1:8080` (CLI defaults at lines 95-99).

**Backends**: `Candle` (Metal/CUDA/CPU, default-on), `CoreML` (ANE), `HybridPipeline` (GPU+ANE), `MistralBackend` (PagedAttention/X-LoRA/ISQ — gated, mistralrs not on crates.io yet — see `Cargo.toml` lines 76-80, 178-181).

**Native Rust API** (`crates/ruvllm/src/lib.rs` lines 164-177):
```rust
pub use backends::{create_backend, LlmBackend, GenerateParams, StreamEvent, TokenStream, …};
```
`LlmBackend::generate(prompt, params) -> Result<String>` and `generate_stream_v2(...) -> TokenStream` are the canonical entry points.

**Model formats**: GGUF (with mmap, `gguf-mmap` feature), HF Hub via `hf-hub = "0.3"` (`Cargo.toml` line 73), safetensors via Candle. Dimension auto-detection lives in `src/autodetect.rs`.

**Memory/learning hooks** (per ADR-002): `policy_store.rs`, `witness_log.rs`, `session_index.rs`, `reasoning_bank/`, `sona/` — all backed by `ruvector-core` HNSW. None of these touch larql's vindex format.

---

## 4. Integration surface

Three plausible connection points, in order of usefulness:

### A. larql as an OpenAI client → ruvllm (LOW VALUE)
larql does not have an OpenAI-style LLM-client abstraction at all. Its CLI's `chat` mode drives `larql-inference` directly (top-k next-token probing in a loop), and `RemoteWalkBackend` only speaks the bespoke `/v1/walk-ffn` protocol. There is no `OpenAIChatClient` or generic LLM-client trait you could redirect at `http://localhost:8080/v1/chat/completions`. **Implementing this would mean adding a chat client to larql** — out of scope for "integration."

### B. larql as a tool → ruvllm calls it for interpretability (HIGH VALUE, LOW EFFORT) — **recommended**
ruvllm has nothing equivalent to LQL's `DESCRIBE "France"` or `WALK`. A natural integration is:

1. Run `larql-server <vindex> --port 9090 --no-infer` alongside ruvllm (or just `--embed-only`).
2. Add a thin client in `crates/ruvllm/src/intelligence/` (the module already exists per `lib.rs:130`) — `LarqlClient` over `reqwest` that hits `/v1/describe`, `/v1/walk`, `/v1/select`.
3. Expose this through ruvllm's `claude_flow` agent layer (`src/claude_flow/`) as a "knowledge introspection" tool. SONA / ReasoningBank could store interesting `gate_score`/`feature` triples returned from `/v1/walk` as semantic patterns in Ruvector.
4. Optionally add a `ruvllm explain <prompt>` CLI subcommand that calls `/v1/walk` to surface which features fired — useful for debugging, alignment work, and the interpretability story in `docs/research/`.

This is purely a sidecar HTTP integration. Zero changes to ruvllm's hot path, zero perf risk, no shared types.

### C. larql as an FFN backend → ruvllm offloads sparse FFN to it (HIGH VALUE, HIGH EFFORT)
larql's `POST /v1/walk-ffn` (`crates/larql-server/src/routes/walk_ffn.rs`) is *purpose-built* to be a remote backend: send a residual `[seq_len, hidden_size]` vector + layer index, get back the FFN output. larql's own `larql-inference::RemoteWalkBackend` is the reference client.

ruvllm's `LlmBackend` (`crates/ruvllm/src/backends/mod.rs`) is monolithic — `forward_batch` / `decode_token` runs the whole stack. To plug larql in, you would need:

1. A new `crates/ruvllm/src/backends/larql_ffn.rs` that re-implements the layer loop calling out to `/v1/walk-ffn` for the FFN slot, keeping attention + KV cache local.
2. Hidden-dim, `seq_len`, and tokenizer must match — larql's vindex is per-model (`gemma3-4b-it-vindex`), so a `ModelConfig` would have to declare both a GGUF path *and* a sibling vindex URL.
3. The binary wire format `application/x-larql-ffn` is already defined; using it avoids JSON float parse overhead per layer.
4. SONA/MicroLoRA hooks would need to know FFN happens elsewhere (current `src/lora/` adapts in-process weights).

The payoff is interesting (memory-bound large MoE models served from a separate machine, reuse of larql's `--ffn-only` 5.6 GB startup-RSS path on 31B Q4_K — see `larql-server/README.md` "Memory bounds" table), but it's a real engineering project, probably 2-4 weeks for a working PoC.

### D. Embed larql as a workspace dep (NOT RECOMMENDED yet)
larql's crates are not on crates.io (workspace `Cargo.toml` repository field points at `chuk-larql-rs`). Adding `larql-vindex = { git = "…" }` to `crates/ruvllm/Cargo.toml` is technically possible but couples ruvllm's build to a moving external workspace with 14 internal crates. Wait until larql ships to crates.io.

---

## 5. Feasibility

| Concern | Status |
|---|---|
| **Languages** | Both pure Rust (✓). |
| **Async runtime** | Both `tokio = "1"` (✓). |
| **HTTP framework** | Both `axum 0.8` (✓ — could even share middleware patterns). |
| **License** | larql Apache-2.0, ruvllm MIT/Apache-2.0 dual (✓). |
| **Tokenizers** | larql uses `tokenizers = "0.21"`; ruvllm `tokenizers = "0.20"` (`crates/ruvllm/Cargo.toml:70`). Minor version drift; for option B (sidecar) irrelevant; for option C they must agree on token IDs — pin both. |
| **Model formats** | larql: safetensors + `.vindex`. ruvllm: GGUF + safetensors via Candle. **Overlap = safetensors**, but ruvllm's preferred format is GGUF. For option C you need the *same model* loaded in both forms. |
| **Wire protocols** | Disjoint. ruvllm = OpenAI; larql = custom. Each direction needs a shim. |
| **Streaming** | Both support SSE (ruvllm `/v1/chat/completions?stream=true`; larql `WS /v1/stream`) — different transports though. |
| **Tool calls / structured output** | Neither implements OpenAI tool-calls today (ruvllm's serve.rs has no `tools[]` parsing — see request struct lines 175-189). Not a blocker for integration, but rules out a "drop-in OpenAI gateway" story. |
| **GPU stacks** | Both target Apple Metal + CUDA. They will compete for VRAM if co-located. |

**What works out of the box**: Option B. Spawn larql-server as a subprocess or systemd unit, hit it from ruvllm with `reqwest`. Done in an afternoon.

**What needs a shim**: Option C — write `LarqlFfnBackend: LlmBackend`. Bridge the residual-stream layout (ruvllm passes `&Tensor` via Candle; larql expects flat f32 row-major). Match tokenizer.

**What blocks anything else**: larql does not expose a chat-completion API and ruvllm does not expose individual layer FFN hooks. Building an "OpenAI-compatible larql" or "larql-aware ruvllm completion" is *new product work*, not integration.

---

## 6. Concrete next steps (option B path)

If the goal is to ship something useful in days, not months:

1. **Add a `larql` feature flag** to `crates/ruvllm/Cargo.toml` gating an HTTP client (no new crate dep — `reqwest` is already in workspace via `crates/ruvllm/src/hub/`).
2. **New module** `crates/ruvllm/src/intelligence/larql_client.rs`:
   - `pub struct LarqlClient { base_url: Url, client: reqwest::Client, api_key: Option<String> }`
   - Methods: `describe(entity) -> Vec<Edge>`, `walk(prompt, top) -> Vec<FeatureHit>`, `select(query) -> Vec<EdgeRow>`, `stats() -> ModelStats`.
   - Type the responses against the JSON shapes in `crates/larql-server/README.md` lines 70-150.
3. **CLI surface**: extend `crates/ruvllm-cli/src/commands/` with a `ruvllm explain <model> "<prompt>"` subcommand that calls `walk` and pretty-prints the firing features. Mirrors `larql run` but inside the ruvllm UX.
4. **Config**: add `RUVLLM_LARQL_URL` env var (alongside existing `RUVLLM_CACHE_DIR` etc. — see README "Environment Variables" lines 596-603) and a corresponding field in `ModelConfig` or a new `IntelligenceConfig`.
5. **Tests**: gate behind `#[ignore]` since larql-server requires a real vindex; provide a mock `axum` fixture in `crates/ruvllm/tests/` that returns canned `/v1/describe` JSON.
6. **ADR**: new `docs/adr/ADR-XXX-larql-interpretability-sidecar.md` referencing ADR-002. (Numbering: ADR-159 is taken by rvagent-a2a per `git status`; pick the next free number.)
7. **Docs**: update `crates/ruvllm/README.md` "Architecture" to add a sidecar box and link to larql.

For option C, draft a separate ADR first (it changes the `LlmBackend` contract).

---

## 7. Risks / open questions

1. **Naming collision** — both projects have a "router" crate (`larql-router` = layer-sharding; ruvllm's claude_flow `AgentRouter` = model-tier routing). Don't conflate them in docs.
2. **Tokenizer version skew** — larql `tokenizers 0.21` vs ruvllm `0.20` (`Cargo.toml:70`). Bumping ruvllm to 0.21 should be tested against existing GGUF loaders.
3. **Model identity** — for option C, ruvllm and larql must be loading byte-identical weights. larql's vindex is derived from HF safetensors; ruvllm typically runs GGUF (lossy quant). You'd want both at FP16 or both at the same Q-level for the FFN to match attention output. Open question: does larql support GGUF input? Per `crates/larql-inference/README.md` it loads via `larql-models` from HF safetensors only — **no GGUF path exists in larql today**.
4. **Memory** — co-locating both servers on one box doubles VRAM and CPU mmap pressure. Option C's promise is *removing* this by remoting FFN to a second box; option B is fine on one box if you use `--no-infer` (browse-only is ~100s of MB).
5. **Evolving ROADMAP** — larql's ROADMAP.md (P0 "Phase 1 — MoE inference path") is mid-implementation for Gemma-4 26B A4B; the API surface around `/v1/walk-ffn` and `--ffn-only` is stable but the inference internals are churning. Track their commits before committing to option C.
6. **Not on crates.io** — pinning a `git` dep to a sha is the only option for now; revisit when larql publishes.
7. **What would actually use this?** Concrete user value for option B is debugging/alignment ("why did the model say X?"). For option C it is serving 31B+ models from cheap memory-rich nodes. Pick one before designing — they don't overlap.

---

## 8. References

- larql repo: https://github.com/chrishayuk/larql (commit visible at `git api repos/chrishayuk/larql` shows `pushed_at: 2026-04-24`; default branch `main`).
- larql server API: `crates/larql-server/README.md` and `crates/larql-server/src/routes/{describe,walk,walk_ffn,infer,stream,embed}.rs`.
- larql remote-FFN protocol: `crates/larql-server/src/routes/walk_ffn.rs` doc comments (binary wire format definition).
- ruvllm OpenAI server: `/home/ruvultra/projects/ruvector/crates/ruvllm-cli/src/commands/serve.rs` lines 119-167.
- ruvllm public API: `/home/ruvultra/projects/ruvector/crates/ruvllm/src/lib.rs` lines 164-177.
- ruvllm features: `/home/ruvultra/projects/ruvector/crates/ruvllm/Cargo.toml` lines 113-181.
- ruvllm integration story: `/home/ruvultra/projects/ruvector/docs/adr/ADR-002-ruvllm-integration.md`.

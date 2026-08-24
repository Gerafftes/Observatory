# ADR-165: Tiny RuvLLM Agents on Heterogeneous ESP32 SoCs

**Status:** Proposed
**Date:** 2026-04-30
**Authors:** RuVector / RuvLLM team
**Deciders:** ruv
**Technical Area:** Edge Inference / Embedded Federation / Vector Memory on MCUs
**Related ADRs:** ADR-002 (RuvLLM ↔ Ruvector Integration), ADR-074 (RuvLLM Neural Embeddings — HashEmbedder tier), ADR-084 (ruvllm-wasm Primitive Surface), ADR-090 (Ultra-Low-Bit QAT / Pi-Quantization for ESP32-P4 PSRAM), ADR-091 (INT8 CNN Quantization)
**Closes / supersedes:** Issue #409 framing (`examples/ruvLLM/esp32-flash` as "tiny LLM")

## 1. Context

`examples/ruvLLM/esp32-flash` was framed as a "Full-featured LLM v0.2" with INT8 transformer inference, MicroLoRA adaptation, and speculative decoding running on a single MCU SRAM (≤ 520 KB). Issue #409 reproduced the actual behavior: `main.rs` was a control-surface skeleton (PRNG-seeded weights, single-multiply pseudo-attention, no KV cache) that no longer compiled against `lib.rs` after the `optimizations/*` and `ruvector/*` modules were refactored.

The framing is the root issue. ADR-002 positions ruvLLM as a *serving runtime* and ruvector as the *unified memory layer*. ADR-084 enumerates the canonical ruvllm primitive surface (KV cache, MicroLoRA r1-4, HNSW Semantic Router, MicroRAG, Chat Templates, SONA Instant). ADR-074 ships HashEmbedder as Tier 1 (deterministic FNV-1a + char-bigram + L2-norm, no model). ADR-090 places "real" model inference at ESP32-P4 with 8 MB PSRAM, not at 520 KB SRAM. **None of those ADRs endorse a transformer skeleton in 4 KB of `HVec`.**

What an ESP32 (Xtensa or RISC-V, 320–520 KB SRAM, no FPU on most variants) *can* do honestly:

- HNSW kNN search over ≤256 INT8 vectors (`MicroHNSW`)
- RAG retrieval with embedded knowledge entries (`MicroRAG`)
- Semantic memory with type-tagged entries (`SemanticMemory`)
- Anomaly detection from embedding drift (`AnomalyDetector`)
- MicroLoRA rank 1-2 adaptation on cached activations (`MicroLoRA`)
- Sparse-attention masks and binary/PQ quantization helpers (`SparseAttention`, `BinaryVector`, `ProductQuantizer`)
- Multi-chip federation via SPI/UART/ESP-NOW (`PipelineNode`, `FederationMessage`, `SpeculativeDecoder`)
- HashEmbedder Tier 1 from ADR-074

The crate `lib.rs` already exports all of these. The example `main.rs` was the only thing not aligned.

## 2. Decision

Reframe `examples/ruvLLM/esp32-flash` as **a fleet of tiny ruvLLM/ruvector agents**, where each chip in a federation runs **one specialized role** (or a small composition) drawn from the lib's primitive surface, not a monolithic "LLM." Cross-chip coordination uses the existing `federation::*` types.

This is the smallest viable framing that:

1. Compiles against the current `lib.rs` without API drift.
2. Matches what the hardware can actually do.
3. Composes with ADR-090's PSRAM big-model path when it lands (a P4 chip can join the federation as the "drafter" role; smaller chips remain "verifiers" / "indexers").
4. Honors ADR-002's split: **ruvllm** primitives on each chip, **ruvector** memory shared across chips.

### 2.1 Tiny-Agent Role Catalog

A *tiny agent* is one Rust binary, one ESP32 SoC, one role + always-on health surface.

| Role | Min variant | Primitives used | Federation traffic |
|---|---|---|---|
| **HnswIndexer** | ESP32-C3 (400 KB) | `MicroHNSW<128, 256>`, `HashEmbedder` | inbound: `add(text)`; outbound: `kNN(query, k)` |
| **RagRetriever** | ESP32 (520 KB) | `MicroRAG`, `HashEmbedder` + `MicroHNSW` | inbound: `recall(query)`; outbound: top-k entries |
| **AnomalySentinel** | ESP32-S2 (320 KB) | `AnomalyDetector` | streams `AnomalyResult` events |
| **MemoryArchivist** | ESP32-C6 (512 KB) | `SemanticMemory` (type-tagged) | inbound: `remember(type, text)`; outbound: `recall_by_type` |
| **LoraAdapter** | ESP32-S3 (512 KB + SIMD) | `MicroLoRA<rank=1..2>`, `LoRAStack` | inbound: rank-1 deltas; outbound: adapted activations |
| **SpeculativeDrafter** | ESP32-S3 (512 KB) | `SpeculativeDecoder` w/ `DraftVerifyConfig::for_five_chips` | drafts → broadcast; consumes `VerifyResult` |
| **PipelineRelay** | any | `PipelineNode { Head/Middle/Tail }` | passes activations along the chain |

Every binary also exposes the **always-on** surface: a UART CLI for `role`, `stats`, `peers`, `help`. This is the shape that lets `espflash flash --monitor` give an honest "what does this chip do" answer in <1 s of serial output.

### 2.2 What ships per ESP32 variant (default role assignment)

| Variant | SRAM | FPU | SIMD | Default role |
|---|---|---|---|---|
| ESP32 | 520 KB | no | no | `RagRetriever` |
| ESP32-S2 | 320 KB | no | no | `AnomalySentinel` |
| ESP32-S3 | 512 KB | yes | yes | `SpeculativeDrafter` (or `LoraAdapter`) |
| ESP32-C3 | 400 KB | no | no | `HnswIndexer` |
| ESP32-C6 | 512 KB | no | no | `MemoryArchivist` |
| ESP32-P4 | 8 MB PSRAM | yes | yes | (deferred — ADR-090 path) |

Role is selected at boot from a build-time `ROLE` env var (defaulted per-variant by the CI matrix), with a UART override `set-role <name>` for development.

### 2.3 Embedding (ADR-074 Tier 1)

All roles share a common embedder so federation messages are interoperable: **HashEmbedder** — FNV-1a hash of UTF-8 bytes + character-bigram bag, L2-normalized to 64-byte INT8. Deterministic, ~5 µs on Xtensa, no float ops on non-FPU variants. Output dim matches the existing `EMBED_DIM = 64` already used across `lib.rs`. Phase 2 (RlmEmbedder, ADR-074) and Phase 3 (Candle, ADR-074) are explicitly deferred — they don't fit on these chips.

### 2.4 Federation bus (existing types)

`CommunicationBus` already enumerates `{Spi, I2c, Uart, EspNow, Parallel}`. v1 uses `Uart` (host bridge) + `EspNow` (chip-to-chip). `FederationMessage` and `MessageHeader` are reused unchanged. `MAX_FEDERATION_SIZE = 8` stays as the upper bound.

### 2.5 Build & release

One Cargo target → one role → one .bin per ESP32 variant. CI matrix produces:

```
ruvllm-esp32-esp32          (RagRetriever)
ruvllm-esp32-esp32s2        (AnomalySentinel)
ruvllm-esp32-esp32s3        (SpeculativeDrafter)
ruvllm-esp32-esp32c3        (HnswIndexer)
ruvllm-esp32-esp32c6        (MemoryArchivist)
ruvllm-esp32-host-test      (host-test binary, x86_64 / aarch64 — for CI smoke)
```

Asset names match the URL pattern hardcoded in `npm/web-flasher/index.html` (`${FIRMWARE_BASE_URL}/ruvllm-esp32-${target}`), closing issue #409 obs 2.

CI uses `espup install` → `cargo +esp build --release --target xtensa-esp32{,s2,s3}-espidf` (and `riscv32imc-esp-espidf` for c3, `riscv32imac-esp-espidf` for c6) → `espflash save-image --merge` → upload to GitHub release.

## 3. Architecture

```
┌──────────────────────────────────────────────────────────┐
│                   Tiny Agent Binary                       │
│                                                           │
│  ┌──────────────────────┐      ┌─────────────────────┐   │
│  │ Role Selector        │      │ HashEmbedder         │   │
│  │  (build-time +       │      │  (ADR-074 Tier 1,    │   │
│  │   UART override)     │      │   64-byte INT8)      │   │
│  └─────┬────────────────┘      └─────────┬───────────┘   │
│        │ enables exactly one of:         │               │
│  ┌─────▼────────┐ ┌──────────┐ ┌────────▼─────────┐     │
│  │ HnswIndexer  │ │ RagRetr  │ │ AnomalySentinel  │ ... │
│  │ MicroHNSW    │ │ MicroRAG │ │ AnomalyDetector  │     │
│  └─────┬────────┘ └────┬─────┘ └────────┬─────────┘     │
│        │ federation messages (FederationMessage)         │
│  ┌─────▼──────────────────────────────────────────────┐  │
│  │ CommunicationBus { Uart, EspNow }                  │  │
│  └─────┬──────────────────────────────────────────────┘  │
│        │                                                 │
│  ┌─────▼─────────────────────────┐                       │
│  │ UART CLI (always on)          │                       │
│  │  role | stats | peers | help  │                       │
│  └───────────────────────────────┘                       │
└──────────────────────────────────────────────────────────┘
```

## 4. Acceptance Gates

Each gate must pass before progressing to the next.

| Gate | Test | Evidence |
|---|---|---|
| **G1** | `cargo build --no-default-features --features host-test` succeeds | local + CI host-test job green |
| **G2** | All 7 roles instantiate in host-test without panic | smoke binary boots each role and prints `role: <name>` |
| **G3** | UART CLI loop accepts `add`/`search`/`recall`/`check`/`role`/`stats`/`help` in host-test | golden-output test fixture |
| **G4** | `cargo +esp build --release --target {xtensa-esp32s3-espidf}` succeeds in CI | a real `.bin` lands as a release asset |
| **G5** | Flash-and-monitor on attached `/dev/ttyACM0` produces banner + accepts `role` within 5 s | manual or `expect` script in CI hardware-loop (optional) |
| **G6** | All 5 target chips produce `.bin`s; web-flasher URL pattern resolves 200 for each | curl smoke against latest release |

G1–G3 are the prerequisite for a meaningful PR closing #409. G4–G6 are the firmware-release piece.

## 5. Out of Scope

- Real transformer inference at MCU SRAM scale. Deferred to ADR-090's PSRAM path (ESP32-P4, ESP32 + external PSRAM).
- WebGPU / WiFi-6 routing inside the agent. Federation can coexist with WiFi services; the agent itself stays bus-agnostic.
- RlmEmbedder Phase 2 / Candle Phase 3 from ADR-074 — they require corpora and model weights that don't fit.
- Reusing RuView's `esp32-csi-node.bin` artifacts — different application, see issue #409 reply.

## 6. Consequences

**Positive**

- Closes the #409 framing gap permanently: the README and binaries describe what they actually do.
- Eliminates the 27 API-drift errors by replacing the drifted code (no patching of misaligned signatures).
- Lets each chip be evaluated independently before federation, dropping the all-or-nothing build.
- Composes forward with ADR-090: when PSRAM "real model" lands, P4 joins the same federation with the existing `FederationMessage` schema.
- Honest baseline for benchmarks: per-role latency / SRAM / energy numbers on real hardware.

**Negative**

- "tinyLLM-on-one-chip" framing in old marketing material no longer matches the example. Acceptable; the issue reporter showed the prior framing was already wrong.
- Federated demos require >1 ESP32 to fully exercise. Single-chip flashing still produces a useful agent for its role.
- ADR-090 PSRAM path remains the only honest answer for "real model on ESP32"; this ADR doesn't accelerate it, only structures the federation it will join.

## 7. Implementation Roadmap

| Step | Owner | Dependency |
|---|---|---|
| 1 — Cfg-guard `build.rs` so `host-test` can build | this branch | none |
| 2 — Replace `main.rs` with role-selecting tiny-agent binary | this branch | step 1 |
| 3 — Add `HashEmbedder`-style embedder to `examples/ruvLLM/esp32-flash/src/embed.rs` | this branch | step 2 |
| 4 — README rewrite reflecting roles + ADR-165 | this branch | step 3 |
| 5 — `.github/workflows/ruvllm-esp32-firmware.yml` — espup + matrix build + release upload | follow-up PR | step 2 |
| 6 — Web-flasher URL alignment + npm CLI fallback fix | follow-up PR | step 5 |
| 7 — Hardware smoke script for `/dev/ttyACM0` | follow-up PR | step 5 |

Steps 1–4 are this PR's scope and unblock issue #409 obs 1+3. Steps 5–7 unblock issue #409 obs 2.

## 8. References

- Issue #409 — examples/ruvLLM/esp32-flash gap analysis (williavs)
- ADR-002 — RuvLLM ↔ Ruvector Integration
- ADR-074 — RuvLLM Neural Embeddings (HashEmbedder Tier 1 used here)
- ADR-084 — ruvllm-wasm v2.0.0 (canonical primitive surface)
- ADR-090 — Ultra-Low-Bit QAT / PSRAM big-model path
- `examples/ruvLLM/esp32-flash/src/{lib,federation/mod,ruvector/mod}.rs` — exported surface this ADR composes

---
id: ADR-170
title: ruvector-hailo-cluster tracing correlation (gRPC metadata + sortable IDs + caller propagation)
status: Accepted
date: 2026-05-02
author: ruv
branch: hailo-backend
tags: [ruvector, hailo, tracing, observability, request-id]
related: [ADR-167, ADR-168]
---

# ADR-170 — Tracing correlation

## Context

A query in a 16-Pi fleet produces:
- 1 web-handler log line (caller side)
- 1 coordinator dispatch log line
- 1 worker embed log line on the chosen Pi

Without correlation, joining these three takes timestamp alignment and
prayer. Standard observability tooling (loki, datadog, jaeger) keys off
**a single ID present in every log line**.

Three sub-problems:

1. **Where does the ID live on the wire?** — proto field vs gRPC metadata
2. **Who generates it?** — coordinator-internal random vs caller-supplied
3. **What format?** — random bytes vs sortable timestamp prefix

## Decision

### 1. gRPC metadata is canonical, proto field is back-compat

The `request_id` rides in an `x-request-id` gRPC metadata header (W3C-style).
The proto schema also carries a `request_id` string field, but workers
prefer the metadata header when both are present.

```rust
// proto.rs
pub const REQUEST_ID_HEADER: &str = "x-request-id";

pub fn inject_request_id<T>(req: &mut Request<T>, id: &str) { /* ... */ }

pub fn extract_request_id<T>(req: &Request<T>, fallback: &str) -> String {
    req.metadata().get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| fallback.to_string())
}
```

**Why metadata over proto field:** Tracing infrastructure inspects HTTP/gRPC
headers without parsing protobuf bodies. A sidecar or logging proxy can
correlate requests without knowing the embedding schema.

**Why keep the proto field:** Workers built before this convention still
need a way to log the ID. A coordinator built post-iter-49 → worker
built pre-iter-49 still works because the proto field is still populated.

### 2. Caller-supplied wins; otherwise auto-generate

Every public embed method has a `*_with_request_id(text, id)` sibling
that propagates a caller-supplied token end-to-end:

```rust
embed_one_blocking(text)                            -> Vec<f32>  // auto
embed_one_blocking_with_request_id(text, id)        -> Vec<f32>  // caller
embed_one(self, text).await                         -> Vec<f32>  // auto, async
embed_one_with_request_id(self, text, id).await     -> Vec<f32>  // caller, async

embed_batch_blocking(texts)                         -> Vec<Vec<f32>>
embed_batch_blocking_with_request_id(texts, id)     -> Vec<Vec<f32>>
embed_batch(self, texts).await                      -> Vec<Vec<f32>>
embed_batch_with_request_id(self, texts, id).await  -> Vec<Vec<f32>>
```

Empty `id` ≡ auto-generate (uniform fallthrough). A web handler can
pass `req.headers().get("x-request-id").unwrap_or("")` blindly.

### 3. Auto-generated IDs are sortable

`proto::random_request_id() -> String` produces 24 hex chars:

```
Layout: <16 hex of u64 epoch ms><8 hex of xorshift64*-derived u32>
Example: 0000019de68b5707983b8745
         |---- ms timestamp ----|<rand>
         u64 epoch ms = 1746146184583
         = 2026-05-02 02:36:24 UTC
```

**Why sortable:** `grep request_id | sort | uniq` in any log query
reveals call sequence without timestamp parsing. IDs starting with
`0000019de68b57` are all from the same hour — natural rough bucketing.

**Why 24 chars not 16:** The pre-iter-63 16-hex xorshift64* produced
random IDs with 64 bits of entropy but no temporal ordering. The new
24-char layout sacrifices 32 bits of randomness for sortability —
collisions only matter within a single millisecond, and 32 bits gives
~1/4-billion intra-ms collision probability, plenty for embed RPC rates.

**Why hex not base32/base58:** Hex has stable byte width and lex order
matches numeric order. Base32 (Crockford ULID-style) would shave 4
chars but require a runtime dep and complicate the test fixtures.

### CLI surface

`--request-id <id>` flag on both `embed` and `bench`:
- `embed`: `--request-id "ci-build-${BUILD_NUM}"` — every RPC carries the literal ID
- `bench`: `--request-id "${BUILD_NUM}"` — suffixed `<id>.t<tid>.c<counter>`
  per RPC, so all calls in a run share a grep-able prefix

Worker tracing logs surface the field automatically via `#[instrument]`:

```
INFO embed{text_len=5 request_id="ci-build-2741" latency_us=421}: worker embed
```

## Consequences

**+** End-to-end correlation across web → coordinator → worker via one
       grep, no timestamp join required
**+** Existing tracing infrastructure (jaeger, loki, datadog) "just works"
       since `x-request-id` is the W3C convention they all key off
**+** Caller-supplied IDs let upstream-already-traced systems thread
       their existing token through without rewriting
**+** Sortable IDs give natural time bucketing for free (`request_id =~ "^00000019de"` = same week)
**−** 24-char IDs are longer than uuid-v4 base16 (32 chars but recognised
       by tooling) or ULID (26 chars). Acceptable.
**−** PRNG is xorshift64*, not crypto-grade. Documented; embed correlation
       isn't a security boundary.
**−** Header injection is best-effort: invalid characters silently fall
       through to the proto field. Tested.

## Implementation

| Iter | Addition |
|---|---|
| 49 | `proto::REQUEST_ID_HEADER`, `inject_request_id`, `extract_request_id`; client + workers wired |
| 50 | `embed_one_blocking_with_request_id` + transport `embed_with_request_id` |
| 51 | `embed_batch_*_with_request_id`, async siblings |
| 52 | `--request-id` flag on `embed` |
| 53 | `info!()` emit on streaming embed in worker + fakeworker (parity with single embed) |
| 54 | `--request-id <token>` on bench (`.t<tid>.c<counter>` suffixing) |
| 63 | Sortable timestamp prefix in `random_request_id` |
| 65 | `random_request_id` promoted to `pub` in `proto` module |

Tests: 4 doc/unit tests on `random_request_id` shape + 4 integration
tests on caller-supplied propagation = **8 tracing-correlation tests**.

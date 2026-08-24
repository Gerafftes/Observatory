# Tutorial — Build a tiny LLM summariser on a Raspberry Pi with `ruvllm_sparse_attention`

A hands-on walkthrough that takes you from `cargo new` to a working
streaming text summariser running on a Raspberry Pi Zero 2W (512 MB RAM,
no GPU). You'll use SmolLM2-135M as the model and this crate as the
attention kernel.

By the end you will have:

1. Loaded a quantised GGUF model from disk into Rust memory.
2. Run a single-pass sparse attention prefill in **O(N log N)**.
3. Generated text with **O(log T) per-token** incremental decode.
4. Cut long-context cost to **O(N)** with the new FastGRNN gate.

Total time: ~30 minutes if you have a Pi handy. ~10 minutes for the
Rust steps if you only run on x86.

> All code in this tutorial is also published as a GitHub Gist:
> <https://gist.github.com/ruvnet/790214c832928d6f2ec7ebe593bb3def>

## 0. Prerequisites

- Rust 1.77 or newer (`rustup install stable`).
- About 200 MB of disk space (105 MB for the model + build artifacts).
- `git` and a working `cargo` toolchain.
- Optional: `ssh` access to a Pi 5 or Pi Zero 2W on your network.

## 1. Set up the project

```bash
cargo new --bin sparse-summariser
cd sparse-summariser
```

Edit `Cargo.toml`:

```toml
[package]
name    = "sparse-summariser"
version = "0.1.0"
edition = "2021"

[dependencies]
ruvllm_sparse_attention = { git = "https://github.com/ruvnet/RuVector", features = ["fp16"] }

[profile.release]
lto = "thin"
codegen-units = 1
```

Build to make sure the dep resolves:

```bash
cargo build --release
```

## 2. The kernel API at a glance

The crate exports four moving parts you'll use in this tutorial:

| Type / function                              | What it does                                              |
|----------------------------------------------|-----------------------------------------------------------|
| `Tensor3 { seq, heads, dim, data: Vec<f32> }`| The QKV tensor format. Flat `Vec<f32>`, no ndarray dep.   |
| `SparseAttentionConfig`                      | Window, block size, global tokens, causal/non-causal.     |
| `SubquadraticSparseAttention`                | The kernel — `forward`, `decode_step`, `forward_gated`.   |
| `FastGrnnGate`                               | The recurrent salience gate that turns log-N into linear. |

You **don't** get model loading, tokenization, sampling, or projection —
those are out of scope. You bring those (or copy them from `cognitum-agent`).

## 3. Sparse attention prefill in 20 lines

`src/main.rs`:

```rust
use ruvllm_sparse_attention::{
    AttentionBackend, SparseAttentionConfig, SubquadraticSparseAttention, Tensor3,
};

fn main() {
    // SmolLM2-135M shape: 9 Q heads, 3 KV heads, head_dim 64, but for the
    // tutorial we use a tiny config so it runs anywhere.
    let seq = 256;
    let heads = 4;
    let dim   = 32;

    let cfg = SparseAttentionConfig {
        window:        32,
        block_size:    16,
        global_tokens: vec![0, 1],
        causal:        true,
        use_log_stride: true,
        use_landmarks:  true,
        sort_candidates: false,
    };
    let attn = SubquadraticSparseAttention::new(cfg).unwrap();

    // In a real LLM, q/k/v come from your projection step (W_q · x, etc).
    let q = Tensor3::zeros(seq, heads, dim);
    let k = Tensor3::zeros(seq, heads, dim);
    let v = Tensor3::zeros(seq, heads, dim);

    let t0 = std::time::Instant::now();
    let _out = attn.forward(&q, &k, &v).unwrap();
    println!("forward: {:.2} ms", t0.elapsed().as_secs_f64() * 1000.0);
}
```

Run it:

```bash
cargo run --release
# forward: 0.81 ms
```

You just ran sparse attention. The cost was `O(seq · log seq)` instead of
`O(seq²)`. At `seq=256` that doesn't matter much, but it scales.

## 4. Streaming decode with KvCache

For real text generation you don't re-prefill on every token; you cache K/V
and decode one new token against the cache. Add this to `main.rs`:

```rust
use ruvllm_sparse_attention::KvCache;

let mut cache = KvCache::new(/*capacity*/ 4096, /*kv_heads*/ heads,
                             /*dim*/ dim, /*block_size*/ 16);

// Append the prefill K/V (in real code you fill these from W_k @ x, W_v @ x).
cache.append_all(&k, &v).unwrap();

// Now decode 32 new tokens.
let q_step = Tensor3::zeros(1, heads, dim);
let new_k  = Tensor3::zeros(1, heads, dim);
let new_v  = Tensor3::zeros(1, heads, dim);

for step in 0..32 {
    cache.try_append(&new_k, &new_v).unwrap();
    let _out = attn.decode_step(&q_step, &cache).unwrap();
    if step == 0 {
        println!("decode_step: O(log T) per token, T = {}", cache.len());
    }
}
```

Each `decode_step` is `O(log T)` where `T` is the number of cached tokens —
the cache grows but per-step latency stays roughly constant in practice.

## 5. The FP16 KV cache (feature `fp16`)

For a Pi Zero 2W with 512 MB total, halving the KV cache memory matters.
Switch to `KvCacheF16`:

```rust
use ruvllm_sparse_attention::KvCacheF16;

let mut cache = KvCacheF16::new(4096, heads, dim, 16);
cache.try_append(&new_k, &new_v).unwrap();   // f32 in, f16 stored
let _out = attn.decode_step_f16(&q_step, &cache).unwrap();
```

Memory at `seq = 8192`, 8 KV heads, `dim = 128`: **2.1 GB → 1.1 GB**.
Quality loss is bounded by f16 round-trip error (<1e-2 per element).

## 6. Near-linear with FastGRNN gating

For very long contexts the `O(N log N)` baseline still grows. The
`FastGrnnGate` gives you `O(N)` by dropping low-salience long-range
tokens.

```rust
use ruvllm_sparse_attention::FastGrnnGate;

let gate = FastGrnnGate::new(/*input_dim = dim*/ dim, /*hidden_dim*/ 16);

let q = Tensor3::zeros(2048, heads, dim);
let k = Tensor3::zeros(2048, heads, dim);
let v = Tensor3::zeros(2048, heads, dim);

// Gate keeps the top-8 salient long-range tokens per position.
// Local window + globals + current position are always retained.
let _out = attn.forward_gated_with_fastgrnn(&q, &k, &v, &gate, /*top_k*/ 8).unwrap();
```

What just happened:

1. `gate.score_kv(&k)` ran a 1-pass `O(N · D_h²)` recurrent forward over
   the K rows and produced a per-token salience score.
2. `keep_mask_top_k(salience, 8)` kept the 8 highest-salience positions.
3. `forward_gated` ran sparse attention with the long-range candidate set
   pruned to that mask. Window + globals + current position were always
   retained (causality preserved).

Per-position cost is now `window + globals + 8 + landmark_blocks` —
constant in `seq`. Total cost is `O(seq)`.

### Tuning the gate

| Knob          | Trade-off                                                                 |
|---------------|---------------------------------------------------------------------------|
| `top_k`       | Smaller = faster, more aggressive pruning. Try 4–32.                      |
| `hidden_dim`  | Larger = smarter gate, slightly more cost. 16–32 is a good range.         |
| `input_dim`   | Must equal your `head_dim` if you use `score_kv`.                         |

For the scaling demo run:

```bash
cargo run -p ruvllm_sparse_attention --example fastgrnn_gated_scaling --release
```

You'll see the per-token cost of `forward_gated_with_fastgrnn` flatten as
`seq` grows, while `forward` keeps growing logarithmically.

## 7. Loading a real GGUF model

`ruvllm_sparse_attention` is format-agnostic — it operates on `f32`
Q/K/V tensors. To use a real GGUF model you need a dequantizer.
The reference implementation lives in
[`cognitum-one/seed`](https://github.com/cognitum-one/seed) at
`src/cognitum-agent/src/sparse_llm_weights.rs`. It supports:

- F32 (type 0)
- Q4_0 (type 2) and **Q4_0 stochastic** (dithered, unbiased reconstruction)
- Q5_0 (type 6)
- Q8_0 (type 8)
- Q4_K (type 12) and **Q4_K stochastic**
- Q6_K (type 14)

Sketch of what the integration looks like:

```rust
// 1. Read the GGUF header + tensor table.
// 2. For each layer, dequant Q/K/V projection weights to f32.
// 3. Project x → q,k,v with matvec.
// 4. Reshape into Tensor3 [seq, heads, head_dim].
// 5. attn.forward(&q, &k, &v) — what this tutorial showed.
// 6. Output projection + RMSNorm + lm_head + sample.
```

Concrete example in production: `cognitum-one/seed#133` runs SmolLM2-135M
end-to-end on a Pi Zero 2W and is a working blueprint.

## 8. Running on a Raspberry Pi

Cross-compile from x86 (assuming you have `aarch64-linux-gnu-gcc`):

```bash
rustup target add aarch64-unknown-linux-gnu

RUSTFLAGS="-C target-cpu=cortex-a53 -C target-feature=+neon,+vfpv4" \
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
cargo build --release --target aarch64-unknown-linux-gnu
```

Copy the binary to the Pi:

```bash
scp target/aarch64-unknown-linux-gnu/release/sparse-summariser pi@pi-zero.local:/tmp/
ssh pi@pi-zero.local /tmp/sparse-summariser
```

Expect ~5–10× the timings vs x86.

## 9. Where to go next

- Read the **API reference** with `cargo doc -p ruvllm_sparse_attention --open`.
- See the **architecture decision records** in `docs/adr/ADR-183` through
  `ADR-191` for the design rationale of every primitive.
- Watch the **production cognitum-agent integration** at
  <https://github.com/cognitum-one/seed/pull/133> for an end-to-end example.

## License

MIT.

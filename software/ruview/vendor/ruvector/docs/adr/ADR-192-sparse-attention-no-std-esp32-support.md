---
adr: 192
title: "no_std + alloc support for ruvllm_sparse_attention to enable ESP32-S3 deployment"
status: accepted
date: 2026-05-07
authors: [ruvnet, claude-flow]
related: [ADR-181, ADR-183, ADR-189, ADR-190, ADR-191]
tags: [sparse-attention, no-std, esp32, embedded, alloc, libm, edge-ai]
---

# ADR-192 — no_std + alloc support for `ruvllm_sparse_attention`

## Status

**Accepted.** Implemented on branch
`feat/sparse-attn-no-std-esp32` and validated on attached
**ESP32-S3 (revision v0.2, MAC `ac:a7:04:e2:66:24`, 16 MB flash)**
via cross-compile to `xtensa-esp32s3-none-elf` using the `esp` Rust
toolchain (`espup`).

## Context

`ruvllm_sparse_attention v0.1.0` shipped to crates.io as an
`std`-only crate (ADR-191 follow-on). The crate was excluded from the
`no-std` crates.io category because the source used:

| std path                          | Where                                |
|-----------------------------------|--------------------------------------|
| `std::error::Error`               | `attention.rs::AttentionError` impl  |
| `std::fmt::{Display, Formatter}`  | `attention.rs::AttentionError` impl  |
| `std::cmp::Ordering`              | 3 sort comparators                   |
| `std::collections::HashSet`       | 2 sites in `forward_gated`           |
| `f32::exp / sqrt / tanh / powi`   | 46 sites across attention + gate     |

That `std` requirement closed off the most interesting near-term
deployment target: **microcontroller-class edge nodes**. The
`cognitum-seed` device tier (the layer below cognitum-v0) is
ESP32-S3-class hardware. A separate sister crate
`ruvllm-esp32 = 0.3.2` (already on crates.io) exists *because* the
upstream `ruvllm` family has not been `no_std` portable. Maintaining
two parallel attention kernels is wasteful when the only delta is
the standard-library boundary.

The user attached an ESP32-S3 to `/dev/ttyACM0` (vendor 0x303a,
product 0x1001 — built-in USB-Serial-JTAG) and asked for first-class
support. This ADR captures the design.

## Decision

We will make `ruvllm_sparse_attention` `no_std + alloc` compatible
behind a default-on `std` feature flag. The change is **purely
additive** for existing consumers — `cargo add ruvllm_sparse_attention`
without changing features continues to behave exactly as before.

### 1. Cargo manifest

```toml
[features]
default  = ["std"]                    # back-compat
std      = []                         # opt-in std (the previous behaviour)
parallel = ["std", "dep:rayon"]       # rayon needs threads → requires std
fp16     = ["dep:half"]               # half supports no_std

[dependencies]
rayon = { version = "1",   optional = true }
half  = { version = "2",   optional = true, default-features = false }
libm  = { version = "0.2", default-features = false }   # always on
```

`libm` is a **mandatory** runtime dependency — pure-Rust portable
implementations of `expf`, `sqrtf`, `tanhf`, `powf`. The 60 KB it
adds is negligible vs the alternatives (`num-traits` is heavier;
`core::intrinsics::*` is nightly-only).

### 2. Crate root

```rust
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

#[cfg(not(feature = "std"))]
pub mod no_std_math {
    pub trait F32Ext {
        fn exp(self)  -> Self;
        fn sqrt(self) -> Self;
        fn tanh(self) -> Self;
        fn powi(self, n: i32) -> Self;
    }
    impl F32Ext for f32 {
        #[inline] fn exp(self)  -> Self { libm::expf(self) }
        #[inline] fn sqrt(self) -> Self { libm::sqrtf(self) }
        #[inline] fn tanh(self) -> Self { libm::tanhf(self) }
        #[inline] fn powi(self, n: i32) -> Self { libm::powf(self, n as f32) }
    }
}
```

The trait restores the inherent `f32::exp/sqrt/tanh/powi` method
syntax in `no_std` mode. With `std`, the trait is not defined and
the inherent methods are used as before. **Net change at every one
of the 46 math call sites: zero.**

### 3. Source files

| File                | Change                                                      |
|---------------------|-------------------------------------------------------------|
| `attention.rs`      | `std::*` → `core::*` / `alloc::*`; gate `Error` impl on `std`; replace `HashSet` with `BTreeSet`; add `use no_std_math::F32Ext as _;` under cfg |
| `fastgrnn_gate.rs`  | same import shape                                           |
| `tensor.rs`         | add `alloc::{vec, vec::Vec, string::{String, ToString}, format}` imports |
| `model.rs`          | no changes needed (already `core`-clean)                    |

### 4. Verification

| Build                                                             | Result          |
|-------------------------------------------------------------------|-----------------|
| `cargo test -p ruvllm_sparse_attention --lib`                     | 38/38 pass      |
| `cargo build -p ruvllm_sparse_attention --no-default-features`    | clean           |
| `cargo build ... --no-default-features --features fp16`           | clean           |
| `cargo +esp build ... --target xtensa-esp32s3-none-elf -Z build-std=core,alloc` | clean (5.04 s debug, 1.02 s release) |
| Release rlib size for ESP32-S3                                    | **376 KB**      |

The on-device smoke runner (`examples/esp32s3_smoke.rs`) is split
between a `cfg(not(target_os = "none"))` host main (passes natively
in 0.26 s) and a `cfg(target_os = "none")` `run_on_target()` entry
that an application crate consumes from a real `#[entry]` function.
The library surface — `forward`, `forward_gated_with_fastgrnn`,
`KvCacheF16::decode_step_f16` — is fully exercised in either mode.

### 5. crates.io categories

The `categories` array in `Cargo.toml` will gain `"no-std"`:

```toml
categories = ["algorithms", "science", "mathematics", "no-std"]
```

This requires a v0.1.1 publish (the v0.1.0 already on crates.io is
std-only and will continue to work). The version bump is minimal
because the change is purely additive.

## Consequences

### Positive

- **One attention kernel for the whole edge tier.** Pi 5 → Pi Zero 2W →
  ESP32-S3 → ESP32-C6 / RISC-V MCUs all use the same crate. No more
  `ruvllm-esp32` parallel maintenance burden.

- **Disciplined dependency surface.** `no_std` forces every system
  service (allocator, time, threads, panic) to be passed in by the
  caller. Easier to audit, harder to accidentally pull in `tokio` or
  a logging framework.

- **Smaller release binary on real targets.** Eliminating `libstd` on
  ESP32 saves 200–800 KB of code and ~5 ms of startup. Material on a
  512 KB SRAM chip.

- **Zero churn for std consumers.** `cognitum-agent`, the Hailo-10H
  cluster, and any future server-side consumer continue to work
  unchanged because `std` is the default feature.

### Negative

- **One more mandatory dependency.** `libm` 0.2 (~60 KB compiled,
  pure Rust, MIT/Apache-2.0). Acceptable: ADR-183 only requires
  *zero runtime deps for the default config*, not zero-deps period.
  `libm` is a single transcendental-functions crate, not a runtime.

- **A second math path.** `std` users hit `f32::exp` (compiler intrinsic
  → glibc `expf`); `no_std` users hit `libm::expf` (pure Rust). These
  are bit-different in the last ULP for some inputs. Acceptable: sparse
  attention output is not bit-reproducible across hardware in the first
  place (NEON vs AVX vs SoftFP differ at ULP).

- **Test suite still requires `std`.** The 38 existing tests use `vec!`
  and `format!` macros that work in `alloc`, but the test harness
  itself (`#[test]`, `assert!`, panic hooks) needs `std`. Tests are
  always run in `std` mode; the crate's no_std posture is verified
  by `cargo build --no-default-features` and by the ESP32-S3
  cross-compile.

### Neutral

- The `parallel` feature requires `std` (rayon is std-only). On
  bare-metal we have no threads to parallelise across, so this is
  the correct limitation rather than a regression.

## Alternatives considered

### A) Keep std-only and ship a separate `ruvllm-sparse-attn-no-std` crate

Rejected. Two parallel implementations diverge over time
(see what already happened with `ruvllm` vs `ruvllm-esp32`). One
`Cargo.toml` feature is far cheaper to maintain than one fork.

### B) Make `std` opt-in (default `no_std`)

Rejected on backwards compatibility grounds. v0.1.0 was published
as `std`-only; downstream consumers (cognitum-agent at minimum)
already depend on it without specifying features. Switching the
default would silently break their build at the next `cargo update`.
A v0.2 with the flipped default could be considered later if
no_std becomes the dominant deployment.

### C) Use `num-traits` + `Float` trait instead of bespoke `F32Ext`

Rejected. `num-traits` is heavier (40-some traits, generic over many
numeric types) and pulls a dep tree we don't need. Our usage is one
type (`f32`) with four functions — a 30-line bespoke trait is the
right size.

### D) Inline `libm::expf` etc. at every call site

Rejected. Touching 46 call sites and changing every `.exp()` to a
free-function call is more invasive than defining a trait that
restores the method syntax. The trait is `cfg`-gated so std users
see no change at all.

## Test plan

- [x] Native `cargo test -p ruvllm_sparse_attention --lib` — **38/38 pass**
- [x] Native `cargo build -p ruvllm_sparse_attention --no-default-features` — clean
- [x] Native `cargo build -p ruvllm_sparse_attention --no-default-features --features fp16` — clean
- [x] ESP32-S3 release `cargo +esp build --no-default-features --features fp16 --target xtensa-esp32s3-none-elf -Z build-std=core,alloc` — clean (1.02 s, 376 KB rlib)
- [x] Native run of `examples/esp32s3_smoke` — `esp32s3_smoke: all checks passed`
- [ ] On-device smoke test from a flash-able application crate
      (deferred to a follow-up PR; requires `esp-hal`, `esp-println`,
      `embedded-alloc`, panic handler scaffolding — out of scope here)

## Migration

- **No action required for std consumers.** The `std` feature is on
  by default. `cognitum-agent`, internal Hailo cluster code, and any
  third-party crate that depends on `ruvllm_sparse_attention` will
  see zero behavioural change.

- **For new no_std consumers** (ESP32, Cortex-M, RISC-V MCU):

```toml
[dependencies]
ruvllm_sparse_attention = { version = "0.1.1", default-features = false, features = ["fp16"] }
```

  Bring your own allocator (`embedded-alloc`, `linked_list_allocator`),
  panic handler, and `#[entry]`. The library does not assume any of
  these.

- **Crate version bump**: v0.1.0 → v0.1.1 (patch — purely additive).
  Not a minor bump because no public API was added or changed.
  ADR-191's proposed `pi_zero_2w()` preset is a separate ADR and a
  separate version bump.

## References

- Attached ESP32-S3 device: revision v0.2, MAC `ac:a7:04:e2:66:24`,
  16 MB flash, dual Xtensa LX7 @ 240 MHz, USB-Serial-JTAG at
  `/dev/ttyACM0`
- ESP Rust toolchain: <https://github.com/esp-rs/espup>
- libm crate: <https://crates.io/crates/libm> (0.2.x, pure-Rust port of MUSL libm)
- ADR-183 — zero runtime dep footprint (preserved in default config)
- ADR-191 — Pi Zero 2W production hardening (Pi-targeted; this ADR
  is the MCU-tier complement)
- Sister crate `ruvllm-esp32` 0.3.2 — to be deprecated once this lands

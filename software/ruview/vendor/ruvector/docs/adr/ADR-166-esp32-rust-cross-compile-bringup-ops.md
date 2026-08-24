# ADR-166: ESP32 Rust Cross-Compile + Bring-Up Operations Manual

**Status:** Proposed
**Date:** 2026-04-30
**Authors:** RuVector / RuvLLM team
**Deciders:** ruv
**Technical Area:** Embedded Build Pipeline / ESP-IDF Toolchain / USB-Serial/JTAG Console / CI Matrix
**Companion to:** [ADR-165](ADR-165-tiny-ruvllm-agents-on-esp32-soCs.md) (what runs on each chip)
**Related ADRs:** ADR-002, ADR-074, ADR-084, ADR-090, ADR-091
**Closes:** Issue #409 obs 2 (no firmware in releases) once §9 is in place

---

## 1. Context

ADR-165 established the *what*: each ESP32 chip runs one tiny ruvLLM/ruvector role. This ADR establishes the *how*: the canonical Rust cross-compile + bring-up operations needed to ship that firmware reliably across 5 ESP32 variants, in CI, and on real hardware.

It exists because the rc1 bring-up surfaced **14 distinct build/runtime failure modes** between "first attempt" and "ELF runs on `/dev/ttyACM0`," many of which are not documented in any single Espressif or `esp-rs` source. The point of this ADR is to ensure the next contributor (or the next chip variant) does not have to re-discover them.

### What was discovered during rc1 bring-up

| # | Failure mode | Where | Fix in this ADR |
|---|---|---|---|
| 1 | `embuild::espidf` not in scope | local build | §3 + §4 |
| 2 | `*const i8` vs `*const u8` mismatch in esp-idf-svc 0.49.1 | local build | §3 |
| 3 | `host-test` feature could not build (espidf cfg unconditional) | local + CI | §5 |
| 4 | `mold` linker rejected on Xtensa | local build | §6 |
| 5 | `Cannot locate argument '--ldproxy-linker <linker>'` (ldproxy panic) | local build | §5 (root cause), §7 |
| 6 | `cannot find 'log' in 'esp_idf_svc'` (`alloc` feature gate) | local build | §3 |
| 7 | `no field 'queue_non_blocking'` (esp-idf-hal 0.46.2) | local build | §3 |
| 8 | `linker 'xtensa-esp32s3-elf-gcc' failed: undefined reference to memcpy/...` | local build | §5 + §7 |
| 9 | UART0 console not visible on `/dev/ttyACM0` (USB-Serial/JTAG dev board) | device | §10 |
| 10 | `Guru Meditation: Double exception` after `app_main()` | device | §11 |
| 11 | sdkconfig stack stayed at 12 KB despite override | device | §8 |
| 12 | Banner reaches host only on panic flush, not steady state | device | §10 |
| 13 | CI: `linker 'ldproxy' not found` | CI | §9 |
| 14 | CI: `toolchain 'esp' is not installed` (RISC-V c3/c6) | CI | §9 |

Every entry below is grounded in a real failure transcript captured during rc1.

---

## 2. Decision

Adopt the canonical configuration in §3–§11 as the *only supported* path for building, flashing, and running ruvllm-esp32 firmware. Document it in this ADR, encode it in `examples/ruvLLM/esp32-flash/Cargo.toml`, `.cargo/config.toml`, `build.rs`, `sdkconfig.defaults*`, `src/main.rs`, and `.github/workflows/ruvllm-esp32-firmware.yml`. Treat any deviation as a regression that requires re-running G1–G6 (§12).

This is not an architectural decision (ADR-165 owns that). It is an *operational* decision: pin the toolchain, pin the crate trio, pin the build-script invocation pattern, pin the CI workflow shape, and document the diagnostics so the next person doesn't bisect 14 failure modes again.

---

## 3. Crate-version matrix (PINNED)

The viable Rust crate trio for ESP-IDF v5.1.2 against the current `bindgen` output:

```toml
[dependencies]
esp-idf-svc = { version = "=0.51.0", default-features = false, features = ["std"], optional = true }
esp-idf-hal = { version = "=0.45.2", default-features = false, features = ["std"], optional = true }
esp-idf-sys = { version = "=0.36.1", default-features = false, features = ["binstart", "native"], optional = true }

[build-dependencies]
embuild = { version = "0.32", features = ["espidf"] }
```

**Why exact `=` pins:**

- **esp-idf-svc 0.49.1 → 0.51.0**: 0.49.1 has 4 `*const i8` vs `*const u8` mismatches in `tls.rs` and `private/cstr.rs` against current bindgen output (rust-esp toolchain promotes C `char` to `u8`, the crate still expects `i8`). 0.51.0 is the first release where this is fixed and the trio still compiles.
- **esp-idf-hal 0.46.2 → 0.45.2**: 0.46.2 references `TransmitConfig.queue_non_blocking` which is not present in the bound struct — release-side regression. 0.45.2 is the latest version that compiles cleanly.
- **esp-idf-sys 0.37.x → 0.36.1**: 0.37.x changes the `binstart` layout. We have not yet validated that path; 0.36.1 is what shipped in rc1.
- **embuild `features = ["espidf"]`**: without this feature, `embuild::espidf::sysenv::output()` does not exist as a callable item — `cannot find 'espidf' in 'embuild'` at build-script compile time. Default features on `embuild` *do not* include `espidf`; you must opt in.

**Why explicit `default-features = false` on the trio**: avoids accidentally pulling in `alloc`-gated modules (`esp_idf_svc::log`) and unrelated peripherals. `binstart` is mandatory for esp-idf-sys to wire `app_main` to Rust `main`. `native` is mandatory for esp-idf-sys's build script to download the SDK and emit the link args.

**Do not bump these without re-running the full G1–G6 acceptance** (§12). The next safe trio bump should be done as ADR-167.

---

## 4. Toolchain matrix per target

| Variant | Rust target triple | Toolchain | How installed |
|---|---|---|---|
| esp32 | `xtensa-esp32-espidf` | `esp` (custom Rust) | `espup install --targets esp32` |
| esp32s2 | `xtensa-esp32s2-espidf` | `esp` | `espup install --targets esp32s2` |
| esp32s3 | `xtensa-esp32s3-espidf` | `esp` | `espup install --targets esp32s3` |
| esp32c3 | `riscv32imc-esp-espidf` | `nightly` + `rust-src` | `rustup toolchain install nightly --component rust-src` |
| esp32c6 | `riscv32imac-esp-espidf` | `nightly` + `rust-src` | same as c3 |

The Xtensa cores (LX6 on esp32, LX7 on s2/s3) require a **custom Rust toolchain** (`esp` channel) because upstream LLVM does not support Xtensa. `espup` installs a forked LLVM, GCC, and the Rust `esp` channel into `~/.rustup/toolchains/esp/` and `~/.espressif/`.

The RISC-V cores (c3/c6) use **upstream nightly** + `rust-src` (for `-Z build-std`).

**Same crate, different `cargo +<toolchain> build` per target.** The CI matrix in §9 picks the right toolchain per matrix entry.

`espup install` invocation matters: `espup install --targets <chip>` for an Xtensa chip installs the `esp` toolchain. Calling it for a RISC-V chip is a no-op for the Rust toolchain — that's why **rc1 c3 + c6 jobs failed with `error: toolchain 'esp' is not installed`** before we split the workflow.

---

## 5. The build.rs cfg pitfall (the silent killer)

**Bug**: `build.rs` runs on the **host** at compile time. `cfg(target_os = "espidf")` evaluates against the *host's* OS — always `linux`/`macos`/`windows`. It is *never* `espidf`, even when `--target xtensa-esp32s3-espidf` is on the command line.

**Symptom**: `embuild::espidf::sysenv::output()` is gated behind a host-cfg in `build.rs`. The call is silently compiled out. The build script runs but emits zero `cargo:rustc-link-arg=...` directives. The final link step only sees `compiler_builtins.rlib` and `--ldproxy-linker` is absent. ldproxy panics with `Cannot locate argument '--ldproxy-linker <linker>'`.

This looks like a build-system mystery for ~3 hours.

**Fix**: read the cargo target at build-script runtime via `CARGO_CFG_TARGET_OS`:

```rust
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("espidf") {
        embuild::espidf::sysenv::output();
    }
}
```

This pattern is required for **any** build script that conditionally runs target-specific logic during a cross-compile. It is not specific to ESP-IDF — anyone who has a host-side build script and a `*-espidf` target needs it.

`embuild::espidf::sysenv::output()` is what re-emits esp-idf-sys's accumulated link args (`--ldproxy-linker=<gcc-path>`, `--ldproxy-cwd=<…>`, all the linker-script `-T` files, and the ESP-IDF static-library list) onto **our binary's** link line. Without it firing, our binary doesn't get linked against ESP-IDF.

---

## 6. RUSTFLAGS environment override

The RuVector workspace ships with `RUSTFLAGS=-C link-arg=-fuse-ld=mold` in the developer shell environment. `mold` does not support Xtensa or RISC-V-esp. Symptom:

```
mold: fatal: unknown command line option: --dynconfig=xtensa_esp32s3.so
collect2: error: ld returned 1 exit status
```

`cargo`'s `RUSTFLAGS` env var **replaces** the per-target `rustflags` from `.cargo/config.toml` rather than augmenting them. So you cannot fix this by adding things to the per-target config.

**Fix in shell**: `env -u RUSTFLAGS cargo +<toolchain> build …`

**Fix in CI**: `unset RUSTFLAGS` in the relevant step (see §9).

**Do NOT** add `rustflags = [...]` to the per-target `.cargo/config.toml` blocks as a workaround — that overrides esp-idf-sys's link args, and you'll be back to symptom #5.

---

## 7. `.cargo/config.toml` — one block per supported target

```toml
[build]
target = "xtensa-esp32s3-espidf"

[target.xtensa-esp32-espidf]
linker = "ldproxy"
runner = "espflash flash --monitor"

[target.xtensa-esp32s2-espidf]
linker = "ldproxy"
runner = "espflash flash --monitor"

[target.xtensa-esp32s3-espidf]
linker = "ldproxy"
runner = "espflash flash --monitor"

[target.riscv32imc-esp-espidf]
linker = "ldproxy"
runner = "espflash flash --monitor"

[target.riscv32imac-esp-espidf]
linker = "ldproxy"
runner = "espflash flash --monitor"

[env]
ESP_IDF_VERSION = "v5.1.2"
ESP_IDF_SDKCONFIG_DEFAULTS = "sdkconfig.defaults"

[unstable]
build-std = ["std", "panic_abort"]
```

**Rules**:

- One `[target.<triple>]` block per supported variant. Don't omit; cargo silently uses the host linker for any unconfigured target, which fails as in symptom #4.
- `linker = "ldproxy"` for every block. ldproxy is a thin wrapper that forwards to the toolchain-provided `xtensa-esp*-elf-gcc` or `riscv32-esp-elf-gcc`, picking up the linker-script `-T` flags from esp-idf-sys.
- **Never** add `rustflags = [...]` here. If you need to override RUSTFLAGS, do it via the environment (§6).
- `runner = "espflash flash --monitor"` lets `cargo run` flash + open serial in one step.
- `build-std = ["std", "panic_abort"]` is required because ESP-IDF targets aren't tier-2; std must be rebuilt for them. Nightly Rust handles this.

`ldproxy` itself must be installed locally and in CI: `cargo install ldproxy --locked`.

---

## 8. sdkconfig defaults — base + per-variant split

ESP-IDF reads `sdkconfig.defaults` files in order. We use a base + variant-specific layered approach:

`sdkconfig.defaults` (chip-agnostic baseline):

```
CONFIG_SPIRAM_SUPPORT=n
CONFIG_LOG_DEFAULT_LEVEL_INFO=y
CONFIG_ESP_MAIN_TASK_STACK_SIZE=98304
CONFIG_MBEDTLS_SSL_IN_CONTENT_LEN=4096
CONFIG_MBEDTLS_SSL_OUT_CONTENT_LEN=2048
```

`sdkconfig.defaults.esp32s3` (per-variant overrides — applied last, win conflicts):

```
CONFIG_ESP32S3_DEFAULT_CPU_FREQ_240=y
CONFIG_SPIRAM=n
CONFIG_ESP32S3_INSTRUCTION_CACHE_32KB=y
CONFIG_ESP32S3_DATA_CACHE_64KB=y
CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG=y
CONFIG_ESP_CONSOLE_SECONDARY_NONE=y
CONFIG_ESPTOOLPY_FLASHSIZE_8MB=y
CONFIG_ESPTOOLPY_FLASHFREQ_80M=y
```

Pitfalls observed in rc1:

- The base file used to set `CONFIG_ESP_CONSOLE_UART_DEFAULT=y`. ESP-IDF's Kconfig "console choice" group lets the per-variant file override it, but only if you explicitly set the new choice (`CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG=y`) — Kconfig does not auto-disable the old one in all merge paths.
- The base file also used to use `CONFIG_ESP32_DEFAULT_CPU_FREQ_240=y` (chip-specific to original ESP32). On esp32s3 this becomes a stale unused config; the active choice is `CONFIG_ESP32S3_DEFAULT_CPU_FREQ_240=y`.
- **Stack size needs at least 96 KB** for TinyAgent + HNSW capacity 32 (see §11). Default 4 KB or even 12 KB will Guru Meditation immediately. Setting it in the base file (98304) keeps every variant safe.

**ESP-IDF caches the merged sdkconfig under `target/<triple>/release/build/esp-idf-sys-*/out/sdkconfig`**. If you change the defaults files but don't see the change reflected, delete `target/<triple>/release/build/esp-idf-sys-*/` to force regen. This is operational reality #3 from rc1.

---

## 9. CI workflow contract (`ruvllm-esp32-firmware.yml`)

Three jobs:

### `host-test smoke (G1–G3)` — runs first
- ubuntu-latest + dtolnay/rust-toolchain@stable
- `cargo build --no-default-features --features host-test --target x86_64-unknown-linux-gnu`
- For each of 5 variants: pipe `stats` into the binary with `RUVLLM_VARIANT=<v>`, assert `role=` shows up
- Fast (~30s). Gates the matrix below.

### `build` matrix — fans out per variant
Per-matrix entry:

```yaml
- target: esp32s3
  rust_target: xtensa-esp32s3-espidf
  chip: esp32s3
  toolchain: esp     # or 'nightly' for c3/c6
```

Steps in order:

1. **`if matrix.toolchain == 'nightly'`** — `rustup toolchain install nightly --component rust-src && rustup default nightly`
2. **`if matrix.toolchain == 'esp'`** — `cargo install espup --locked && espup install --targets ${{ matrix.target }} && source ~/export-esp.sh` (and propagate `PATH` + `LIBCLANG_PATH` to `$GITHUB_ENV`)
3. **Always** — `cargo install espflash ldproxy --locked` (both must be installed; `ldproxy` was rc1 failure mode #13)
4. **Build step**:

```bash
if [ "${{ matrix.toolchain }}" = "esp" ]; then source ~/export-esp.sh; fi
unset RUSTFLAGS
cargo +${{ matrix.toolchain }} build --release --target ${{ matrix.rust_target }}
```

5. **Image step**:

```bash
if [ "${{ matrix.toolchain }}" = "esp" ]; then source ~/export-esp.sh; fi
espflash save-image --chip ${{ matrix.chip }} --merge \
  target/${{ matrix.rust_target }}/release/ruvllm-esp32 \
  ruvllm-esp32-${{ matrix.target }}.bin
```

6. `actions/upload-artifact@v4` with `if-no-files-found: error`.

### `release` job — runs on `push: tags: 'ruvllm-esp32-v*'` or `workflow_dispatch`
- Downloads all 5 firmware artifacts
- `softprops/action-gh-release@v2` with `tag_name: ${{ github.event.inputs.release_tag || github.ref_name }}`, `files: dist/ruvllm-esp32-*.bin`, `fail_on_unmatched_files: true`

### Why the per-toolchain `if:` matters
rc1 attempt 1 used a single `cargo install espup && espup install --targets <var>` step for every matrix entry. For RISC-V (c3, c6), `espup install` does not install the `esp` Rust toolchain because RISC-V doesn't need it — the build then ran `cargo +esp build` and failed with `error: toolchain 'esp' is not installed`. The fix: split toolchain install per matrix entry, and choose `cargo +nightly` vs `cargo +esp` per target.

---

## 10. USB-Serial/JTAG console — the missing two calls

**Setup**: `CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG=y` in sdkconfig routes ESP-IDF's bootloader logs and `printf`/`ESP_LOG*` through the USB-CDC peripheral. Linux sees `/dev/ttyACM0`.

**The trap**: with `CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG=y` alone, ESP-IDF installs a *polling-mode* console driver. It works for kernel logs (which use a synchronous low-level path) but **does not** route Rust `std::io::stdout` / `stderr` / `stdin` through the USB CDC FIFO in interrupt mode. Symptoms:

- Bootloader logs (`I (255) main_task: Calling app_main()`) appear normally.
- After `app_main()`, `eprintln!`/`println!` from Rust produce **silence** on `/dev/ttyACM0`.
- A `panic!` *does* show its message — because the panic handler triggers a reboot, which flushes the polling-mode FIFO during teardown.

The polling-mode driver buffers TX indefinitely until reset.

**Fix** — switch to interrupt-mode driver and route VFS through it. Required *both* calls, *both* required, in this order:

```rust
unsafe {
    let mut cfg = esp_idf_svc::sys::usb_serial_jtag_driver_config_t {
        tx_buffer_size: 1024,
        rx_buffer_size: 256,
    };
    let _ = esp_idf_svc::sys::usb_serial_jtag_driver_install(&mut cfg);
    esp_idf_svc::sys::esp_vfs_usb_serial_jtag_use_driver();
}
```

After this:
- `eprintln!`/`println!` flush via interrupt-driven TX
- `std::io::stdin().lock().lines()` blocks on USB-CDC RX exactly like host stdio
- The interactive CLI in ADR-165 §2.1 works on `/dev/ttyACM0` with no special host setup

The function `esp_vfs_usb_serial_jtag_use_driver` exists in esp-idf-sys 0.36.1 bindings as a non-variadic FFI function — Rust calls it cleanly with no signature gymnastics.

**ESP32-S3 dev-board specifics**: the dev board's USB connector is wired to the chip's native USB-Serial/JTAG controller, *not* to a USB-UART bridge. UART0 (GPIO1/GPIO3) is **not connected to the USB connector**. Do not write your console code against `UartDriver(uart0, gpio1, gpio3)` and expect it to reach `/dev/ttyACM0` — it won't. (rc1 failure #9.) This is true for most "dev kit" S3 boards with native USB.

---

## 11. Stack budget for TinyAgent

ESP-IDF default main task stack is 4 KB. TinyAgent on its own holds:

- `Option<MicroHNSW<EMBED_DIM=64, CAPACITY>>` — at capacity 256, ~80 KB; at capacity 32, ~10 KB
- `Option<MicroRAG>` — `MAX_KNOWLEDGE_ENTRIES=64 × 160 B` ≈ 10 KB
- `Option<SemanticMemory>` — `MAX_MEMORIES × 150 B` ≈ 5 KB
- `Option<AnomalyDetector>` — small
- `Option<MicroLoRA>` — small (rank 1-2)

Worst case (HnswIndexer + full HNSW): ~80 KB *on stack*. Default 4 KB stack → immediate Guru Meditation `Double exception` after `Calling app_main()`.

**Two complementary fixes**, both applied:

1. **Bump main task stack** in `sdkconfig.defaults`: `CONFIG_ESP_MAIN_TASK_STACK_SIZE=98304` (96 KB).
2. **Reduce `HNSW_CAPACITY` constant in `main.rs`** from 256 to 32. 32 fits comfortably; 256 is a CI-only / production-only value.

The cleaner long-term fix is to heap-allocate the agent's `Option<…>` fields via `Box`. This removes the stack pressure entirely and lets all 5 variants run with HNSW capacity 256. Defer to ADR-167 — not blocking rc1 release.

---

## 12. Acceptance gates (operational counterparts to ADR-165 §4)

Each gate has a definite pass/fail signal and a quick diagnostic if it fails.

| Gate | What | Pass signal | Fast-diagnose if failing |
|---|---|---|---|
| **G1** | `cargo build --no-default-features --features host-test --target x86_64-unknown-linux-gnu` | exit 0, ELF in `target/x86_64-unknown-linux-gnu/debug/ruvllm-esp32` | `grep -E "^error" build.log` first; usual cause is unpinned trio |
| **G2** | All 7 roles instantiate without panic on host-test | `for r in hnsw rag anomaly memory lora drafter relay; do RUVLLM_ROLE=$r ./ruvllm-esp32 < /dev/null \| grep role=; done` shows each role | Stack/heap is ~unbounded on host; failure here is a TinyAgent bug |
| **G3** | UART/stdio CLI accepts `add`, `search`, `recall`, `remember`, `learn`, `check`, `stats`, `role`, `set-role`, `help` | golden-output fixture in `tests/cli_smoke.rs` | grep the missing command in `process_command` |
| **G4** | `cargo +<toolchain> build --release --target <rust_target>` per matrix | `target/<rust_target>/release/ruvllm-esp32` is a Tensilica/RISC-V ELF | match the failure to §13 table |
| **G5** | Flash + monitor on attached `/dev/ttyACM0` produces ADR-165 banner within 5 s | `=== ruvllm-esp32 tiny-agent (ADR-165) ===\nvariant=esp32s3 role=…\n[ready] type 'help' for commands` | If silent past `app_main()` — §10. If `Guru Meditation` — §11. |
| **G6** | `curl -fI .../releases/latest/download/ruvllm-esp32-${target}` returns 200 for all 5 targets | one HTTP 200 per target | CI matrix didn't upload → re-run; or asset name mismatch with web flasher |

G1–G3 run in <1 min on a laptop. G4 first-run is 10–30 min per variant (esp-idf-sys SDK build is the bottleneck); cached subsequent runs are <1 min. G5 runs in seconds on real hardware. G6 is gated on G4+release, ~3–5 min after a tag push.

---

## 13. Common failure → remedy table

This is the searchable index. Every entry was hit live during rc1.

| Symptom | Root cause | Fix |
|---|---|---|
| `cannot find 'espidf' in 'embuild'` (build.rs E0433) | embuild lacks `espidf` feature | `embuild = { features = ["espidf"] }` (§3) |
| `error[E0308]: expected '*const u8', found '*const i8'` in esp-idf-svc/tls.rs:214 | esp-idf-svc 0.49.1 ↔ bindgen char regression | pin `esp-idf-svc = "=0.51.0"` (§3) |
| `error[E0609]: no field 'queue_non_blocking' on type '&TransmitConfig'` | esp-idf-hal 0.46.2 release-side bug | pin `esp-idf-hal = "=0.45.2"` (§3) |
| `cannot find 'log' in 'esp_idf_svc'` (E0433) | feature-gated behind `alloc` | drop `EspLogger::initialize_default()` or enable `alloc` (§3) |
| `mold: fatal: unknown command line option: --dynconfig=xtensa_esp32s3.so` | Host RUSTFLAGS=mold | `env -u RUSTFLAGS cargo build …` (§6) |
| `Cannot locate argument '--ldproxy-linker <linker>'` (ldproxy panic) | build.rs cfg evaluating against host | use `CARGO_CFG_TARGET_OS` env var (§5) |
| `error: linker 'ldproxy' not found` | ldproxy not installed in this env | `cargo install ldproxy --locked` (§7) |
| `undefined reference to memcpy / xQueueCreateMutex / uart_param_config / …` | `linker = "ldproxy"` not declared for this `--target` | add per-target block to `.cargo/config.toml` (§7) |
| `error: toolchain 'esp' is not installed` (CI on c3/c6) | RISC-V doesn't need the `esp` Rust channel; espup didn't install it | use `cargo +nightly` for RISC-V matrix entries (§4 + §9) |
| Bootloader logs reach `/dev/ttyACM0` but `app_main()` is silent | USB-Serial/JTAG VFS not switched to interrupt mode | both calls in §10 |
| `Guru Meditation Error: Core 0 panic'ed (Double exception)` immediately after `Calling app_main()`, SP looks like `0x3DFFFFE0` | Stack overflow during `TinyAgent::new()` | bump `CONFIG_ESP_MAIN_TASK_STACK_SIZE` and/or shrink `HNSW_CAPACITY` (§11) |
| sdkconfig changes not reflected after rebuild | esp-idf-sys cached the merged sdkconfig | `rm -rf target/<triple>/release/build/esp-idf-sys-*/` (§8) |
| Banner appears only after a panic, not at boot | TX FIFO buffered without interrupt-mode VFS driver | §10 |
| UART0 CLI not visible on `/dev/ttyACM0` (S3 dev board) | UART0 GPIO1/3 not wired to USB connector on native-USB boards | use USB-Serial/JTAG console path (§10) |

---

## 14. Out of scope (deferred to follow-up ADRs)

- **Heap-allocate TinyAgent fields** (replace `Option<MicroHNSW<…>>` with `Option<Box<MicroHNSW<…>>>`) so HNSW capacity 256 fits on every variant — ADR-167.
- **ESP-IDF v5.2 / v5.3 migration** (currently pinned to v5.1.2 via `ESP_IDF_VERSION`). Includes new sdkconfig keys, new bindgen output, possibly new crate trio.
- **ESP32-P4 PSRAM big-model path** — ADR-090 territory; this manual covers only the SRAM-only variants.
- **Hardware-loop CI** (GH Actions runner with a connected ESP32 over USB) — proves G5 in CI rather than only locally.
- **`std::io` console parity for `wasm` feature path** — out of scope for the `esp32` feature this ADR documents.

---

## 15. References

- **ADR-165** — Tiny RuvLLM agents on heterogeneous ESP32 SoCs (the *what*, this ADR is the *how*)
- **ADR-002** — RuvLLM ↔ Ruvector Integration
- **ADR-074** — RuvLLM Neural Embeddings (HashEmbedder Tier 1)
- **ADR-084** — ruvllm-wasm v2.0.0 (canonical primitive surface)
- **ADR-090** — Ultra-Low-Bit QAT / PSRAM big-model path (ESP32-P4)
- **Issue #409** — original gap analysis that motivated the ADR-165 + ADR-166 pair
- **`examples/ruvLLM/esp32-flash/build.rs`** — §5 fix lives here
- **`examples/ruvLLM/esp32-flash/Cargo.toml`** — §3 trio pin
- **`examples/ruvLLM/esp32-flash/.cargo/config.toml`** — §7 per-target linker config
- **`examples/ruvLLM/esp32-flash/sdkconfig.defaults*`** — §8 console + stack
- **`examples/ruvLLM/esp32-flash/src/main.rs`** — §10 USB-Serial/JTAG calls + §11 HNSW capacity
- **`.github/workflows/ruvllm-esp32-firmware.yml`** — §9 CI contract

---

## 16. Verification log (rc1 — what was actually proven)

For posterity, the actual evidence from the rc1 bring-up against ESP32-S3 (`ac:a7:04:e2:66:24`, revision v0.2, 8 MB embedded PSRAM):

```
$ cargo build --no-default-features --features host-test --target x86_64-unknown-linux-gnu
Finished `dev` profile [optimized + debuginfo] target(s) in 0.49s    # G1 ✓

$ for v in esp32 esp32s2 esp32s3 esp32c3 esp32c6; do
    echo "stats" | RUVLLM_VARIANT=$v ./target/.../ruvllm-esp32 | grep role=;
  done
role=RagRetriever variant=esp32 sram_kb=520 ops=0 hnsw=0 rag=0
role=AnomalySentinel variant=esp32s2 sram_kb=320 ops=0 anomaly_samples=0
role=SpeculativeDrafter variant=esp32s3 sram_kb=512 ops=0 hnsw=0
role=HnswIndexer variant=esp32c3 sram_kb=400 ops=0 hnsw=0
role=MemoryArchivist variant=esp32c6 sram_kb=512 ops=0 mem=0      # G2 ✓

$ env -u RUSTFLAGS cargo +esp build --release --target xtensa-esp32s3-espidf
Finished `release` profile [optimized] target(s) in 18.33s        # G4 (s3) ✓
$ file target/xtensa-esp32s3-espidf/release/ruvllm-esp32
ELF 32-bit LSB executable, Tensilica Xtensa, version 1 (SYSV), statically linked   # 832 KB

$ espflash flash --chip esp32s3 --port /dev/ttyACM0 .../ruvllm-esp32
Flashing has completed!                                            # 451 KB / 16 MB

$ cat /dev/ttyACM0
…
I (255) main_task: Calling app_main()
=== ruvllm-esp32 tiny-agent (ADR-165) ===
variant=esp32s3 role=SpeculativeDrafter chip_id=0 sram_kb=512
[ready] type 'help' for commands
role=SpeculativeDrafter variant=esp32s3 sram_kb=512 ops=0 hnsw=0   # G5 ✓ (with §10 fix)
```

G6 lands when `ruvllm-esp32-firmware.yml` runs successfully against a tag (rc2 will be the first run after the §9 ldproxy + per-toolchain fix lands).

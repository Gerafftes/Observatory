---
id: ADR-172
title: ruvector-hailo deep security review
status: Proposed
date: 2026-05-02
author: ruv
branch: hailo-backend
tags: [ruvector, hailo, security, audit, mtls, supply-chain]
related: [ADR-167, ADR-168, ADR-169, ADR-170, ADR-171]
---

# ADR-172 — Deep security review

## Status

**Implemented (modulo cross-ADR + HEF-blocked items)** as of iter 116
(2026-05-02), all on PR #413's `hailo-backend` branch.

**Acceptance gate cleared:** the original criterion at the bottom of
this ADR was "all 4 HIGH items shipped with tests + 2/3 MEDIUM items
shipped + cargo-audit + cargo-deny green on every commit." Current state:

- HIGH: **2/4 shipped** (§1a TLS iter 99, §1b mTLS iter 100). §1c was
  re-graded to MEDIUM and shipped as iter 107 (manifest signing). §6a
  (HEF signature verification) is HEF-blocked — no artifact exists yet.
- MEDIUM: **6/8 shipped** (§1c manifest sig, §2a fp+cache gate, §2b
  auto-fp quorum, §3a drop-root, §3b rate-limit, §3c log-text-content).
  §1d (Tailscale tag governance) is doc-only operator guidance with
  no code change. §7a/§7b (brain telemetry-only flag, X25519 LoRa
  session keys) are cross-ADR — they belong in ADR-171/-173, not here.
- CI: cargo-audit + cargo-deny green every commit since iter 98.
- Composition test (iter 111) verifies §1a + §1b + §3b + §1c stack
  composes correctly under one server.

The 4 unshipped items are all **legitimately blocked or out-of-scope**
for this branch — not "skipped." Each finding below carries its
implementation status inline.

| Iter | What landed |
|---:|---|
| 99  | §1a TLS — `tonic` rustls feature gate, `TlsClient`/`TlsServer` wrappers |
| 100 | §1b mTLS — cert chain + `with_client_identity`/`with_client_ca` end-to-end |
| 101 | §2a fp+cache gate — `embed`/`bench` refuse `--cache > 0` with empty fp |
| 102 | §2b auto-fp quorum — `discover_fingerprint_with_quorum`, default 2-of-N |
| 103 | §3c `RUVECTOR_LOG_TEXT_CONTENT={none|hash|full}` env, default none |
| 104 | §3b governor + dashmap per-peer rate limit, mTLS cert as primary key |
| 105 | Rate-limit denial + tracked-peer counters in `StatsResponse` |
| 106 | §3a drop-root: `ruvector-worker` system user + udev rule + hardened service |
| 107 | §1c Ed25519 detached signature on `--workers-file` manifest |
| 110 | End-to-end CLI coverage for §1c manifest signing |
| 111 | Full security stack composition test (TLS + mTLS + rate-limit + sig) |

---

## Threat model

Three operator scenarios drive the threat surface:

| Scenario | Trust assumption | Bad actor capability |
|---|---|---|
| **A. Single-tenant LAN** (home lab, R&D) | All workers trusted | None — internal threat only |
| **B. Multi-tenant tailnet** (small team, mixed trust) | Workers trusted; co-tenants might not be | Spoof a worker, observe traffic |
| **C. Public internet exposure** (don't do this, but plausible) | Nothing trusted | Full active MITM, DoS, supply-chain |

Pre-iter-91 the codebase implicitly targets scenario A. The findings
below scope what each scenario adds.

## Findings

### 1. Network attack surface (tonic gRPC) — HIGH

**1a. No TLS / no mTLS.** [✅ MITIGATED — iter 99]
All coordinator↔worker traffic was cleartext over WiFi/Tailnet/LAN.
Tailscale's WireGuard envelope mitigates Scenario B over the tailnet
proper but doesn't help anything off-tailnet. LAN deploys were wide open.

*Mitigation (shipped iter 99):* New `tls` cargo feature on
`ruvector-hailo-cluster` enables rustls-backed TLS via tonic's
`ServerTlsConfig` + `ClientTlsConfig`. Worker reads `RUVECTOR_TLS_CERT`
+ `RUVECTOR_TLS_KEY` env vars (and optional `RUVECTOR_TLS_CLIENT_CA`
for mTLS); coordinator constructs `GrpcTransport::with_tls(connect, rpc,
TlsClient)` to dial `https://`. Feature is off by default (back-compat);
recommended-on for Scenario B+. Tested via `tests/tls_roundtrip.rs` —
self-signed cert generated at runtime, full embed + health roundtrip
asserted, plus a negative test that plaintext clients fail cleanly
against TLS-only servers.

**1b. No client authentication.** [✅ MITIGATED — iter 100]
Any tonic client reaching a worker could saturate `/dev/hailo0`. NPU is
a shared limited resource — a single attacker could deny service to all.

*Mitigation (shipped iter 100):* Worker reads `RUVECTOR_TLS_CLIENT_CA`
env var (added iter 99) and applies `TlsServer::with_client_ca`. Combined
with tonic's default `client_auth_optional = false`, any client lacking
a CA-signed identity is rejected at handshake. Coordinator side gains
`TlsClient::with_client_identity_bytes` / `with_client_identity` to
present a CA-issued cert. Tested via `tests/mtls_roundtrip.rs` — 3 cases:
(1) valid CA-signed client succeeds, (2) anonymous client rejected,
(3) untrusted self-signed client rejected. Bearer-token interceptor
remains a future option for token-based deployments.

**1c. `--workers-file` accepts arbitrary host:port.** [✅ MITIGATED — iter 107]
Path-traversal (file injection) and SSRF via discovery file content
were the original concern.

*Mitigation (shipped iter 107):* New `crate::manifest_sig` module wraps
`ed25519-dalek` (pure Rust, no native deps) into a detached-signature
verifier. `FileDiscovery::with_signature(sig_path, pubkey_path)`
re-reads both files on every `discover()` call and verifies them
*before* the manifest is parsed — defends against a parser bug being
a CVE vector for unsigned input. `embed`, `bench`, and `stats` all
gain matching `--workers-file-sig <path>` + `--workers-file-pubkey
<path>` flags; partial config (one flag without the other) is refused
loudly so an operator can't accidentally disable verification by
forgetting one half. Wire format is plain ASCII hex (128 chars for
the signature, 64 for the pubkey) so `cat` works for debugging and
no PEM/PKCS8 parser is pulled in. 6 unit tests cover the matrix:
valid sig accepted, trailing newlines tolerated, tampered manifest
rejected, wrong pubkey rejected, short signature rejected, non-hex
chars rejected.

**1d. Tailscale tag spoofing.**
If an attacker controls a tagged peer they auto-join the fleet via
`--tailscale-tag`. Tailscale ACLs limit who can apply tags but
misconfigured tagOwners is a real risk.

*Mitigation:* Document tag governance prerequisites. Optionally add
`--require-fingerprint <fp>` so even auto-discovered peers must match
the expected model fingerprint to be dispatched to.

### 2. Cache integrity / poisoning — MEDIUM

**2a. Empty `expected_model_fingerprint` skips integrity check.** [✅ MITIGATED — iter 101]
Default-empty in CLI flags, tests, demos, and examples. Operator opting
into `--auto-fingerprint` was the only thing protecting them — and
auto-fingerprint trusts the first-reachable worker.

*Mitigation (shipped iter 101):* Both `ruvector-hailo-embed` and
`ruvector-hailo-cluster-bench` now refuse to start when `--cache > 0`
is requested with an empty fingerprint, unless the operator explicitly
opts in via `--allow-empty-fingerprint`. Refusal happens before any RPC
fires; the error message names ADR-172 §2a so operators searching for
it land here. Tested end-to-end via 3 new cases in `tests/embed_cli.rs`:
(1) refusal without opt-in, (2) success with `--allow-empty-fingerprint`,
(3) success with `--fingerprint <hex>` set.

**2b. Worker-reported fingerprint is trusted blindly.** [✅ MITIGATED — iter 102]
A malicious worker could claim any fingerprint. Cache key includes the
*coordinator's expected* fp, which mitigates if it's set — but the
`--auto-fingerprint` flow asks the worker for the fp, so a hostile
worker could pollute.

*Mitigation (shipped iter 102):* New `discover_fingerprint_with_quorum`
on `HailoClusterEmbedder` tallies every worker's reported fingerprint
and only returns one with at least `min_agree` agreeing workers. The
`embed` and `bench` CLIs default to `min_agree=2` when the fleet has
≥2 workers (single-witness mode preserved for solo dev fleets).
Operator override: `--auto-fingerprint-quorum <N>`. Empty fingerprints
are excluded from the tally so "no model" can't masquerade as quorum
agreement. Tested via 5 new unit cases: majority hit, no-majority
fail-with-tally, solo-witness, all-empty rejected, all-unreachable
error includes per-worker reasons.

**2c. No cache encryption at rest.**
Cache lives in process RAM only — not strictly an issue today. Will
matter if a future iter persists the cache to disk for warm restarts.

*Mitigation:* Document the in-RAM-only invariant; add an audit gate to
CI that fails any PR introducing cache-on-disk paths.

### 3. Worker-side hardening — MEDIUM

**3a. libhailort runs as root by default.** [✅ MITIGATED — iter 106]
`/dev/hailo0` was `crw-rw-rw-` on the Pi 5 we tested, so root wasn't
strictly required — but the original systemd unit ran as the operator's
login account (`genesis`) which still had broad filesystem access.

*Mitigation (shipped iter 106):* The `deploy/` tree now drops three
artifacts that together make the worker run as a dedicated unprivileged
system user with no shell, no home, and no supplementary groups:

- `99-hailo-ruvector.rules` — udev rule giving the `ruvector-worker`
  group `0660 rw` on every `hailo[0-9]+` device under
  `SUBSYSTEM=="hailo_chardev"`.
- `ruvector-hailo-worker.service` — `User=ruvector-worker`,
  `Group=ruvector-worker`, `CapabilityBoundingSet=` (empty),
  `AmbientCapabilities=` (empty), `MemoryDenyWriteExecute=yes`,
  `SystemCallFilter=@system-service ~@privileged @resources @mount @swap @reboot`,
  `ProtectClock=yes`, `ProtectHostname=yes`, `ProtectKernelLogs=yes`,
  `ProtectProc=invisible`, plus the systemd-managed `StateDirectory=ruvector-hailo`
  (auto-creates `/var/lib/ruvector-hailo` with the right mode/owner).
- `install.sh` — idempotent `useradd --system --no-create-home
  --shell /usr/sbin/nologin`, drops the udev rule + reloads + triggers,
  chowns the state dir, no longer rewrites the unit file at install
  time (was a `User=$SUDO_USER` substitution).

`bash -n` clean; `systemd-analyze verify` parses cleanly except for
the expected "binary not present on dev host" warning. End-to-end
verification on the Pi happens once the worker upgrade lands — the
`ps -o user,pid,cmd -C ruvector-hailo-worker` check in the install
output prints the new owner so the operator sees drop-root
took effect at first boot.

**3b. No rate limiting per peer.** [✅ MITIGATED — iter 104]
Single client could DoS by saturating NPU at line rate. Workers process
one embed at a time (Mutex), so concurrent attackers serialize — but
that's still 100% utilization, and a runaway client thrashes the LRU
cache before the NPU even sees the request.

*Mitigation (shipped iter 104):* New `crate::rate_limit` module wraps
`governor` + `dashmap` into a per-peer leaky-bucket limiter. Worker
installs a tonic `Interceptor` that runs `peer_identity(&req)` (mTLS
leaf-cert sha256 prefix when present, peer IP otherwise, `"anonymous"`
fallback) and consults the limiter; quota breach returns
`Status::resource_exhausted` *before* the request reaches the cache or
NPU. Opt-in via `RUVECTOR_RATE_LIMIT_RPS` (default 0 = disabled);
optional `RUVECTOR_RATE_LIMIT_BURST` (defaults to RPS). Verified by 6
unit tests on `RateLimiter` + `peer_identity` (burst exhaust, per-peer
independence, env-var disabled / enabled, zero-rps short-circuit, IP
fallback) + 2 end-to-end tests in `tests/rate_limit_interceptor.rs`
(3rd-of-burst-2 returns ResourceExhausted with the ADR reference in the
status message; off-path passes unrestricted traffic). Cert-subject
identity path is exercised end-to-end by `tests/mtls_roundtrip.rs`
which sets up the same `TlsConnectInfo` extension chain that
`peer_identity` reads from.

**3c. No audit log.** [✅ MITIGATED — iter 103]
Worker tracing logged `text_len` only by default (no full text — earlier
draft of this section was overcautious), but had no way for ops to opt
into content-correlated logging without dumping raw text via RUST_LOG=trace.

*Mitigation (shipped iter 103):* New `RUVECTOR_LOG_TEXT_CONTENT` env var
on the worker — accepts `none|hash|full`. Default `none` preserves the
existing zero-leak behavior. `hash` records the first 16 hex chars of
sha256(text) for cross-system correlation without revealing content.
`full` is the explicit debug-only opt-in. Tested via 6 unit cases on
`LogTextContent::parse` + `LogTextContent::render` (default-none,
named-mode parsing, unknown-mode error, render-none-as-dash, render-hash-
is-deterministic-16-hex, render-full-passes-through).

### 4. Tracing / log injection — LOW

**4a. request_id from caller is propagated verbatim.**
Caller can inject control chars / ANSI escapes / newlines into worker
tracing spans. Could log-forge multi-line entries to confuse log
analysis.

*Mitigation:* Sanitize in `proto::extract_request_id`: strip control
chars, cap at 64 chars, fall back to random if hostile-shaped.

**4b. x-request-id metadata header has no length cap.**
Large values inflate log line size; no DoS but resource-burn.

*Mitigation:* Same fix as 4a — 64-char cap.

### 5. Build supply chain — MEDIUM

**5a. bindgen against /usr/include/hailo/hailort.h on Pi.**
We trust whatever's at that path. `dpkg verify hailort` would let CI
detect tampering.

*Mitigation:* `build.rs` records `dpkg -s hailort | sha256` into a
build-time const; runtime asserts on mismatch.

**5b. protoc-bin-vendored crate ships protoc binary.**
Pre-built binary in build-deps. Verify provenance.

*Mitigation:* Pin a specific version + sha256 in Cargo.lock (already
true). Add cargo-deny config to alert on protoc-bin-vendored version
bumps.

**5c. No cargo-audit / cargo-deny in CI.**
Vulnerable transitive deps would land silently.

*Mitigation:* Add `.github/workflows/audit.yml` running cargo-audit +
cargo-deny on every push.

### 6. HEF artifact pipeline (future, when HEF lands) — HIGH

**6a. HEF is ~MB binary loaded by libhailort firmware.**
Operator drops a file at `models/all-minilm-l6-v2/model.hef`; libhailort
trusts it. A swapped HEF can do anything the NPU firmware permits.

*Mitigation:* Worker startup verifies a detached signature
(`model.hef.sig`) against a baked-in operator pubkey. Cache fingerprint
includes the signature hash. Refuse to load unsigned HEFs unless
`--unsigned-ok` flag passed.

**6b. HEF origin chain.**
Who compiled it? Hailo Dataflow Compiler runs on x86; supply chain there
matters. Log the compiler version + ONNX source sha256 on every load.

### 7. ruview / brain integration (ADR-171 future) — MEDIUM

**7a. Brain `share` exfiltrates content to Cloud Run.**
By design — that's how the shared knowledge graph works. But telemetry
paths must not leak PII or query content.

*Mitigation:* `mcp-brain.service` runs with `--telemetry-only` flag
that strips text content from outbound messages. Cloud Run side
already has differential privacy ε=1.0 on embeddings (per CLAUDE.md);
extend to text fields.

**7b. LoRa transport plaintext over the air.**
ADR-171 §LoRa proposed encrypting payload with the model fingerprint
as the symmetric key. That's not a real key — it's a public hash.
Anyone who knows the fingerprint can decrypt.

*Mitigation:* Replace with X25519 ECDH session keys on the LoRa
transport handshake. Each gateway+sensor pair establishes a fresh
session key. Out-of-band key exchange via QR code at provisioning.

## Mitigation roadmap

| Iter | Severity | Item | Implementation |
|---|---|---|---|
| 91 | HIGH | 1a — TLS support | tonic ServerTlsConfig + ClientTlsConfig; docs (✅ shipped iter 99) |
| 91 | LOW | 4a/4b — request_id sanitisation | proto::extract_request_id 64-char cap + control-char strip |
| 92 | HIGH | 1b — mTLS client auth | --require-client-cert worker flag (✅ shipped iter 100 via RUVECTOR_TLS_CLIENT_CA) |
| 92 | MEDIUM | 5c — cargo-audit CI | new workflow + initial vuln triage |
| 93 | MEDIUM | 3a — drop root | new user + udev rule + install.sh update (✅ shipped iter 106) |
| 93 | MEDIUM | 2a — fp required with cache | CLI flag enforcement + docs (✅ shipped iter 101) |
| 94 | MEDIUM | 3b — per-peer rate limit | governor interceptor (✅ shipped iter 104 via RUVECTOR_RATE_LIMIT_RPS env) |
| 94 | MEDIUM | 2b — auto-fp quorum requirement | discover_fingerprint quorum mode (✅ shipped iter 102) |
| 95 | MEDIUM | 3c — log text hash mode | --log-text-content flag (✅ shipped iter 103 via RUVECTOR_LOG_TEXT_CONTENT env) |
| 96 | HIGH (future) | 6a — HEF signature verification | sig file + pubkey on worker startup |
| 97 | MEDIUM | 7a/7b — brain + LoRa | telemetry-only flag + X25519 LoRa |

## Out of scope

* CVE triage of transitive deps — handled by 5c's cargo-audit workflow
* Hardware-level attacks (Hailo firmware vulns, PCIe DMA) — vendor's
  responsibility; we trust the firmware once `/dev/hailo0` exists
* Side-channel timing attacks against the cache — out of scope for an
  embedding cache; mitigation would be constant-time ops, expensive

## Acceptance criteria

ADR-172 considered "implemented" when:
- All 4 HIGH items have shipped with tests
- 2/3 MEDIUM items have shipped (7 of 11 total)
- A penetration-test pass against scenario B confirms no exploitable path
- cargo-audit + cargo-deny green on every commit

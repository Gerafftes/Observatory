# Changelog — @ruvector/rvagent-wasm

All notable changes to this package will be documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [0.2.0] — 2026-05-27

### Added
- `CHANGELOG.md` (this file)
- ruflo / `@claude-flow/cli` integration documentation in README
- ADR-129 reference: documents `JsModelProvider` + `addMcpTools()` wiring patterns
- Explicit Node.js CommonJS usage examples (`require('@ruvector/rvagent-wasm/rvagent_wasm.js')`)
- `WasmRvfBuilder.addMcpTools()` usage example showing ruflo ADR-129 Gap 2 pattern
- `WasmGallery` direct API examples (Gap 3 / Gap 4 methods already implemented)
- Security limits documented: RVF 10 MB max, capability delegation depth 10, gallery 100 custom

### Changed
- Version bumped from 0.1.0 to 0.2.0
- README corrected: package name is `@ruvector/rvagent-wasm` (not unscoped `rvagent-wasm`)
- README corrected: published artifact uses `nodejs` wasm-pack target (not `web`)
- README: import example updated from browser ESM to Node.js CommonJS (primary usage)
- `pkg/package.json` name field fixed to `@ruvector/rvagent-wasm` (scoped)

### No Rust changes
All WASM-level APIs are identical to 0.1.0. The full ADR-129 gap surface
(`JsModelProvider`, `set_model_provider`, `addMcpTools`, `get_state`, `get_todos`, `reset`,
all `WasmGallery` methods) was already implemented in 0.1.0. This release documents and
republishes with corrected metadata.

### Compatible with
- `@claude-flow/cli >= 3.10.4`
- wasm-pack 0.14.x, 0.15.x
- Rust 1.80+, wasm32-unknown-unknown target

---

## [0.1.0] — 2026-03-17

### Added
- Initial release
- `WasmAgent`: agent execution with virtual filesystem, conversation history, `JsModelProvider` callback
- `JsModelProvider`: JS-to-Rust model callback bridge (`async (messagesJson) => string`)
- `WasmRvfBuilder`: RVF cognitive container builder (tools, prompts, skills, mcp_tools, capabilities, orchestrator)
- `WasmGallery`: 6 built-in templates (coder, researcher, tester, reviewer, security, swarm-orchestrator)
- `WasmMcpServer`: MCP JSON-RPC server running in WASM
- Virtual filesystem tools: `read_file`, `write_file`, `edit_file`, `list_files`, `write_todos`
- SHA3-256 checksum on RVF containers
- Security: request size limits, path traversal protection, capability delegation depth limit

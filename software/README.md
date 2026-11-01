# Observatory software source

`software/ruview/` contains the complete source snapshot used by Observatory:

- the browser UI under `ui/`
- the Rust workspace and sensing backend under `v2/`
- the ESP32 CSI and mmWave firmware under `firmware/`
- the historical Python backend under `archive/v1/`
- scripts, tests, architecture documents and D4/D5/D6 implementation paths
- all eight upstream submodule source trees as ordinary vendored files

A clone of Observatory therefore does not need a second checkout from
`ruvnet/RuView`. Cargo, npm, Python and ESP-IDF may still download normal
package dependencies when they are not already cached.

## Provenance

The parent source snapshot is based on RuView commit
`bbf99e2dc94458e80121d2bacd9a8f229acb5a7c` (`feat: publish RuView calibration
workflow and Observatory docs`). The following former submodules are vendored
at their pinned commits:

| Path | Commit |
|---|---|
| `vendor/midstream` | `8f70d2bb9d64a8ddee7745bc18bc4ff9e151845e` |
| `vendor/ruvector` | `a083bd77fa2f4879595daa68686ed5b2132d981a` |
| `vendor/sublinear-time-solver` | `c25dddf163d8c413628ecdc6e979583d39270f22` |
| `vendor/rvcsi` | `72891d740f92903c78a8208a9069f6c82a4d1cc2` |
| `vendor/rufield` | `509d8ae29e654a322910bd504d325b0dd1fdd895` |
| `v2/crates/ruv-neural` | `c9638faaf8ae1d910039171be487a465a5762313` |
| `v2/crates/ruview-swarm` | `267aba5be2288aa6cbe574492062b04fa8c8a6ce` |
| `v2/crates/worldgraph` | `fdade422069d3162634292710d78cb9963c0f48d` |

Observatory adaptations include the experiment cockpit and calibration UI,
RX source binding and recorder/audit paths, RX unified/peer measurement support,
TX safety and maintenance tools, and local regression tests. See
[RuView adaptations](../docs/project/06-ruview-anpassungen.md) and the
[current result report](../docs/project/rx-unified-tag14-result-20261029.md).

This is a source distribution. Local agent settings and memories, generated
machine-specific SDK configuration, learning-session data, and an unrelated
extracted third-party CLI research dump are omitted. Their private originals
remain in local storage. Required runtime sources and third-party license/notice
files remain included. Normal development dependencies are obtained through the
package managers; no second RuView checkout or submodule initialization is needed.

## Prerequisites

- Git and a current stable Rust/Cargo toolchain (locally checked with Rust 1.96).
- Node.js for the browser's dependency-free `node --test` suite.
- Python 3.15 for the existing tooling that uses lazy-import syntax.
- KiCad 10 and its standard libraries for [editable hardware](../hardware/KICAD.md).
- Firmware builds additionally need the ESP-IDF version and target setup stated
  in the corresponding firmware guide; simulation does not need ESP-IDF or boards.

## Build and run

From the Observatory repository root:

```bash
cd software/ruview/v2
cargo check --locked -p wifi-densepose-sensing-server --no-default-features
WDP_RUFIELD_SIGNING_SEED="$(python3 -c 'import secrets; print(secrets.token_hex(32))')" \
cargo run --locked -p wifi-densepose-sensing-server --no-default-features -- \
  --source simulate --rx-session-only --bind-addr 127.0.0.1 \
  --http-port 3002 \
  --ws-port 3001
```

Then open `http://127.0.0.1:3002/ui/index.html#sensing`. Simulation binds to loopback and disables mmWave network/control paths.
The generated signing seed exists only in the demo process environment. A real
measurement setup needs its own persistent private signing key; never put that
key in an example file or a commit.
A first build may need Internet access to download ordinary dependencies.

From the repository root, run the publication and browser checks:

```bash
sh project_tools/verify_observatory_source.sh
python3 project_tools/check_publication.py --secrets
node --test software/ruview/ui/tests/*.test.mjs
```

The secret check requires Gitleaks 8.30.1 or newer. It scans locally, includes
archive contents up to depth two, and reports only paths and rule names. Reviewed
synthetic fixtures are bound to their complete file hashes; a changed file must
be reviewed again. This checks the proposed tree; a separate history scan is
required before publishing a new commit range. See [publication rules](../docs/PUBLICATION.md).

`project_tools/csi-host-ip-alias.sh` is an optional macOS operational helper,
not part of simulation. Its privileged modes require your own TX identity in
`OBSERVATORY_TX_BSSID`. Installing it is a separate network operation; the
hardware-free `self-test` never needs that identity.

Simulation and software tests do not prove real CSI, mmWave operation or
position accuracy. Hardware results remain subject to the setup, preflight,
calibration and blind-validation gates documented in the repository root.

## Licenses

The vendored source keeps its original `LICENSE`, `LICENSE.md`, `NOTICE` and
component-level license files. Those files govern their respective source
trees; the Observatory root license does not replace third-party notices.

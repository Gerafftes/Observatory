# Fixed-room position manifest templates

These templates expose the strict JSON input shapes used by the sensing
server without publishing a real room setup, raw transmitter MAC, WiFi
credentials, or capture identities.

The `__UPPER_CASE__` values are required placeholders. The template files are
valid JSON for tooling, but they are deliberately not valid sensing-server
inputs until every placeholder has been replaced with measured or inspected
data. Do not replace a SHA-256 placeholder with zeroes merely to pass a shape
check.

## Setup specification

Copy `position-setup-spec.template.json` to a private working directory and
fill it from the final physical setup:

- all coordinates use `[x_length, y_height, z_width]`
- `room_dimensions_mm` and every `position_mm` are integer millimetres
- firmware hashes are SHA-256 over the exact deployed artifacts
- each RX `expected_grid` comes from the final live discovery, not an assumed
  or historical grid
- `recording_host` describes the Mac at its normal measurement position and
  its actual cable revision
- revision strings identify the frozen room, furniture, and door state
- `tx_filter_identity.sha256` is computed privately over exactly the six
  binary filter-MAC bytes in network order; never put the raw MAC in this file

The scheme and coordinate-system strings are protocol constants and must not
be changed. Receiver entries must remain exactly RX1 through RX4 in that
order.

Check that no placeholder remains:

```bash
rg -n '__[A-Z0-9_]+__' private/setup-spec.json
```

The command must print nothing. Then let the actual Rust schema validate and
seal the completed specification:

```bash
cargo run --manifest-path v2/Cargo.toml \
    -p wifi-densepose-sensing-server -- \
    --position-create-setup private/setup-spec.json \
    --position-output private/sealed-setup.json
```

Creation fails on unknown or missing fields, invalid dimensions, coordinates
outside the room, wrong receiver order, grid/channel disagreement, malformed
hashes, or path/IP/raw-MAC-shaped public identifiers. It also binds the exact
server executable to the sealed artifact.

## Position training manifest

Copy `position-training-manifest.template.json` beside the private capture
set. Fill it only after the empty and P01-P09 captures have passed
`--position-inspect`:

- copy `setup_id` and `setup_sha256` from the sealed setup
- copy `geometry` exactly from the sealed setup, converting millimetres to
  metres without changing the coordinate order
- paths may be relative to the training manifest and must point to the exact
  raw files inspected
- copy `recording_id`, `raw_sha256`, `metadata_sha256`, and `signal_sha256`
  from inspection output; do not invent or reuse identities
- keep points exactly P01 through P09 with unique measured floor coordinates
- add more objects to a point's `captures` array only for genuinely separate
  captures

Again, the placeholder check must print nothing:

```bash
rg -n '__[A-Z0-9_]+__' private/training-manifest.json
```

Build the index through the strict Rust loader:

```bash
cargo run --manifest-path v2/Cargo.toml \
    -p wifi-densepose-sensing-server -- \
    --position-build-index private/training-manifest.json \
    --position-output private/position-index.json
```

The build additionally checks exact capture and sidecar bytes, setup binding,
server version, four-RX geometry and grid identity, duration, rate, window
coverage, and duplicate or overlapping signal identities. A template passing
JSON parsing alone is therefore never evidence that a real training set is
valid.

## Repository validation

Focused Rust tests render both public templates with synthetic test-only
values and pass them through the same private schema validators used by the
CLI:

```bash
cargo test --manifest-path v2/Cargo.toml \
    -p wifi-densepose-sensing-server \
    public_setup_template_matches_strict_schema

cargo test --manifest-path v2/Cargo.toml \
    -p wifi-densepose-sensing-server \
    public_training_template_matches_strict_schema
```

The synthetic replacements exist only inside tests. No generated setup or
capture in this directory represents a real measurement.

## Private classification truth

After the unlabelled replay report has been written, generate a private truth
template directly from its capture identities:

```bash
python3 scripts/build_classification_truth_template.py \
    private/classification-predictions.json \
    private/classification-truth.json
```

The output is created with mode `0600` and is never overwritten. Replace every
`__SET_TRUE_OR_FALSE__` value with a JSON boolean. Empty checks use
`expected_point_id: null`; each occupied blind capture uses its actual `P01`
through `P09`. The completed protocol must contain exactly three empty checks
and two occupied captures for each of the nine points.

Evaluate only after the unlabelled predictions are frozen:

```bash
v2/target/release/sensing-server \
    --classification-evaluate private/classification-predictions.json \
    --classification-truth private/classification-truth.json \
    --classification-output private/classification-report.json
```

After the separate position report exists, create the single final verdict:

```bash
v2/target/release/sensing-server \
    --experiment-classification-report private/classification-report.json \
    --experiment-position-report private/position-report.json \
    --experiment-output private/experiment-report.json
```

The final report passes only when Classification and Position both pass and
both reports belong to the same sealed setup.

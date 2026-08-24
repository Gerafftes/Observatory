#!/bin/sh
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

(cd "$repo_dir/v2" && cargo test -p wifi-densepose-sensing-server \
  mmwave_position_index::tests::synthetic_guided_training_round_trips_and_predicts_without_radar \
  --no-default-features)
(cd "$repo_dir/v2" && cargo test -p wifi-densepose-sensing-server \
  calibration_dataset::tests:: \
  --no-default-features)
(cd "$repo_dir/v2" && cargo test -p wifi-densepose-sensing-server \
  mmwave_calibration::tests::preflight_reports_each_individual_blocker_and_stays_fail_closed \
  --no-default-features)
(cd "$repo_dir/v2" && cargo test -p wifi-densepose-sensing-server \
  mmwave_calibration::tests::synthetic_server_status_matches_the_ui_full_path_contract \
  --no-default-features)
(cd "$repo_dir" && node --test ui/tests/mmwave-calibration-assistant.test.mjs)

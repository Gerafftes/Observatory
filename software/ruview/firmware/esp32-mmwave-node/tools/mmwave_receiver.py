#!/usr/bin/env python3
"""Record timestamped LD2450 UDP measurements as JSONL."""

import argparse
import json
import socket
import time


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bind", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=5010)
    parser.add_argument("--expected-mode", required=True,
                        choices=("calibration", "reference"))
    parser.add_argument("--output", required=True)
    parser.add_argument("--require-single-target", action="store_true",
                        help="abort instead of recording an ambiguous frame")
    args = parser.parse_args()

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind((args.bind, args.port))
    with open(args.output, "x", encoding="utf-8", buffering=1) as output:
        while True:
            packet, address = sock.recvfrom(4096)
            received_ns = time.time_ns()
            measurement = json.loads(packet)
            if measurement.get("schema") != "ruview.mmwave.ld2450.v1":
                continue
            if measurement.get("mode") != args.expected_mode:
                raise RuntimeError(
                    f"Refusing {measurement.get('mode')!r} data while "
                    f"{args.expected_mode!r} was required"
                )
            present_targets = sum(
                target.get("present") is True
                for target in measurement.get("targets", [])
            )
            measurement["quality"] = {
                "present_target_count": present_targets,
                "single_target": present_targets == 1,
            }
            if args.require_single_target and present_targets > 1:
                raise RuntimeError(
                    f"Refusing ambiguous frame with {present_targets} targets"
                )
            measurement["host_receive_time_ns"] = received_ns
            measurement["source_ip"] = address[0]
            output.write(json.dumps(measurement, separators=(",", ":")) + "\n")


if __name__ == "__main__":
    main()

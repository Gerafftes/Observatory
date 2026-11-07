#!/usr/bin/env python3.15
"""
Cognitum Seed — Happiness Vector Query Tool

Query the Seed's vector store for happiness patterns across ESP32 swarm nodes.
Demonstrates kNN search, drift monitoring, and witness chain verification.

Usage:
    python seed_query.py --seed https://10.1.10.236:8443 --token TOKEN status
    python seed_query.py --seed-ca-file /path/to/seed-ca.pem status

Requirements:
    Python 3.15+ (stdlib only, no dependencies)

The HTTPS URL must match the server certificate. For private trust, obtain
the CA or self-signed certificate PEM through a trusted channel and pass
--seed-ca-file. The PEM replaces system trust. Redirects are rejected.
"""

lazy import argparse
lazy import json
lazy import sys
lazy import time
lazy import ssl
lazy import urllib.parse
lazy import urllib.request
lazy import urllib.error


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        # Keep credentials at the operator's configured SEED endpoint.
        return None


def api(base, path, token=None, method="GET", data=None, ca_file=None):
    """Make an API request to the Seed."""
    try:
        parsed = urllib.parse.urlsplit(base)
        port = parsed.port
        if (
            parsed.scheme != "https"
            or not parsed.hostname
            or port == 0
            or parsed.netloc.endswith(":")
            or parsed.username is not None
            or parsed.password is not None
            or "?" in base
            or "#" in base
            or any(char.isspace() or ord(char) < 32 or ord(char) == 127 for char in base)
            or "%" in parsed.netloc
            or "\\" in parsed.netloc
        ):
            raise ValueError("SEED URL must be HTTPS without user information, query or fragment")
        if not path.startswith("/") or path.startswith("//"):
            raise ValueError("SEED API path must start with a single slash")
        if ca_file == "":
            raise ValueError("SEED CA file must not be empty")

        tls = ssl.create_default_context(cafile=ca_file)
        opener = urllib.request.build_opener(
            urllib.request.HTTPSHandler(context=tls), NoRedirect(),
        )
        url = f"{parsed.geturl().rstrip('/')}{path}"
        headers = {"Content-Type": "application/json"}
        if token:
            headers["Authorization"] = f"Bearer {token}"
        body = json.dumps(data).encode() if data else None
        req = urllib.request.Request(url, data=body, headers=headers, method=method)
        with opener.open(req, timeout=5) as resp:
            return json.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        with e:
            return {"error": f"HTTP {e.code}", "detail": e.read().decode()[:200]}
    except Exception as e:
        return {"error": str(e)}


def print_header(title):
    print(f"\n{'=' * 60}")
    print(f"  {title}")
    print(f"{'=' * 60}")


def cmd_status(args):
    """Show Seed and swarm status."""
    print_header("Seed Status")
    s = api(args.seed, "/api/v1/status", args.token, ca_file=args.seed_ca_file)
    if "error" in s:
        print(f"  Error: {s['error']}")
        return
    print(f"  Device:     {s['device_id'][:8]}...")
    print(f"  Vectors:    {s['total_vectors']} (dim={s['dimension']})")
    print(f"  Epoch:      {s['epoch']}")
    print(f"  Store:      {s['file_size_bytes'] / 1024:.1f} KB")
    print(f"  Uptime:     {s['uptime_secs'] // 3600}h {(s['uptime_secs'] % 3600) // 60}m")
    print(f"  Witness:    {s['witness_chain_length']} entries")

    print_header("Drift Detection")
    d = api(args.seed, "/api/v1/sensor/drift/status", args.token,
            ca_file=args.seed_ca_file)
    if "error" not in d:
        print(f"  Drifting:   {d.get('drifting', False)}")
        print(f"  Score:      {d.get('current_drift_score', 0):.4f}")
        print(f"  Detectors:  {d.get('detectors_active', 0)} active")
        print(f"  Total:      {d.get('detections_total', 0)} detections")


def cmd_search(args):
    """Search for similar happiness vectors."""
    print_header("Happiness kNN Search")

    # Reference vectors for common moods
    refs = {
        "happy":   [0.8, 0.7, 0.9, 0.8, 0.6, 0.7, 0.9, 0.5],
        "neutral": [0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
        "stressed":[0.2, 0.3, 0.2, 0.2, 0.3, 0.3, 0.2, 0.7],
    }

    query = refs.get(args.mood, refs["happy"])
    print(f"  Query mood: {args.mood}")
    print(f"  Vector:     [{', '.join(f'{v:.1f}' for v in query)}]")
    print(f"  k:          {args.k}")
    print()

    result = api(args.seed, "/api/v1/store/search", args.token,
                 method="POST", data={"vector": query, "k": args.k},
                 ca_file=args.seed_ca_file)

    if "error" in result:
        print(f"  Error: {result['error']}")
        return

    neighbors = result.get("neighbors", result.get("results", []))
    if not neighbors:
        print("  No results found.")
        return

    print(f"  {'ID':>10}  {'Distance':>10}  {'Vector'}")
    print(f"  {'-'*10}  {'-'*10}  {'-'*40}")
    for n in neighbors:
        vid = n.get("id", "?")
        dist = n.get("distance", n.get("dist", 0))
        vec = n.get("vector", n.get("values", []))
        vec_str = "[" + ", ".join(f"{v:.2f}" for v in vec[:4]) + ", ...]" if len(vec) > 4 else str(vec)
        print(f"  {vid:>10}  {dist:>10.4f}  {vec_str}")


def cmd_witness(args):
    """Show the witness chain for audit trail."""
    print_header("Witness Chain (Audit Trail)")

    epoch = api(args.seed, "/api/v1/custody/epoch", args.token,
                ca_file=args.seed_ca_file)
    if "error" not in epoch:
        print(f"  Current epoch:  {epoch.get('epoch', '?')}")
        head = epoch.get("witness_head", "?")
        print(f"  Chain head:     {head[:16]}..." if len(head) > 16 else f"  Chain head:     {head}")

    chain = api(args.seed, "/api/v1/cognitive/status", args.token,
                ca_file=args.seed_ca_file)
    if "error" not in chain:
        cv = chain.get("chain_valid", {})
        print(f"  Chain valid:    {cv.get('valid', '?')}")
        print(f"  Chain length:   {cv.get('chain_length', '?')}")
        print(f"  Epoch range:    {cv.get('first_epoch', '?')} - {cv.get('last_epoch', '?')}")


def cmd_monitor(args):
    """Live monitor happiness vectors flowing into the Seed."""
    print_header("Live Happiness Monitor")
    print(f"  Polling every {args.interval}s (Ctrl+C to stop)")
    print()

    prev_epoch = 0
    prev_vectors = 0

    try:
        while True:
            s = api(args.seed, "/api/v1/status", args.token,
                    ca_file=args.seed_ca_file)
            if "error" in s:
                print(f"  [{time.strftime('%H:%M:%S')}] Error: {s['error']}")
                time.sleep(args.interval)
                continue

            epoch = s["epoch"]
            vectors = s["total_vectors"]
            new_v = vectors - prev_vectors if prev_vectors > 0 else 0
            new_e = epoch - prev_epoch if prev_epoch > 0 else 0

            d = api(args.seed, "/api/v1/sensor/drift/status", args.token,
                    ca_file=args.seed_ca_file)
            drift = d.get("current_drift_score", 0) if "error" not in d else 0
            drifting = d.get("drifting", False) if "error" not in d else False

            ts = time.strftime("%H:%M:%S")
            drift_str = f"  DRIFT!" if drifting else ""
            print(f"  [{ts}] epoch={epoch} vectors={vectors} (+{new_v}) "
                  f"drift={drift:.4f} chain={s['witness_chain_length']}{drift_str}")

            prev_epoch = epoch
            prev_vectors = vectors
            time.sleep(args.interval)
    except KeyboardInterrupt:
        print("\n  Stopped.")


def cmd_happiness_report(args):
    """Generate a happiness report from stored vectors."""
    print_header("Happiness Report")

    s = api(args.seed, "/api/v1/status", args.token, ca_file=args.seed_ca_file)
    if "error" in s:
        print(f"  Error: {s['error']}")
        return

    print(f"  Total vectors:  {s['total_vectors']}")
    print(f"  Store epoch:    {s['epoch']}")
    print()

    # Search for happiest and saddest vectors
    happy_ref = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.5]
    sad_ref = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5]

    print("  Happiest moments (closest to ideal happy):")
    happy = api(args.seed, "/api/v1/store/search", args.token,
                method="POST", data={"vector": happy_ref, "k": 3},
                ca_file=args.seed_ca_file)
    for n in happy.get("neighbors", happy.get("results", [])):
        dist = n.get("distance", n.get("dist", 0))
        vec = n.get("vector", n.get("values", []))
        score = vec[0] if vec else 0
        print(f"    id={n.get('id','?'):>10}  happiness={score:.2f}  dist={dist:.4f}")

    print()
    print("  Most stressed moments (closest to stressed reference):")
    sad = api(args.seed, "/api/v1/store/search", args.token,
              method="POST", data={"vector": sad_ref, "k": 3},
              ca_file=args.seed_ca_file)
    for n in sad.get("neighbors", sad.get("results", [])):
        dist = n.get("distance", n.get("dist", 0))
        vec = n.get("vector", n.get("values", []))
        score = vec[0] if vec else 0
        print(f"    id={n.get('id','?'):>10}  happiness={score:.2f}  dist={dist:.4f}")

    # Drift status
    print()
    d = api(args.seed, "/api/v1/sensor/drift/status", args.token,
            ca_file=args.seed_ca_file)
    if "error" not in d:
        if d.get("drifting"):
            print(f"  WARNING: Mood drift detected (score={d['current_drift_score']:.4f})")
            print(f"  This may indicate a change in guest satisfaction.")
        else:
            print(f"  Mood stable (drift score={d.get('current_drift_score', 0):.4f})")


def main():
    parser = argparse.ArgumentParser(
        description="Happiness Vector Query Tool for Cognitum Seed",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s --seed https://169.254.42.1:8443 status
  %(prog)s --seed https://10.1.10.236:8443 --token TOKEN search --mood happy
  %(prog)s --seed-ca-file /path/to/seed-ca.pem monitor
  %(prog)s --seed-ca-file /path/to/seed-ca.pem report
  %(prog)s --seed-ca-file /path/to/seed-ca.pem witness
"""
    )
    parser.add_argument("--seed", default="https://169.254.42.1:8443",
                        help="Seed HTTPS base URL (default: USB link-local on port 8443)")
    parser.add_argument("--seed-ca-file", default=None,
                        help="Trusted PEM certificate bundle (default: system trust store)")
    parser.add_argument("--token", default=None,
                        help="Bearer token for WiFi access (not needed for USB)")

    sub = parser.add_subparsers(dest="command")

    sub.add_parser("status", help="Show Seed and swarm status")
    sub.add_parser("witness", help="Show witness chain audit trail")

    p_search = sub.add_parser("search", help="kNN search for mood patterns")
    p_search.add_argument("--mood", default="happy",
                          choices=["happy", "neutral", "stressed"])
    p_search.add_argument("--k", type=int, default=5)

    p_monitor = sub.add_parser("monitor", help="Live monitor incoming vectors")
    p_monitor.add_argument("--interval", type=int, default=5)

    sub.add_parser("report", help="Generate happiness report")

    args = parser.parse_args()
    if not args.command:
        args.command = "status"

    cmds = {
        "status": cmd_status,
        "search": cmd_search,
        "witness": cmd_witness,
        "monitor": cmd_monitor,
        "report": cmd_happiness_report,
    }
    cmds[args.command](args)


if __name__ == "__main__":
    main()

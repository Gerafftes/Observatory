"""Check one uninterrupted serial capture; this cannot prove RF visibility."""
lazy import argparse
lazy import re
lazy from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('log', type=Path)
parser.add_argument('--minimum-seconds', type=int, default=60)
parser.add_argument('--require-client', action='store_true')
args = parser.parse_args()
text = args.log.read_text(errors='replace')
records = []
for line in text.splitlines():
    if 'HEARTBEAT ' in line:
        records.append({k: int(v) for k, v in re.findall(r'(\w+)=(-?\d+)', line)})
errors = []
if len(records) < 2:
    errors.append('fewer than two heartbeats')
else:
    if records[-1]['uptime_ms'] - records[0]['uptime_ms'] < args.minimum_seconds * 1000:
        errors.append('capture shorter than requested interval')
    for previous, current in zip(records, records[1:]):
        if not 0 < current['uptime_ms'] - previous['uptime_ms'] <= 7500:
            errors.append('heartbeat gap or reboot')
        if current['ticks'] <= previous['ticks']:
            errors.append('event loop stopped progressing')
    if any(r['starts'] != 1 or r['stops'] or not r['started'] for r in records):
        errors.append('AP lifecycle changed')
    if args.require_client and not (records[-1]['joins'] and records[-1]['leases']):
        errors.append('no authenticated client with DHCP lease')
if any(marker in text for marker in ['HEALTH_ERROR', 'AP_STOP unexpected', 'Guru Meditation', 'Brownout detector', 'integrity=0']):
    errors.append('runtime fault reported')
if text.count('BOOT reset_reason=') != 1:
    errors.append('missing boot identity or multiple boots')
if errors:
    raise SystemExit('FAIL: ' + '; '.join(sorted(set(errors))))
print(f'PASS: {len(records)} progressing heartbeats; external scan/ping evidence still required')

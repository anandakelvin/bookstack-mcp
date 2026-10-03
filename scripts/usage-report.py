#!/usr/bin/env python3
"""Totals from the BSMCP_USAGE_LOG file (fork-only).

usage-report.py [LOG] [--since YYYY-MM-DD] [--session PREFIX]
Prints totals per tool, per day and per session. Tokens = characters / 4.
"""
import argparse
import json
from collections import defaultdict
from datetime import datetime, timezone

ap = argparse.ArgumentParser()
ap.add_argument("log", nargs="?", default="/home/ubuntu/bsmcp/data/usage.jsonl")
ap.add_argument("--since", help="only calls on or after this day (UTC), YYYY-MM-DD")
ap.add_argument("--session", help="only sessions whose id starts with this")
a = ap.parse_args()

rows = []
with open(a.log) as f:
    for raw in f:
        try:
            r = json.loads(raw)
        except ValueError:
            continue
        r["day"] = datetime.fromtimestamp(r["ts"], timezone.utc).strftime("%Y-%m-%d")
        if a.since and r["day"] < a.since:
            continue
        if a.session and not r["session"].startswith(a.session):
            continue
        rows.append(r)


def table(title, key):
    groups = defaultdict(lambda: [0, 0, 0, 0])  # calls, chars, tokens, ms
    first = {}
    for r in rows:
        k = key(r)
        g = groups[k]
        g[0] += 1
        g[1] += r["chars"]
        g[2] += r["tokens"]
        g[3] += r["ms"]
        first.setdefault(k, r["ts"])
    print(f"\n{title}")
    print(f"{'':38} {'calls':>6} {'chars':>9} {'tokens':>8} {'avg ms':>7}")
    order = sorted(groups, key=lambda k: -groups[k][2]) if title == "Per tool" else sorted(groups, key=first.get)
    for k in order:
        c, ch, t, ms = groups[k]
        print(f"{k[:38]:38} {c:6} {ch:9} {t:8} {ms // c:7}")
    print(f"{'TOTAL':38} {sum(g[0] for g in groups.values()):6} "
          f"{sum(g[1] for g in groups.values()):9} {sum(g[2] for g in groups.values()):8}")


if not rows:
    print("no calls logged")
else:
    table("Per tool", lambda r: r["tool"])
    table("Per day (UTC)", lambda r: r["day"])
    start = {}
    for r in rows:
        start[r["session"]] = min(start.get(r["session"], r["ts"]), r["ts"])
    table("Per session (first call time UTC, id)",
          lambda r: datetime.fromtimestamp(start[r["session"]], timezone.utc)
          .strftime("%m-%d %H:%M ") + r["session"][:8])

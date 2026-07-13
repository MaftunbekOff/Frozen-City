#!/usr/bin/env python3
"""Read the Frozen City playtest telemetry JSONL and print a human report.

The server writes one JSON object per line to `FC_TELEMETRY_PATH` (see
`crates/fc-net/src/telemetry.rs`): a `session_start` when a player joins and a
`session_end` — carrying a progress snapshot — when they leave. From just those
two events this derives the numbers that actually tell you whether anyone plays
and whether the game is fun enough to keep them:

  * DAU / concurrency  — distinct players per day, peak simultaneous players;
  * session length     — how long a real sitting lasts;
  * drop-off day       — the in-game DAY players quit on (for a survival game,
                         the single most useful retention signal);
  * the vision funnel  — how many accounts reach the Tunnel and graduate to the
                         Global World.

Stdlib only (the server already runs Python for the registration bot). Usage:

    python3 bot/analyze_telemetry.py [path] [--since YYYY-MM-DD]

`path` defaults to /var/lib/frozen-city/telemetry.jsonl.
"""

import argparse
import json
import sys
from collections import defaultdict
from datetime import datetime, timezone

DEFAULT_PATH = "/var/lib/frozen-city/telemetry.jsonl"


def day_of(ts):
    """UTC calendar day (YYYY-MM-DD) for a unix-seconds timestamp."""
    return datetime.fromtimestamp(ts, timezone.utc).strftime("%Y-%m-%d")


def load(path, since):
    """Parse the JSONL, skipping blank/corrupt lines (a truncated tail line
    after a hard kill shouldn't sink the whole report)."""
    events = []
    bad = 0
    try:
        with open(path, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    ev = json.loads(line)
                except json.JSONDecodeError:
                    bad += 1
                    continue
                if since and "ts" in ev and day_of(ev["ts"]) < since:
                    continue
                events.append(ev)
    except FileNotFoundError:
        sys.exit(f"No telemetry file at {path}\n"
                 f"(set FC_TELEMETRY_PATH on the server and let some sessions run first.)")
    return events, bad


def pct(sorted_vals, p):
    """Nearest-rank percentile of an already-sorted list."""
    if not sorted_vals:
        return 0
    k = max(0, min(len(sorted_vals) - 1, round((p / 100.0) * (len(sorted_vals) - 1))))
    return sorted_vals[k]


def bar(n, width_unit=1):
    return "#" * min(60, n // width_unit)


def peak_concurrency(events):
    """Walk the merged start/end timeline; +1 on a start, -1 on an end. Returns
    (overall peak, {day: peak}). Clamps at 0 so an end with no matching start
    (its start predates `--since`) can't drive the counter negative."""
    timeline = []
    for ev in events:
        if ev["event"] == "session_start":
            timeline.append((ev["ts"], 1))
        elif ev["event"] == "session_end":
            timeline.append((ev["ts"], -1))
    # Ends before starts at the same second, so concurrency never over-counts a
    # hand-off where one player leaves exactly as another joins.
    timeline.sort(key=lambda t: (t[0], t[1]))
    cur = peak = 0
    day_peak = defaultdict(int)
    for ts, delta in timeline:
        cur = max(0, cur + delta)
        peak = max(peak, cur)
        d = day_of(ts)
        day_peak[d] = max(day_peak[d], cur)
    return peak, day_peak


def main():
    ap = argparse.ArgumentParser(description="Summarize Frozen City playtest telemetry.")
    ap.add_argument("path", nargs="?", default=DEFAULT_PATH, help="telemetry JSONL file")
    ap.add_argument("--since", metavar="YYYY-MM-DD", help="ignore events before this UTC day")
    args = ap.parse_args()

    events, bad = load(args.path, args.since)
    starts = [e for e in events if e.get("event") == "session_start"]
    ends = [e for e in events if e.get("event") == "session_end"]

    print("=" * 66)
    print(" FROZEN CITY - PLAYTEST TELEMETRY")
    print("=" * 66)
    if not events:
        print("\nNo events yet. Nobody has played a recorded session.\n")
        return
    span = f"{day_of(min(e['ts'] for e in events))} .. {day_of(max(e['ts'] for e in events))}"
    print(f" file           {args.path}")
    print(f" span (UTC)     {span}")
    print(f" sessions       {len(starts)} started, {len(ends)} ended"
          + (f"   ({len(starts) - len(ends)} still open / lost to hard kill)"
             if len(starts) != len(ends) else ""))
    if bad:
        print(f" skipped        {bad} unparseable line(s)")

    # --- Reach: distinct accounts vs guest sessions -------------------------
    accounts = {e["account"] for e in starts if e.get("account") is not None}
    guest_sessions = sum(1 for e in starts if e.get("account") is None
                         and not e.get("reconnect"))
    fresh_starts = [e for e in starts if not e.get("reconnect")]
    print(f" reach          {len(accounts)} distinct account(s), "
          f"{guest_sessions} guest sitting(s)")
    print(f" fresh sittings {len(fresh_starts)}  (reconnects excluded)")

    peak, day_peak = peak_concurrency(events)
    print(f" peak online    {peak} simultaneous")

    # --- Daily activity -----------------------------------------------------
    print("\n--- DAILY ACTIVITY (UTC) " + "-" * 41)
    print(f"{'day':<12}{'sittings':>9}{'accounts':>10}{'guests':>8}{'peak':>6}   DAU")
    by_day_starts = defaultdict(list)
    for e in fresh_starts:
        by_day_starts[day_of(e["ts"])].append(e)
    for d in sorted(by_day_starts):
        rows = by_day_starts[d]
        acc = len({e["account"] for e in rows if e.get("account") is not None})
        gue = sum(1 for e in rows if e.get("account") is None)
        dau = acc + gue
        print(f"{d:<12}{len(rows):>9}{acc:>10}{gue:>8}{day_peak[d]:>6}   {bar(dau)}")

    # --- Session length -----------------------------------------------------
    print("\n--- SESSION LENGTH (minutes) " + "-" * 37)
    by_world = defaultdict(list)
    for e in ends:
        by_world[e.get("world", "?")].append(e.get("duration_s", 0))
    all_durs = sorted(d for ds in by_world.values() for d in ds)
    print(f"{'world':<15}{'n':>5}{'p50':>7}{'p90':>7}{'p99':>7}{'max':>7}")
    for world in sorted(by_world):
        ds = sorted(by_world[world])
        print(f"{world:<15}{len(ds):>5}"
              f"{pct(ds, 50) / 60:>7.1f}{pct(ds, 90) / 60:>7.1f}"
              f"{pct(ds, 99) / 60:>7.1f}{(ds[-1] if ds else 0) / 60:>7.1f}")
    if all_durs:
        print(f"{'ALL':<15}{len(all_durs):>5}"
              f"{pct(all_durs, 50) / 60:>7.1f}{pct(all_durs, 90) / 60:>7.1f}"
              f"{pct(all_durs, 99) / 60:>7.1f}{all_durs[-1] / 60:>7.1f}")

    # --- Drop-off day -------------------------------------------------------
    # In a personal/central world `day` is that player's own progress; in the
    # shared guest world it's the world clock (many players share it), so it's
    # reported separately and read with a grain of salt.
    print("\n--- DROP-OFF: in-game DAY at leave " + "-" * 31)
    for scope, label in (("personal", "personal worlds (per-player)"),
                         ("central", "central / Global World"),
                         ("shared_guest", "shared guest world (shared clock)")):
        rows = [e for e in ends if e.get("world") == scope]
        if not rows:
            continue
        print(f"  {label}:  n={len(rows)}")
        hist = defaultdict(int)
        for e in rows:
            hist[e.get("day", 0)] += 1
        for day in sorted(hist):
            print(f"    day {day:>2}  {hist[day]:>4}  {bar(hist[day])}")

    # --- The vision funnel --------------------------------------------------
    # Per account, the furthest state ever snapshotted at a session_end. Guests
    # have no cross-session identity, so they're counted as sittings only.
    print("\n--- VISION FUNNEL (by account) " + "-" * 35)
    reached = defaultdict(lambda: {"played": False, "built": False,
                                   "tunnel": False, "graduated": False,
                                   "central": False, "max_day": 0})
    for e in ends:
        acc = e.get("account")
        if acc is None:
            continue
        r = reached[acc]
        r["played"] = True
        if e.get("buildings", 0) > 1:  # more than the pre-placed furnace
            r["built"] = True
        if e.get("tunnel_stage", 0) > 0:
            r["tunnel"] = True
        if e.get("graduated"):
            r["graduated"] = True
        if e.get("world") == "central":
            r["central"] = True
        r["max_day"] = max(r["max_day"], e.get("day", 0))
    total_acc = len(reached)
    if total_acc == 0:
        print("  (no account sessions yet — only guests)")
    else:
        def stage(name, key):
            n = sum(1 for r in reached.values() if r[key])
            share = 100 * n / total_acc
            print(f"  {name:<26}{n:>4} / {total_acc:<4} {share:5.0f}%  {bar(n)}")
        stage("played (account)", "played")
        stage("placed a building", "built")
        stage("started the Tunnel", "tunnel")
        stage("graduated", "graduated")
        stage("entered Global World", "central")

    print("\n" + "=" * 66)
    print(" Read the drop-off histogram first: the day players quit on is where")
    print(" the fun runs out. Fix that before building anything new.")
    print("=" * 66)


if __name__ == "__main__":
    main()

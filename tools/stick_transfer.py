#!/usr/bin/env python3
"""Plot what the phone's stick actually sends against how far the thumb moved.

Pairs a swept touch displacement on the phone with the axis values the PC
accepted, so stick tuning is measured rather than argued about. Reads the CSV
written by `pp-headless --trace-csv`.

    python tools/stick_transfer.py tools/traces/before.csv --axis lx

Reports the transfer curve as a table: for each 5% of thumb travel, the axis
value that reached the PC. Two things make a touchscreen stick feel wrong and
both are visible here — a flat run at the start (a dead band) and a step
between adjacent rows (a cliff).
"""

import argparse
import csv
import sys

FULL = 32767


def load(path, axis):
    rows = []
    with open(path, newline="", encoding="utf-8") as fh:
        for r in csv.DictReader(fh):
            rows.append((int(r["t_us"]), int(r[axis])))
    if not rows:
        sys.exit(f"{path}: no rows")
    return rows


def sweep(rows):
    """Isolate the monotonic ramp: from the last zero before the peak, to the peak."""
    peak_i = max(range(len(rows)), key=lambda i: abs(rows[i][1]))
    start = peak_i
    while start > 0 and rows[start - 1][1] != 0:
        start -= 1
    return rows[start : peak_i + 1]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("csv")
    ap.add_argument("--axis", default="lx", choices=["lx", "ly", "rx", "ry", "lt", "rt"])
    ap.add_argument("--buckets", type=int, default=20)
    args = ap.parse_args()

    ramp = sweep(load(args.csv, args.axis))
    if len(ramp) < 4:
        sys.exit("not enough samples in the ramp — was the swipe captured?")

    t0, t1 = ramp[0][0], ramp[-1][0]
    span = max(t1 - t0, 1)
    scale = 255 if args.axis in ("lt", "rt") else FULL

    print(f"{args.axis}: {len(ramp)} samples over {span / 1000:.0f} ms\n")
    print(f"{'travel':>7}  {'output':>7}  {'%':>6}  curve")
    print("-" * 54)

    prev = None
    dead = 0.0
    biggest_step = 0.0
    for b in range(args.buckets + 1):
        frac = b / args.buckets
        at = t0 + span * frac
        val = min(ramp, key=lambda r: abs(r[0] - at))[1]
        pct = abs(val) / scale
        if prev is None and pct == 0:
            dead = frac
        if prev is not None:
            biggest_step = max(biggest_step, abs(pct - prev))
        bar = "#" * round(pct * 30)
        print(f"{frac * 100:6.0f}%  {val:7d}  {pct * 100:5.1f}%  {bar}")
        prev = pct

    print("-" * 54)
    print(f"dead band      {dead * 100:.0f}% of travel   (want: ~0%)")
    print(f"largest step   {biggest_step * 100:.1f}% per {100 / args.buckets:.0f}% of travel"
          f"   (want: even, no cliff)")


if __name__ == "__main__":
    main()

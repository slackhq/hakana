#!/usr/bin/env python3
"""Compare retained-size snapshots, optionally breaking down one Rust type."""

import argparse
import json
from collections import defaultdict
from pathlib import Path


def load(path):
    snapshot = json.loads(Path(path).read_text())
    if snapshot["accounting_warnings"]:
        raise ValueError(f"{path}: {snapshot['accounting_warnings']}")
    total = sum(row["self_bytes"] for row in snapshot["allocations"])
    if total != snapshot["estimated_retained_bytes"]:
        raise ValueError(f"{path}: allocation contributions do not match total")
    return snapshot


def grouped(snapshot, selected_type):
    sizes = defaultdict(int)
    for row in snapshot["allocations"]:
        parts = row["path"].split(";")
        if selected_type:
            # Use the first occurrence so recursive types never double-count
            # the same allocation within this breakdown.
            match = next(
                (i for i, part in enumerate(parts) if part.rsplit("::", 1)[-1] == selected_type),
                None,
            )
            if match is None:
                continue
            field = parts[match + 1] if match + 1 < len(parts) else "<inline/padding>"
            key = f"{selected_type}.{field}"
        elif parts[0] in ("codebase", "symbol_references"):
            field = parts[2] if len(parts) > 2 else "<inline/padding>"
            key = f"{parts[0]}.{field}"
        elif parts[0] == "ArcInner":
            key = "shared_allocations"
        else:
            key = parts[0]
        sizes[key] += row["self_bytes"]
    return sizes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before")
    parser.add_argument("after")
    parser.add_argument("--type", dest="selected_type", help="e.g. ClassLikeInfo or TUnion")
    parser.add_argument("--limit", type=int, default=25)
    args = parser.parse_args()
    before, after = load(args.before), load(args.after)
    left, right = grouped(before, args.selected_type), grouped(after, args.selected_type)
    keys = sorted(
        left.keys() | right.keys(),
        key=lambda k: (-abs(right[k] - left[k]), -right[k], -left[k], k),
    )
    width = max([len(k) for k in keys[:args.limit]] + [9])
    print(f"{'Structure':<{width}} {'Before MiB':>12} {'After MiB':>12} {'Delta MiB':>12}")
    for key in keys[:args.limit]:
        a, b = left[key] / 2**20, right[key] / 2**20
        print(f"{key:<{width}} {a:12.2f} {b:12.2f} {b-a:+12.2f}")
    a, b = sum(left.values()) / 2**20, sum(right.values()) / 2**20
    print(f"{'TOTAL':<{width}} {a:12.2f} {b:12.2f} {b-a:+12.2f}")
    print("\nEstimates of retained objects; not RSS or peak allocation.")
    if args.selected_type:
        print("Shared payloads appear separately from referring fields; parent-container spare capacity is outside this type breakdown.")


if __name__ == "__main__":
    main()

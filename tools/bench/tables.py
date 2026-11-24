#!/usr/bin/env python3
"""Render the TSV written by tools/bench/bench.sh as Markdown tables.

usage: python3 -I tools/bench/tables.py results.tsv
One table per metric (time, peak memory) and operation (cutsets, prob, mc),
with the C engine, ftar, and the ratio ftar / C.
"""

import sys

OPERATIONS = (
    ("cutsets", "Cut sets"),
    ("prob", "Probability (cut sets + probability analysis)"),
    ("mc", "Monte Carlo (NOT comparable, see text)"),
)
METRICS = (("time", "wall time (median)"), ("mem", "peak resident memory (median)"))


def fmt_s(v):
    if v < 0.01:
        return f"{v * 1000:.1f} ms"
    if v < 1:
        return f"{v * 1000:.0f} ms"
    return f"{v:.2f} s"


def fmt_mb(kb):
    return f"{kb / 1024.0:.1f} MB"


def cell(row, metric):
    if row is None:
        return "-", None
    if row["status"] != "ok":
        return row["status"], None
    if row["seconds"] == "":
        return "-", None
    if metric == "time":
        v = float(row["seconds"])
        return fmt_s(v), v
    v = float(row["max_rss_kb"])
    return fmt_mb(v), v


def read(path):
    """The result rows, the cut set counts per tree (C, ftar), and the trees in file order."""
    rows, cuts, order = [], {}, []
    head = None
    with open(path) as tsv:
        for line in tsv:
            f = line.rstrip("\n").split("\t")
            if f[0] == "tree":
                head = f
            elif f[0] == "#cutcount":
                cuts[f[1]] = (f[2], f[3])
            elif head and len(f) == len(head):
                r = dict(zip(head, f, strict=True))
                rows.append(r)
                if r["tree"] not in order:
                    order.append(r["tree"])
    return rows, cuts, order


def ratio(cv, rv):
    if not (cv and rv):
        return "-"
    return "<0.01" if rv / cv < 0.005 else f"{rv / cv:.2f}"


def cut_count(cuts, tree):
    """The cut set count, or MISMATCH when the engines disagree, or - when neither has one."""
    cc, fc = cuts.get(tree, ("", ""))
    known = [x for x in (cc, fc) if x not in ("", "-")]
    if not known:
        return "-"
    if len(set(known)) == 1:
        return known[0]
    return f"MISMATCH C {cc} / ftar {fc}"


def print_table(operation, metric, trees, results):
    (op, title), (metric, mtitle) = operation, metric
    idx, info, cuts = results
    print(f"### {title}: {mtitle}\n")
    extra = "| cut sets " if op == "cutsets" else ""
    print(f"| tree | kind | basic events {extra}| OpenFTA C | ftar | ftar / C |")
    print("|---|---|---:{}|---:|---:|---:|".format("|---:" if op == "cutsets" else ""))
    for t in trees:
        c, cv = cell(idx.get((t, "c", op)), metric)
        r, rv = cell(idx.get((t, "ftar", op)), metric)
        cs = f"| {cut_count(cuts, t)} " if op == "cutsets" else ""
        kind, events = info[t]["kind"], info[t]["events"]
        print(f"| {t} | {kind} | {events} {cs}| {c} | {r} | {ratio(cv, rv)} |")
    print()


def main():
    rows, cuts, order = read(sys.argv[1])
    idx = {(r["tree"], r["engine"], r["op"]): r for r in rows}
    info = {r["tree"]: r for r in rows}
    for operation in OPERATIONS:
        trees = [t for t in order if (t, "c", operation[0]) in idx]
        if not trees:
            continue
        for metric in METRICS:
            print_table(operation, metric, trees, (idx, info, cuts))


main()

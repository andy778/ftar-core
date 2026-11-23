#!/usr/bin/env python3
"""Render the TSV written by tools/bench/bench.sh as Markdown tables.

usage: python3 -I tools/bench/tables.py results.tsv
One table per metric (time, peak memory) and operation (cutsets, prob, mc),
with the C engine, ftar, and the ratio ftar / C.
"""
import sys


def fmt_s(v):
    if v < 0.01:
        return "%.1f ms" % (v * 1000)
    if v < 1:
        return "%.0f ms" % (v * 1000)
    return "%.2f s" % v


def fmt_mb(kb):
    return "%.1f MB" % (kb / 1024.0)


def cell(row, metric):
    if row is None:
        return "-", None
    if row["status"] != "ok":
        return row["status"], None
    if row["seconds"] == "":
        return "-", None
    v = float(row["seconds"]) if metric == "time" else float(row["max_rss_kb"])
    return (fmt_s(v) if metric == "time" else fmt_mb(v)), v


def main():
    path = sys.argv[1]
    rows, cuts, order = [], {}, []
    head = None
    for line in open(path):
        f = line.rstrip("\n").split("\t")
        if f[0] == "tree":
            head = f
        elif f[0] == "#cutcount":
            cuts[f[1]] = (f[2], f[3])
        elif head and len(f) == len(head):
            r = dict(zip(head, f))
            rows.append(r)
            if r["tree"] not in order:
                order.append(r["tree"])
    idx = {(r["tree"], r["engine"], r["op"]): r for r in rows}
    info = {r["tree"]: r for r in rows}
    for op, title in (("cutsets", "Cut sets"), ("prob", "Probability (cut sets + probability analysis)"), ("mc", "Monte Carlo (NOT comparable, see text)")):
        trees = [t for t in order if (t, "c", op) in idx]
        if not trees:
            continue
        for metric, mtitle in (("time", "wall time (median)"), ("mem", "peak resident memory (median)")):
            print("### %s: %s\n" % (title, mtitle))
            extra = "| cut sets " if op == "cutsets" else ""
            print("| tree | kind | basic events %s| OpenFTA C | ftar | ftar / C |" % extra)
            print("|---|---|---:%s|---:|---:|---:|" % ("|---:" if op == "cutsets" else ""))
            for t in trees:
                c, cv = cell(idx.get((t, "c", op)), metric)
                r, rv = cell(idx.get((t, "ftar", op)), metric)
                ratio = ("<0.01" if rv / cv < 0.005 else "%.2f" % (rv / cv)) if cv and rv else "-"
                cs = ""
                if op == "cutsets":
                    cc, fc = cuts.get(t, ("", ""))
                    known = [x for x in (cc, fc) if x not in ("", "-")]
                    cs = "| %s " % ("-" if not known else known[0] if len(set(known)) == 1 else "MISMATCH C %s / ftar %s" % (cc, fc))
                print("| %s | %s | %s %s| %s | %s | %s |" % (t, info[t]["kind"], info[t]["events"], cs, c, r, ratio))
            print()


main()

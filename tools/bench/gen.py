#!/usr/bin/env python3
"""Generate a synthetic fault tree (.fta + .ped) that OpenFTA and ftar both read.

usage: python3 -I tools/bench/gen.py FAMILY PARAM OUTDIR [SEED]

Families (all files have LF line ends, which OpenFTA on Linux needs):
  andor K    AND of K OR gates of two events each: 2K events, 2^K cut sets of order K
  orand K    OR of K AND gates of three events each: 3K events, K cut sets of order 3
  rand N     random tree over N basic events with shared events, fan-in 2..4,
             deterministic from SEED (default 1); cut set count not known up front
Writes OUTDIR/<family><PARAM>.fta and .ped and prints the tree file path.
Every gate sits under an intermediate event ("M"), as OpenFTA requires.
"""
import os
import random
import sys


def emit_tree(name, top_gate):
    """gate: (kind, [child]); child: ('B', event name) | ('G', gate)."""
    out = [name + ".ped", "S NULL 0", "0", ""]

    def gate(g):
        kind, kids = g
        out.append("M NULL 1 ")
        out.append("0")
        out.append("%s NULL %d " % (kind, len(kids)))
        for k in kids:
            if k[0] == "B":
                out.append("B %s 0 " % k[1])
            else:
                gate(k[1])

    gate(top_gate)
    return "\n".join(out) + "\n"


def emit_ped(events):
    # Same record layout as tests/data/*.ped: name;;B ;description;probability; ;
    return "".join(
        "%s;;B ;Bench event;%.6e; ;\n" % (e, 1e-3 * (1 + i % 7)) for i, e in enumerate(events)
    )


def andor(k):
    ev = ["E%d" % i for i in range(2 * k)]
    kids = [("G", ("O", [("B", ev[2 * i]), ("B", ev[2 * i + 1])])) for i in range(k)]
    return ("A", kids), ev


def orand(k):
    ev = ["E%d" % i for i in range(3 * k)]
    kids = [("G", ("A", [("B", ev[3 * i + j]) for j in range(3)])) for i in range(k)]
    return ("O", kids), ev


def rand(n, seed):
    rng = random.Random(seed)
    ev = ["E%d" % i for i in range(n)]
    used = []
    budget = [n]

    def build(depth, kind):
        kids = []
        for _ in range(rng.randint(2, 4)):
            if depth < 6 and budget[0] > 3 and rng.random() < 0.45:
                budget[0] -= 2
                kids.append(("G", build(depth + 1, "O" if kind == "A" else "A")))
            else:
                # sharing: mostly a fresh event, sometimes one already used
                if used and rng.random() < 0.25:
                    e = rng.choice(used)
                else:
                    e = ev[rng.randrange(n)]
                used.append(e)
                kids.append(("B", e))
        return (kind, kids)

    tops = []
    while budget[0] > 3:
        budget[0] -= 2
        tops.append(("G", build(1, "A")))
    return ("O", tops), ev


def main():
    if len(sys.argv) < 4:
        sys.exit(__doc__)
    fam, param, outdir = sys.argv[1], int(sys.argv[2]), sys.argv[3]
    seed = int(sys.argv[4]) if len(sys.argv) > 4 else 1
    if fam == "andor":
        top, ev = andor(param)
    elif fam == "orand":
        top, ev = orand(param)
    elif fam == "rand":
        top, ev = rand(param, seed)
    else:
        sys.exit("unknown family " + fam)
    name = "%s%d" % (fam, param)
    os.makedirs(outdir, exist_ok=True)
    with open(os.path.join(outdir, name + ".fta"), "w", newline="\n") as f:
        f.write(emit_tree(name, top))
    with open(os.path.join(outdir, name + ".ped"), "w", newline="\n") as f:
        f.write(emit_ped(ev))
    print(os.path.join(outdir, name + ".fta"))


main()

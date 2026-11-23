# Benchmarks: ftar engine vs OpenFTA's original C engine

Speed and peak memory of ftar's analysis engine compared with OpenFTA's C analysis core (built headless by
`tools/openfta-native/build.sh`). Everything below was measured on one machine by `tools/bench/bench.sh`; nothing is
extrapolated. The numbers are for these trees and this machine only.

## Setup

| | |
|---|---|
| CPU | AMD Ryzen 5 5600X (6 cores / 12 threads) |
| RAM | 31 GiB |
| OS | Manjaro Linux, kernel 7.2.9 |
| Compilers | rustc 1.99.0, gcc 16.2.1 |
| ftar | commit `8ca08ca` (master), `cargo build --release -p ftar-cli`, run as `ftar cutsets` / `ftar prob` |
| OpenFTA C | commit `fa753a9` of andy778/OpenFTA (pinned in `build.sh`), built by `build.sh` with its default `-O` (gcc `-std=gnu89 -O -fcommon`), run through the headless driver `openfta-run` |

The machine was not idle: other sessions were running on it, so absolute times carry noise. The same `orand1000` /
`andor13` cases measured a few minutes apart differed by up to about 1.5x for the C engine. Treat ratios within about
1.3x as noise, and rows under about 5 ms as process start-up.

## Method

* Each case runs `RUNS=5` times and the median is reported. If the first run takes more than 10 s it is not repeated
  (that row then rests on one run). `TIMEOUT=60` seconds kills a run; it is recorded as TIMEOUT. `/usr/bin/time -v`
  gives `Maximum resident set size`; wall time is read from the clock around the same command (GNU time has only 10 ms
  resolution). Output goes to a file, so printing cost is included on both sides. The C driver also prints debug lines
  while it loads the `.ped`; that cost is part of the C numbers.
* **Cut sets**: `openfta-run TREE cutsets ORDER` vs `ftar cutsets TREE`, all orders.
* **Probability**: `openfta-run TREE prob ORDER 1 2` vs `ftar prob TREE --terms 2 --time 1`. Both generate the cut
  sets first and then run the probability analysis (the C driver runs the cut sets analysis and reads `TREE.mcs` back,
  as the GUI does), so this metric includes the cut set time. Two inclusion-exclusion terms are used. With four terms
  (as `generate.sh` uses) the cost grows roughly with cut sets^4: one spot check, not in the tables, had `andor8`
  (256 cut sets) at 31 s for C and 1.8 s for ftar.
* **Trees**: the five example trees (`crates/core/tests/data`, LF-converted copies, because OpenFTA on Linux cannot read
  a CRLF `.ped`) and generated trees from `tools/bench/gen.py`. `andorK` is an AND of K two-event OR gates (2^K cut sets
  of order K). `orandK` is an OR of K three-event AND gates (K cut sets of order 3, many events). `randN` is a seeded
  random AND/OR tree with shared events and nesting. Generated trees are produced on demand in a temp dir and are not
  committed.
* **Correctness check**: the number of cut sets reported by the two engines was compared on every tree where both
  finished. They agree on all of them; no mismatch was found. This is a count comparison only (set equality for the
  examples is covered by `docs/VERIFICATION.md`).
* "basic events" counts the `B` lines of the `.fta` (event occurrences), so it is 0 for the transfer-chain examples
  `top` and `middle`.

## Results

#### Cut sets: wall time (median)

| tree | kind | basic events | cut sets | OpenFTA C | ftar | ftar / C |
|---|---|---:|---:|---:|---:|---:|
| theatre | example | 3 | 2 | 2.5 ms | 2.7 ms | 1.08 |
| lift | example | 21 | 12 | 2.7 ms | 2.8 ms | 1.04 |
| top | example | 0 | 2 | 2.5 ms | 2.5 ms | 1.00 |
| middle | example | 0 | 2 | 2.3 ms | 2.4 ms | 1.04 |
| bottom | example | 2 | 2 | 2.2 ms | 2.5 ms | 1.14 |
| orand100 | generated | 300 | 100 | 11 ms | 3.2 ms | 0.29 |
| orand1000 | generated | 3000 | 1000 | 4.64 s | 18 ms | <0.01 |
| orand5000 | generated | 15000 | 5000 | TIMEOUT | 281 ms | - |
| orand20000 | generated | 60000 | 20000 | TIMEOUT | 4.06 s | - |
| andor6 | generated | 12 | 64 | 3.0 ms | 3.1 ms | 1.03 |
| andor8 | generated | 16 | 256 | 7.9 ms | 5.9 ms | 0.75 |
| andor10 | generated | 20 | 1024 | 83 ms | 35 ms | 0.42 |
| andor11 | generated | 22 | 2048 | 341 ms | 141 ms | 0.41 |
| andor12 | generated | 24 | 4096 | 1.40 s | 606 ms | 0.43 |
| andor13 | generated | 26 | 8192 | 6.03 s | 2.44 s | 0.41 |
| andor14 | generated | 28 | 16384 | 25.37 s | 9.26 s | 0.36 |
| andor16 | generated | 32 | - | TIMEOUT | TIMEOUT | - |
| andor18 | generated | 36 | - | TIMEOUT | TIMEOUT | - |
| rand60 | generated | 61 | 65 | 4.2 ms | 3.3 ms | 0.79 |
| rand120 | generated | 113 | 2783 | 2.89 s | 806 ms | 0.28 |
| rand200 | generated | 211 | 503 | 2.26 s | 277 ms | 0.12 |
| rand300 | generated | 304 | 2548 | 3.09 s | 317 ms | 0.10 |
| rand500 | generated | 533 | - | TIMEOUT | TIMEOUT | - |
| rand1000 | generated | 1058 | - | TIMEOUT | TIMEOUT | - |

#### Cut sets: peak resident memory (median)

| tree | kind | basic events | cut sets | OpenFTA C | ftar | ftar / C |
|---|---|---:|---:|---:|---:|---:|
| theatre | example | 3 | 2 | 2.5 MB | 3.5 MB | 1.40 |
| lift | example | 21 | 12 | 2.6 MB | 3.6 MB | 1.41 |
| top | example | 0 | 2 | 2.6 MB | 3.7 MB | 1.41 |
| middle | example | 0 | 2 | 2.6 MB | 3.6 MB | 1.40 |
| bottom | example | 2 | 2 | 2.6 MB | 3.6 MB | 1.40 |
| orand100 | generated | 300 | 100 | 2.7 MB | 3.9 MB | 1.46 |
| orand1000 | generated | 3000 | 1000 | 6.5 MB | 6.0 MB | 0.93 |
| orand5000 | generated | 15000 | 5000 | TIMEOUT | 20.1 MB | - |
| orand20000 | generated | 60000 | 20000 | TIMEOUT | 70.6 MB | - |
| andor6 | generated | 12 | 64 | 2.6 MB | 3.6 MB | 1.38 |
| andor8 | generated | 16 | 256 | 2.5 MB | 3.6 MB | 1.44 |
| andor10 | generated | 20 | 1024 | 2.7 MB | 4.4 MB | 1.61 |
| andor11 | generated | 22 | 2048 | 2.8 MB | 5.6 MB | 2.01 |
| andor12 | generated | 24 | 4096 | 3.3 MB | 10.3 MB | 3.13 |
| andor13 | generated | 26 | 8192 | 4.4 MB | 20.1 MB | 4.57 |
| andor14 | generated | 28 | 16384 | 4.8 MB | 37.3 MB | 7.79 |
| andor16 | generated | 32 | - | TIMEOUT | TIMEOUT | - |
| andor18 | generated | 36 | - | TIMEOUT | TIMEOUT | - |
| rand60 | generated | 61 | 65 | 2.5 MB | 3.7 MB | 1.47 |
| rand120 | generated | 113 | 2783 | 3.2 MB | 8.0 MB | 2.48 |
| rand200 | generated | 211 | 503 | 2.8 MB | 6.4 MB | 2.29 |
| rand300 | generated | 304 | 2548 | 3.2 MB | 6.2 MB | 1.92 |
| rand500 | generated | 533 | - | TIMEOUT | TIMEOUT | - |
| rand1000 | generated | 1058 | - | TIMEOUT | TIMEOUT | - |

#### Probability (cut sets + probability analysis): wall time (median)

| tree | kind | basic events | OpenFTA C | ftar | ftar / C |
|---|---|---:|---:|---:|---:|
| theatre | example | 3 | 2.5 ms | 2.7 ms | 1.08 |
| lift | example | 21 | 2.7 ms | 2.8 ms | 1.04 |
| top | example | 0 | 2.3 ms | 2.5 ms | 1.09 |
| middle | example | 0 | 2.2 ms | 2.5 ms | 1.14 |
| bottom | example | 2 | 2.3 ms | 2.5 ms | 1.09 |
| orand100 | generated | 300 | 18 ms | 4.4 ms | 0.24 |
| orand1000 | generated | 3000 | 9.35 s | 38 ms | <0.01 |
| orand5000 | generated | 15000 | TIMEOUT | 625 ms | - |
| orand20000 | generated | 60000 | TIMEOUT | 10.79 s | - |
| andor6 | generated | 12 | 3.4 ms | 3.2 ms | 0.94 |
| andor8 | generated | 16 | 13 ms | 7.2 ms | 0.55 |
| andor10 | generated | 20 | 168 ms | 52 ms | 0.31 |
| andor11 | generated | 22 | 684 ms | 201 ms | 0.29 |
| andor12 | generated | 24 | 2.85 s | 909 ms | 0.32 |
| andor13 | generated | 26 | 12.47 s | 3.46 s | 0.28 |
| andor14 | generated | 28 | 51.29 s | 14.15 s | 0.28 |
| andor16 | generated | 32 | TIMEOUT | TIMEOUT | - |
| andor18 | generated | 36 | TIMEOUT | TIMEOUT | - |
| rand60 | generated | 61 | 5.1 ms | 3.6 ms | 0.71 |
| rand120 | generated | 113 | 4.18 s | 956 ms | 0.23 |
| rand200 | generated | 211 | 2.34 s | 277 ms | 0.12 |
| rand300 | generated | 304 | 5.15 s | 374 ms | 0.07 |
| rand500 | generated | 533 | TIMEOUT | TIMEOUT | - |
| rand1000 | generated | 1058 | TIMEOUT | TIMEOUT | - |

#### Probability (cut sets + probability analysis): peak resident memory (median)

| tree | kind | basic events | OpenFTA C | ftar | ftar / C |
|---|---|---:|---:|---:|---:|
| theatre | example | 3 | 2.7 MB | 3.8 MB | 1.40 |
| lift | example | 21 | 2.6 MB | 3.7 MB | 1.43 |
| top | example | 0 | 2.6 MB | 3.7 MB | 1.43 |
| middle | example | 0 | 2.6 MB | 3.6 MB | 1.41 |
| bottom | example | 2 | 2.6 MB | 3.6 MB | 1.42 |
| orand100 | generated | 300 | 2.7 MB | 3.9 MB | 1.44 |
| orand1000 | generated | 3000 | 6.6 MB | 6.2 MB | 0.93 |
| orand5000 | generated | 15000 | TIMEOUT | 20.1 MB | - |
| orand20000 | generated | 60000 | TIMEOUT | 70.4 MB | - |
| andor6 | generated | 12 | 2.6 MB | 3.7 MB | 1.43 |
| andor8 | generated | 16 | 2.6 MB | 3.8 MB | 1.46 |
| andor10 | generated | 20 | 2.7 MB | 4.9 MB | 1.80 |
| andor11 | generated | 22 | 2.9 MB | 6.5 MB | 2.21 |
| andor12 | generated | 24 | 3.4 MB | 14.9 MB | 4.38 |
| andor13 | generated | 26 | 4.2 MB | 26.6 MB | 6.36 |
| andor14 | generated | 28 | 6.3 MB | 50.9 MB | 8.06 |
| andor16 | generated | 32 | TIMEOUT | TIMEOUT | - |
| andor18 | generated | 36 | TIMEOUT | TIMEOUT | - |
| rand60 | generated | 61 | 2.5 MB | 3.8 MB | 1.51 |
| rand120 | generated | 113 | 3.2 MB | 9.2 MB | 2.84 |
| rand200 | generated | 211 | 2.8 MB | 6.2 MB | 2.25 |
| rand300 | generated | 304 | 3.1 MB | 7.1 MB | 2.29 |
| rand500 | generated | 533 | TIMEOUT | TIMEOUT | - |
| rand1000 | generated | 1058 | TIMEOUT | TIMEOUT | - |

#### Monte Carlo (NOT comparable, see text): wall time (median)

| tree | kind | basic events | OpenFTA C | ftar | ftar / C |
|---|---|---:|---:|---:|---:|
| theatre | example | 3 | 7.4 ms | 4.1 ms | 0.55 |
| lift | example | 21 | 21 ms | 9.5 ms | 0.45 |
| orand100 | generated | 300 | exit139 | 52 ms | - |

#### Monte Carlo (NOT comparable, see text): peak resident memory (median)

| tree | kind | basic events | OpenFTA C | ftar | ftar / C |
|---|---|---:|---:|---:|---:|
| theatre | example | 3 | 2.7 MB | 3.7 MB | 1.39 |
| lift | example | 21 | 2.5 MB | 3.5 MB | 1.41 |
| orand100 | generated | 300 | exit139 | 3.8 MB | - |


### Same trees with the C engine built at `-O2`

`build.sh` builds the C engine with `-O`, which may understate the original. A second build with
`OPENFTA_CFLAGS=-O2` was run on a subset (3 runs each, same machine, shortly after). The `-O2` build was about 1.5 to
2x faster than `-O` on these trees, which cuts ftar's lead on the `andor` trees (cut sets: ratio about 0.4 becomes
about 0.8) but not on `orand1000`. The 8.8 ms ftar `theatre` row here is a one-off outlier (2.7 ms in the main run).

#### Cut sets: wall time (median)

| tree | kind | basic events | cut sets | OpenFTA C | ftar | ftar / C |
|---|---|---:|---:|---:|---:|---:|
| theatre | example | 3 | 2 | 2.9 ms | 8.8 ms | 3.03 |
| orand1000 | generated | 3000 | 1000 | 3.26 s | 18 ms | 0.01 |
| andor12 | generated | 24 | 4096 | 720 ms | 626 ms | 0.87 |
| andor13 | generated | 26 | 8192 | 3.11 s | 2.44 s | 0.79 |
| andor14 | generated | 28 | 16384 | 12.73 s | 9.41 s | 0.74 |

#### Cut sets: peak resident memory (median)

| tree | kind | basic events | cut sets | OpenFTA C | ftar | ftar / C |
|---|---|---:|---:|---:|---:|---:|
| theatre | example | 3 | 2 | 2.4 MB | 3.6 MB | 1.47 |
| orand1000 | generated | 3000 | 1000 | 6.2 MB | 6.1 MB | 0.98 |
| andor12 | generated | 24 | 4096 | 3.3 MB | 10.3 MB | 3.12 |
| andor13 | generated | 26 | 8192 | 4.2 MB | 20.1 MB | 4.83 |
| andor14 | generated | 28 | 16384 | 4.8 MB | 37.3 MB | 7.82 |

#### Probability (cut sets + probability analysis): wall time (median)

| tree | kind | basic events | OpenFTA C | ftar | ftar / C |
|---|---|---:|---:|---:|---:|
| theatre | example | 3 | 3.7 ms | 3.1 ms | 0.84 |
| orand1000 | generated | 3000 | 7.69 s | 36 ms | <0.01 |
| andor12 | generated | 24 | 2.03 s | 882 ms | 0.43 |
| andor13 | generated | 26 | 9.04 s | 3.48 s | 0.39 |
| andor14 | generated | 28 | 36.64 s | 13.71 s | 0.37 |

#### Probability (cut sets + probability analysis): peak resident memory (median)

| tree | kind | basic events | OpenFTA C | ftar | ftar / C |
|---|---|---:|---:|---:|---:|
| theatre | example | 3 | 2.7 MB | 3.7 MB | 1.39 |
| orand1000 | generated | 3000 | 6.6 MB | 6.3 MB | 0.95 |
| andor12 | generated | 24 | 3.4 MB | 14.9 MB | 4.42 |
| andor13 | generated | 26 | 4.3 MB | 26.4 MB | 6.14 |
| andor14 | generated | 28 | 6.1 MB | 50.7 MB | 8.32 |


## Summary

What was measured, no more.

* **Small trees (the five examples, up to about 100 cut sets)**: both engines finish in a few milliseconds, which is
  process start-up; no meaningful speed difference. ftar peaks at about 1.4x the memory of C there (about 3.6 MB vs
  2.6 MB).
* **Many cut sets, wide trees (`orand`)**: C slows down sharply. `orand1000` took 4.6 s for cut sets and 9.4 s for
  probability; 5000 and 20000 cut sets did not finish in 60 s. ftar did 1000 cut sets in 18 ms and 20000 in 4.1 s (cut
  sets) and 10.8 s (probability). This is the largest gap measured (over 100x at 1000 cut sets). Memory is about equal
  at 1000 cut sets (6.0 vs 6.5 MB).
* **Many cut sets, AND of ORs (`andor`)**: ftar is faster in time, about 2.5x on cut sets and 3x on probability
  against the `-O` C build (`andor10` to `andor14`), and about 1.2 to 1.35x on cut sets against the `-O2` build.
  **ftar uses much more memory here and the gap grows with size**: at 16384 cut sets ftar peaks at 37 MB against 4.8 MB
  for C (7.8x; probability 50.9 MB vs 6.3 MB, 8.1x). C's resident memory barely moves on these trees.
* **Random trees (`rand`)**: ftar was faster, 1.3x to 14x depending on tree and metric (`rand60` to `rand300`), and
  used 1.5 to 2.8x the memory. `rand500` and `rand1000` did not finish in 60 s on either engine.
* **Where ftar is slower or hungrier**: peak memory is higher than C on every tree except `orand1000` (about equal),
  and much higher (up to about 8x) on `andor`. In time, ftar was not slower than C anywhere beyond start-up noise.
* **Monte Carlo** (100000 trials; seed 1 for ftar) is listed only for completeness. The engines use different random
  generators and sampling code and nothing was checked about the statistical results, so time and memory are **not
  comparable**. The C engine crashed (exit 139, segmentation fault) on Monte Carlo for `orand100` (100 AND gates, 300
  events); ftar completed it.
* **Not covered**: runs beyond the 60 s limit, gate types other than AND/OR (XOR, INHIBIT, PRIORITY-AND, transfers
  beyond the examples), more than two probability terms in the tables, other machines, compiler settings beyond `-O`
  and `-O2`, and the GUI.

## Re-running

```bash
tools/bench/bench.sh > results.tsv                  # full run: tens of minutes (many 60 s timeouts)
python3 -I tools/bench/tables.py results.tsv        # Markdown tables like the ones above
QUICK=1 RUNS=3 tools/bench/bench.sh > quick.tsv     # small set, a few minutes
```

The knobs are listed at the top of `tools/bench/bench.sh` (`RUNS`, `TIMEOUT`, `TERMS`, `GEN`, `EXAMPLES`, `C_RUN`,
`FTAR`, `MC_TRIALS`). To compare against a `-O2` C build:
`OPENFTA_CFLAGS=-O2 tools/openfta-native/build.sh /some/dir`, then `C_RUN=/some/dir/openfta-run tools/bench/bench.sh`.
It needs GNU time, gcc, python3 and cargo, and builds `ftar` (release) and the C engine if they are missing.

The tables above come from two script runs, because `target/` was deleted from under the first run partway through:
the `rand` rows and the Monte Carlo rows are from the second run (same settings, `GEN` set explicitly).

## Update after #118 (cut set memory)

The tables above were measured at commit `8ca08ca`. Issue #118 changed the engine to intern event names to `u32` ids
(sorted boxed slices while computing, shared `Arc<str>` names in the result) and to skip absorption checks between
equal-order sets. The ftar columns above for the `andor` rows are therefore out of date. This section was **not** a
full re-run: it is ftar only, one run per cell, same machine and commands (`/usr/bin/time -v`, `ftar cutsets TREE` and
`ftar prob TREE --terms 2 --time 1`), measured on 2026-11-23 with the #118 change on top of `550e00f` (v0.3.0). The C
columns are unchanged from above.

| tree | cut sets | cut sets: memory before | after | time before | after | prob: memory before | after | time before | after |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| andor10 | 1024 | 4.3 MB | 3.8 MB | 40 ms | 10 ms | 4.8 MB | 4.0 MB | 50 ms | 20 ms |
| andor11 | 2048 | 5.7 MB | 4.2 MB | 140 ms | 10 ms | 6.1 MB | 4.4 MB | 210 ms | 70 ms |
| andor12 | 4096 | 10.3 MB | 4.5 MB | 630 ms | 10 ms | 14.7 MB | 5.4 MB | 880 ms | 230 ms |
| andor13 | 8192 | 20.2 MB | 5.5 MB | 2.44 s | 20 ms | 26.6 MB | 7.0 MB | 3.51 s | 1.04 s |
| andor14 | 16384 | 37.4 MB | 7.5 MB | 9.45 s | 30 ms | 50.5 MB | 10.6 MB | 14.4 s | 4.2 s |
| orand1000 | 1000 | 6.0 MB | 5.9 MB | 20 ms | 10 ms | 6.3 MB | 5.8 MB | 40 ms | 20 ms |
| orand5000 | 5000 | 20.2 MB | 20.2 MB | 260 ms | 20 ms | - | - | - | - |
| theatre, lift | 2, 12 | 3.6 MB | 3.7 MB | - | - | - | - | - | - |

andor14 is now 7.5 MB for cut sets (C: 4.8 MB, 1.6x instead of 7.8x) and 10.6 MB for probability (C: 6.3 MB). The
probability time on the andor rows is dominated by the two inclusion-exclusion terms, which are unchanged.

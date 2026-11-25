# Verification of the Rust analysis against OpenFTA

Reference: OpenFTA `dev` branch, `src/lib/Analysis/*.c`, `src/lib/Appn/{treeUtil,item,event_list}.c`.
The comparison is by reading the C, an independent brute-force oracle (`tests/verify.rs`), and reports from
a native build of OpenFTA's C core.

## Native OpenFTA reports (`tests/openfta_reports.rs`)

`tools/openfta-native/` builds OpenFTA's analysis engine (pinned commit of the `dev` branch) as a headless program,
with the Java GUI's callbacks replaced by a small C driver, and `generate.sh` writes its cut sets and probability
reports for every example tree in `tests/data` to `tests/data/openfta/`. `tests/openfta_reports.rs` checks ftar
against them:

- **Cut sets reports: identical** for all 11 trees (after #41: the order limit and empty orders are printed as in C).
- **Probability reports: identical layout**; numbers agree to float precision (item 7). Single inclusion-exclusion
  term values differ by up to 2.4e-5 relative where C sums C(54,4) = 316251 products in `float`; running totals and
  everything else within 1e-5.
- **Monte Carlo** is not compared: the random streams differ (item 18).

Two things the native build needs that the GUI does silently: the probability analysis reads the cut sets back from
`TREE.mcs` (saved by the cut sets run, used only when strictly newer than the tree, to the second); and on Linux
OpenFTA cannot open a `.ped` with CRLF line ends (item 20), so `generate.sh` converts them first.

## The oracle (`tests/verify.rs`)

The oracle evaluates the boolean function of the fault tree by truth table over all primary events.
It mirrors `eval_tree` (`treeUtil.c:1828-1895`) on the "process tree" built by
`generate_processing_pointers` (`item.c:190-233`): intermediate events and transfer-outs are transparent,
AND/INHIBIT/PRIORITY-AND are conjunctions, OR/XOR disjunctions, an analysed condition as first child of an OR/XOR
must hold, a not-analysed first child is ignored, a transfer-in evaluates the head of the transferred tree.
From the truth table it derives and compares with the library:

* (a) minimal cut sets = minimal true points (also for order limits 1..4),
* (b) exact top probability = sum over the truth table (also with order limits, and "L" events at several unit times),
  plus the alternating upper/lower bound property of the inclusion-exclusion partial sums,
* (c) importance contribution = sum of the probabilities of the cut sets containing the event, importance = 100 * contribution / top,
* Monte Carlo estimates (top probability and per-event joint failure frequency) against the exact values (6 sigma).

Inputs: 300 seeded random trees (own PRNG; shared events, nested intermediates/gates of all five kinds,
analysed and not-analysed conditions, transfer-in files with transfer-out markers, `L` events, `.fta`/`.ped`
written to disk and loaded through `Project::load`), plus every example tree: `theatre`, `lift`, `three_levels`
(top/middle/bottom), `Three Motor Example (Motor 2 Only)` + `T.fta` (54 cut sets), `transTest*`.
Mutation check: treating XOR as AND makes the random test fail.

Result: the cut sets, exact probabilities and importances agree with the oracle on all of them.
`Three Motor` has 54 cut sets, so only the order-limited probabilities (<= 16 sets) are compared there.

## PFTA's time-dependent analysis (`tests/pfta.rs`)

`ftar time` (`ftar_core::quant`) follows PFTA, which publishes its mathematics (`MATHS.md`) and
whose sources were read for the edge cases. No PFTA run is part of the tests: ftar does not execute
PFTA. The checks are:

- **Oracle.** 60 random trees of AND, OR and VOTE gates over 2 to 7 events with random `q` and `ω`.
  Every gate's `q` and `ω` at full order agree to 1e-10 with brute force over every state: exact `q`,
  and `ω = Σ ω_e (Q(e failed) − Q(e working))`. So the truncated expansion PFTA uses (`ω = ω¹ − ω²`)
  is implemented so that it is exact when not truncated.
- **Worked examples.** The examples in PFTA's README give the same numbers: the toast tree, and
  `A AND B` OR `C` with rate 0.6301.
- **Closed forms.** Constant-rate events over time, VOTE(2) of 3, and the rare-event approximation
  at `computational_order: 1`.
- **Edge cases.** PFTA's answers for zero, infinite and `nan` rates and times in the constant-rate
  model, and `True`/`False` events simplifying cut sets, are copied as unit tests.
- **OpenFTA trees.** `ftar time theatre.fta --times 1` gives OpenFTA's exact top probability.

Known differences from PFTA:

- **Sampling.** ftar's generator is xoshiro256** seeded from the `seed` text; PFTA's is Python's
  Mersenne Twister. Sampled numbers therefore match PFTA's in distribution only. Without a `seed`,
  PFTA draws a random one and ftar uses `ftar`, so ftar's runs stay reproducible.
- **Rounding.** ftar sums terms in a fixed order, where PFTA's order follows a Python set. Results
  can differ in the last digits.
- **Large trees.** A gate whose expansion needs more than 50 million combinations stops with an
  error asking for `computational_order`. PFTA keeps computing.
- **Not written yet.** The importance tables and the SVG drawings per time.

## Confirmed equal

| Item | C | Rust |
|---|---|---|
| XOR analysed as OR | `CutSetsByAlgebraicMethod.c:104-105` | `analysis.rs` `Or \| Xor` |
| INHIBIT and PRIORITY_AND are plain AND | `…Algebraic…c:154-156` | `And \| Inhibit \| PriorityAnd` |
| Analysed condition first in OR/XOR is ANDed with the OR of the rest; not-analysed first child ignored | `…c:113-125, 144-150` | `analysis.rs` Or branch |
| In AND, a not-analysed first child is skipped; NULL children skipped | `…c:162-181` | `None` results skipped |
| Transfer-out contributes nothing, transfer-in is the transferred tree | `…c:185-192`, `item.c:214-216` | `strip_transfer_out`, `inline_transfers` |
| Absorption law / duplicates removed | `ExprORGroup` `NormalisedBooleanExpressions.c:385-455` | `minimise` |
| Order limit prunes products with more than `limit` events (monotone, so equal to filtering the full set) | `ExprAND` `…c:494` | `and()` |
| Combo method result = minimal true points of `eval_tree` = algebraic result | `CutSetsByComboMethod.c:229-241, 298, 331` | (same function; verified by oracle) |
| `L` events: `p = 1 - exp(-lambda * unit_time)`, only when the lambda marker is exactly `'L'` | `event_list.c:543-548` | `effective_probability` |
| Cut set probability = product over events | `calc_sub_term` `…c:921-952` | `combos`, `cut_set_probability` |
| k-th inclusion-exclusion term = sum over k-combinations of P(union), sign `(-1)^(k+1)`; terms clamped to the number of cut sets | `ExprProb` `…c:1094-1117`, `ProbabilityAnalysis.c:165-169, 261-276` | `probability_report` |
| "Exact" only when all terms are used | `ProbabilityAnalysis.c:274` | `exact` |
| Importance: `imp[j] = sum of P(cut set) over cut sets containing event j`; percentage `100*imp/p` with p the (possibly partial) sum | `ProbabilityAnalysis.c:291-303, 325` | `events` |
| Monte Carlo: P(at least one component fails) | `calc_bas_parms` `event_list.c:679-694` | `any_component_failure` |
| Event identity is the id only (one global list; shared across transferred trees) | `event_list.c:159-169` | `ids` dedup |
| COND_ANAL events are primary events with a probability | `event_list.c:151-155`, `set_bas_prob` | `ids` includes `ConditionAnalysed` |

## Discrepancies

### Fixed (definite bugs, regression tests added)

1. **Event type from the PED overrides the tree file** (`item.c:457-467`: `ip->type = primary_event_item_type(component->type)`
   for every primary item found in the tree's own database; PED types are `B E U C N`). Rust used the type letter in the
   `.fta`, so an event whose database type is `N`/`C` (condition) but is stored as `B` in the tree was analysed as a plain event.
   Fix: `project.rs` `apply_ped_types`, applied per file with that file's own `.ped`. Test: `ped_type_overrides_tree_type`.
2. **Monte Carlo seed aliasing** (`montecarlo.rs`, `Rng(opts.seed | 1)`): seeds `2k` and `2k+1` produced identical runs,
   and small seeds gave a poor start for xorshift. Fix: splitmix64 scrambling in `Rng::seeded`.
   Test: `monte_carlo_neighbouring_seeds_differ`. (Run output for a given seed changes.)

### Not changed (design differences or edge cases; proposed fixes)

3. **Order 0 means "unlimited" in C** (`ExprAND`: `(limit > 0) && …`, `NormalisedBooleanExpressions.c:494`); in Rust
   `max_order = 0` gives no cut sets (`analysis.rs:34`, `and`). The GUI never passes 0, so low impact.
   Proposal: map 0 to `usize::MAX` in `minimal_cut_sets_limited`/`probability_report` (and say so in the CLI help).
4. **Missing primary event in the PED**: C silently uses probability 0.0 (`event_list.c:552-553`, `set_bas_prob`);
   Rust returns `Error::MissingEvent`. Rust's behaviour is arguably better (a typo does not silently drop a cut set);
   proposal: keep the error, optionally add a `--missing-as-zero` switch for bit-for-bit parity.
5. **Importance and cut-set list use all generated cut sets in C, only `<= max_order` in Rust.** C generates cut sets once
   (`tree->max_order`) and `calculate_probs` uses *all* of them for the per-cut-set table and importance
   (`ProbabilityAnalysis.c:207, 291-303`) but only those `<= max_order` for the top probability (`ExprProb`, `…c:1061-1062`).
   Rust uses a single order for both. They differ only when the probability order is lower than the cut-set order.
   Proposal: separate `cutset_order` from `prob_order` in `ProbabilityOptions` if exact report parity is wanted.
6. **No cut sets**: C returns FALSE without a report (`ProbabilityAnalysis.c:165-167`); Rust returns top 0, exact.
7. **Probability precision**: C stores probabilities, cut-set terms and sums as `float` (`primary_event.h`, `ProbabilityAnalysis.c:95-96`,
   `NormalisedBooleanExpressions.c:877-878`); Rust uses `f64`. Measured against the native build: last-digit differences
   on most numbers, up to 2.4e-5 relative on a term that sums 316251 `float` products. Rust is the more accurate one.
8. **Monte Carlo estimator. (RESOLVED: ported - `Method::FirstFailure` is now the default, with the failure-mode table and the C error formula; `Method::Direct` keeps the old behaviour. Text below describes the original discrepancy.)** C simulates only runs with at least one failure: it picks the first failed event with the
   conditional distribution `rel[]` and fails the later events independently (`MonteCarloSimulation.c:536-575`,
   `calc_bas_parms`), and reports `P = P(any failure) * failures / tests` with error `P/sqrt(failures)` (`…c:312, 342-344`).
   Rust draws every event independently on every trial and reports `failures / trials` with error `sqrt(p(1-p)/n)`.
   Both are unbiased for the top probability; the C scheme has much lower variance for rare events (at 1e-6 event
   probabilities Rust needs ~1e6 times more trials to see a failure). Rust also reports per-event and per-cut-set counts
   instead of C's failure-mode ranking (`MonteFileWriteFailures`, compressed failure modes, `other`) and importance
   `imp[j] = P(any) * (failures with event j) / tests` (`…c:244`). Proposal: port the first-failure scheme
   (`calc_bas_parms` + `rand_disc`) and the failure-mode table; `event_failures` counts already match the C
   importance numerator. The C uses `frand() <= p`, Rust `< p` (measure-zero difference). Rust evaluates the minimal cut
   sets, C evaluates the tree (`eval_tree`); these agree for the monotone trees OpenFTA produces.
9. **Basic event order. (RESOLVED: ported - `Tree::primary_event_ids` sorts by byte order, as `strcmp`; reports, importance,
   cut-set lines, failure statistics, Monte Carlo and the GUI all use it. Text below describes the original discrepancy.)**
   C reports are alphabetical (`strcmp` insertion, `event_list.c:159-196`), Rust used tree order. The order also decides which
   event is the "first failure" in the Monte Carlo method, so per-seed Monte Carlo numbers changed with the fix.
10. **Event database lookup**: C looks an event up in the PED of the *file in which it is first met* during traversal and uses
    probability 0.0 if that PED lacks it (`event_list.c:171-175, 539-554`). Rust merges all PEDs, main file first, first definition wins.
    Differs only when the same id has different values in different PEDs or is defined only in another file's PED.
11. **Transfer-in path**: nested transfer-ins are resolved relative to the *top* tree's directory in C
    (`event_list.c:225, 262-263`); Rust resolves relative to the directory of the file containing the transfer-in.
    Identical when all files share a directory. C has no cycle detection (infinite recursion); Rust reports an error.
12. **Non-first not-analysed conditions**: algebraic method returns NULL (ignored, with an error print, `…Algebraic…c:99-102`),
    but `eval_tree` returns TRUE (`treeUtil.c:1859-1863` handles only the first child; a later `N` evaluates TRUE and makes an
    OR true). So in C the combo method and Monte Carlo can disagree with the algebraic method for such trees. Rust follows
    the algebraic method everywhere. The GUI normally only allows the condition as first child.
13. **Degenerate gates**: an OR whose only child is an analysed condition, or an OR whose other children all vanish, dereferences
    NULL in C (`…Algebraic…c:113-123, 145`); Rust returns the condition's cut sets.
14. **Intermediate with several children**: C splices an intermediate's children into the parent's child list
    (`item.c:208-212`); Rust treats an `M` with several children as an OR (`analysis.rs`). Real files have exactly one gate under an `M`.
15. **`'D'` as a type letter** is accepted by Rust (`EventType::Dormant`); in C `D` is only the dormant *modifier* (second
    character), and a leading `D` is an invalid item type (`…Algebraic…c:194-196`). Harmless.
16. **`top_probability`** (fast bounds) ignores unit time (uses 1.0), `probability_report` honours it.
17. **PED lambda marker written by the Java GUI** is `(char)TRUE` = `\x01`, not `'L'` (`NativePEDFrame.c:120`), while the analysis
    tests for `'L'` (`event_list.c:543`) and `count_lambda_events` for any non-NUL (`treeUtil.c:1802`). Files saved by that GUI
    therefore contain rate events that no analysis treats as rates, in C and in Rust alike. Likely a bug in the `dev` branch of OpenFTA.

18. **Monte Carlo random numbers**: ftar uses a 64-bit generator, OpenFTA's C code a 32-bit one, so the same seed or trial count
    gives different counts. On `lift.fta` (100000 trials) top-event estimates were 1.198924E-05 (ftar) and 1.199722E-05
    (OpenFTA), both within one standard error (4.1E-08) of the exact 1.199993E-05. Expected, not a bug; compare Monte Carlo runs
    statistically, never digit by digit.
19. **Long lines in reports** (issue #9). Event lists wrap as in C (`BitPara`/`ParaGrow`, `event_list.c`, `util.c`): at 50 bytes
    for cut sets, 30 for probabilities and 20 for Monte Carlo failure modes, with the same continuation formats
    (`tests/report.rs` pins all three). Columns are padded in bytes like C's `%-Ns`, so non-ASCII ids line up as in
    OpenFTA's files (before #9 ftar padded in characters). `tests/data/wrap` (43 events, six cut sets of 4 to 9 events,
    ids of 1 to 12 bytes including `å`, `ä`, `ö`) makes every cut set wrap at 50 and 30 bytes; its reports from the native build are
    byte-identical to ftar's, and fail with character padding. OpenFTA limits ids to 13 bytes (`MAX_SYMBOL_ID_LENGTH`), so the
    ids-longer-than-the-line case below cannot arise from a loaded file. One difference is kept: an id longer than the line width is
    dropped from the line by `ParaGrow` ("string too long") and makes `ParaCreate` return NULL, which `BitPara`'s caller then
    dereferences when it is the first id. ftar prints the id on its own line instead.
20. **`.ped` with CRLF line ends**: OpenFTA built for Linux fails to open them ("error opening file") and then crashes
    in the analysis; on Windows the C runtime strips the `\r`. Two example databases in `tests/data/transtest` have
    CRLF. ftar reads both.
21. **Monte Carlo on very simple models** (found by hand testing, 2026-11): OpenFTA and the other FTA tools tried crash
    on small trees (a single event, an AND of two rare events) where no simulated run fails, and ftar was the only one
    that gave a report. In OpenFTA's C, `fault_num = 0` gives `monte_carlo_prob = 0`, and the report then divides by both:
    the top event's error `monte_carlo_prob / sqrt(fault_num)` (`MonteCarloSimulation.c:344`) and every importance
    `100 * imp[i] / monte_carlo_prob` (`…c:261`) are 0/0. When every probability is 0 the C still runs all trials:
    `rel[]` is all zero, so `rand_disc` returns the last event and fails it with probability 0. Only afterwards does the
    report say "analysis abandoned" (`…c:322-327`). It asserts `monte_carlo_n > 0` (`…c:441`) and returns without a report
    for a tree with no primary events (`…c:470-473`). ftar runs no trials when every probability is 0 and prints the same
    "analysis abandoned" line. With no system failures it prints a 95% upper bound instead of `+/-`, explains why no
    failure modes are listed, and shows 0 % importance. Trials = 0 count as 1 in the divisions. Test:
    `monte_carlo_on_degenerate_models_stays_finite` (single event at p = 0, 1e-12, 0.5 and 1, two-event AND at 1e-9 and 1,
    both methods, 0, 1 and 1000 trials: no NaN or inf in the numbers or the report).

22. **Numbers OpenFTA never checks** (found by the edge-value test `edge_probability_trees_stay_finite`): the C reads `.ped`
    probabilities with `atof` and takes the unit time as typed, so a probability of 2, -1, `nan` or `1e999`, or a negative
    unit time, gives top probabilities above 1, below 0 or NaN. ftar now rejects them: a `.ped` probability must be within
    0..1 (a failure rate `>= 0`), finite either way (`parse_ped`, a parse error naming the line), and the unit time must be
    finite and `>= 0` (`check_unit_time`, `Error::InvalidInput`, in `probability_report`, `cut_set_probability` and
    `simulate`; the GUI field is clamped). Tests: `ped_probabilities_out_of_range_are_parse_errors`,
    `unit_time_must_be_finite_and_not_negative`.

### Notes on the C snapshot

* `generate_processing_pointers` (`item.c:190-233`) only links `process_sibling` for gates through the intermediate-event branch
  (`item.c:212`) and the leaf branch (`item.c:226`). That is correct only because every gate is the single child of an
  intermediate event, which holds for all example files. A gate directly under a gate would lose its right siblings.
* `TRANSOUT` handling recurses on `ip->right_sib` and would dereference NULL if a transfer-out is the last child (`item.c:214-216`).
  In all files the marker comes first under an intermediate event.

## Proposed follow-ups (priority order)

1. Port the first-failure Monte Carlo scheme and the failure-mode report (item 8).
2. Treat order 0 as unlimited (item 3); decide on missing-event policy (item 4).
3. Split cut-set order from probability order if byte-compatible reports are required (item 5).
4. ~~Compare reports with a native OpenFTA build~~ (done: `tests/openfta_reports.rs`, #12).

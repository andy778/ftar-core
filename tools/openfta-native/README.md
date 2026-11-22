# Native OpenFTA, for comparison

Builds the original OpenFTA analysis engine (its C core, without the Java GUI) as a headless program, `openfta-run`.
It makes the report fixtures in `crates/core/tests/data/openfta/`, which `crates/core/tests/openfta_reports.rs`
compares ftar against. CI does not build it: the fixtures are committed.

```bash
tools/openfta-native/build.sh       # clones OpenFTA at a pinned commit into target/openfta-native, builds openfta-run
tools/openfta-native/generate.sh    # rewrites the fixtures from it
```

The build needs `git` and `gcc`. `openfta-run TREE.fta cutsets ORDER | prob ORDER UNIT TERMS | mc TRIALS UNIT` prints
one report to stdout; OpenFTA's own progress and debug output goes to stderr and stdout before the report title.

- `driver.c` does what the JNI layer and the Java GUI did: set up the tree, load it, load its `.ped`, run the
  analysis the dialog's OK button runs, show the report file. For probabilities it first runs the cut sets analysis and
  reads them back from `TREE.mcs`, as the GUI's probability dialog does.
- `stubs.c` replaces the remaining GUI callbacks (drawing, dialogs, progress bars) with no-ops.
- Two files in OpenFTA's tree are left out: a stray `Copy of FailureStatistics.c` and `PostScriptCanvas.c` (printing,
  does not compile).

What the comparison found is in `docs/VERIFICATION.md` ("Native OpenFTA reports").

Licence: GPL-2.0-or-later, as OpenFTA, (c) 2005 FSC Limited.

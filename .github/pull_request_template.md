<!-- Every change has an issue. Use "Closes #N" if this PR completes it, "Refs #N" if it covers only part. -->
Closes #

## What and why


## Checklist
- [ ] `cargo xtask ci` passes locally (fmt, clippy, layers, tests)
- [ ] New behaviour has tests; a bug fix has a regression test that fails without the fix
- [ ] No new `unwrap`/`expect`/`panic!`/indexing on untrusted input in non-test code
- [ ] GUI change: headless `egui_kittest` test, widgets have accessible names
- [ ] Differences from OpenFTA recorded in `docs/VERIFICATION.md`
- [ ] README status checklist / docs updated if a capability changed
- [ ] Workflow changes (`.github/workflows`) are in their own PR

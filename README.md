# ftar-core

Rust port of the [OpenFTA](https://github.com/luyangshang/OpenFTA) fault tree analysis engine (unmaintained since 2013).
Java 21 baseline of the original: <https://github.com/andy778/OpenFTA/tree/dev>.

Status (v0.1 scaffold):

- [x] `.fta` tree and `.ped` primary-event database parsers
- [x] Minimal cut sets (AND/OR/inhibit/priority-AND, intermediate events, conditions)
- [x] Top-event probability (rare-event and min-cut upper bound)
- [ ] XOR gates, transfer gates (multi-file trees)
- [ ] Monte Carlo simulation, failure statistics, importance measures
- [ ] Verify numbers against the original engine's reports (`src/lib/Analysis` upstream)
- [ ] CLI (`ftar`) and GUI

License: GPL-2.0-or-later (derived from OpenFTA, © 2005 FSC Limited). A `LICENSE` file with the GPL-2.0 text still needs adding.

# GUI parity with OpenFTA

Every menu item, shortcut and dialog of OpenFTA's Java GUI (`src/gui/FTAGUI`, branch `dev`: `FTAFrame.java`
menus and toolbars, the `*Dialog.java` files, `PEDFrame.java`, `Preferences.java`, `PrintPreferences.java`) and
what ftar does for it. Status: **same** (same command, same effect), **differs** (works, but not identically),
**missing** (no equivalent yet), **extra** (ftar only). The right-hand column names the test that proves the
ftar side where there is one; rows without a test are checked by eye only.

The analysis results themselves are proven equal in [VERIFICATION.md](VERIFICATION.md).

## File menu

| OpenFTA | Shortcut | ftar | Status | Test |
|---|---|---|---|---|
| New | Ctrl+N | File > New, Ctrl+N, toolbar | same | `headless.rs` |
| Open... | Ctrl+O | File > Open..., Ctrl+O, toolbar | same | `headless.rs` |
| Save | Ctrl+S | File > Save, Ctrl+S, toolbar | same | `headless.rs` |
| Save As... | Ctrl+A | File > Save As..., Ctrl+A | same | Ctrl+A not tested (opens a file dialog) |
| Export Image... | | File > Export image... | same: JPEG (also PNG and SVG) | `headless.rs` |
| Printer Setup... | | File > Page setup and print preview... | differs: one window for the three OpenFTA entries (#78) | `print_preview.rs` |
| Page Setup... | | (same window) | differs (#78) | `print_preview.rs` |
| Print Preview... | | (same window) | differs (#78) | `print_preview.rs` |
| Print... | Ctrl+P | File > Print..., Ctrl+P, toolbar | same | `print_preview.rs` |
| Database | | File > Primary event database... | same (its own window, #94) | `ped_window.rs` |
| Exit | Ctrl+Q | File > Quit, Ctrl+Q | differs: the name | `headless.rs` |
| | | Export report..., failure-mode CSV | extra | `headless.rs` |

## Edit menu

| OpenFTA | Shortcut | ftar | Status | Test |
|---|---|---|---|---|
| Undo | Ctrl+Z | Edit > Undo, Ctrl+Z, toolbar | same | `headless.rs` |
| Cut / Copy / Paste | Ctrl+X/C/V | Edit menu, shortcuts, toolbar | same | `headless.rs` |
| Delete | Ctrl+D | Edit > Delete selected, Ctrl+D, Delete key, toolbar | same | `headless.rs`, `toolbar.rs` |
| Shift Left / Right | Ctrl+L / Ctrl+R | Edit menu, Ctrl+L / Ctrl+R, details panel | same | `headless.rs`, `selected_shift.rs` |
| Link | Ctrl+I | Edit > Link, Ctrl+I, toolbar | same | `ped_window.rs` |
| Selected... | | Edit > Selected..., double-click | same | `selected_shift.rs` |
| Preferences... | | Edit > Preferences... | same | `preferences.rs` |
| | | Redo (Ctrl+Y) | extra | `headless.rs` |

## View, Analysis, Report, Help

| OpenFTA | ftar | Status | Test |
|---|---|---|---|
| View > Tree Overview | View > Tree Overview | same | `headless.rs` |
| View > Zoom In / Out | View menu, Ctrl+Plus/Minus, toolbar | same | `headless.rs` |
| View > Set Zoom... | View > Set Zoom... | same | `headless.rs` |
| Analysis > Validate | Analysis > Validate | same | `analysis_menu.rs` |
| Analysis > Minimal Cut Sets... (dialog) | Cut sets tab in the right panel | differs: panel, not dialog | `headless.rs` |
| Analysis > Numerical Probability... (dialog) | Probability tab | differs: panel | `headless.rs` |
| Analysis > Monte Carlo Simulation... (dialog) | Monte Carlo tab | differs: panel | `headless.rs` |
| Report > View > Qualitative / Probability / Monte Carlo | same menu | same | `report_view.rs` |
| Report > Print > the three reports | same menu | same | `report_view.rs` |
| Help > Help on Help, Index, User Support, History, Copyright | Help > the same five pages, then User guide, About ftar | same (pages written for ftar) | `help.rs` |
| | View > Theme | extra | `headless.rs` |

## Analysis dialogs

| OpenFTA control | ftar | Status |
|---|---|---|
| Cut sets: "Generate all" / "Limit order" | max order (0 = all) | same |
| Cut sets: "Use minimal cut sets on file" | none: every run uses the current tree | omitted on purpose |
| Cut sets: "There are n basic events in the tree" | same text on every analysis tab | same |
| Probability: number of terms, unit time, limit order | terms, unit time, max order | same |
| Probability: "Use minimal cut sets on file" | none: every run uses the current tree | omitted on purpose |
| Probability / Monte Carlo: "Estimated run time" | none: OpenFTA's comes from a benchmark of its own engine | omitted on purpose |
| Monte Carlo: number of simulations, unit time | trials, unit time (plus seed, method) | same (extra seed) |
| OK, Cancel, Help in each dialog | Run, Analysis help; no Cancel (the panel is not modal) | differs |

## Symbol dialogs and canvas

| OpenFTA | ftar | Status | Test |
|---|---|---|---|
| Gate dialog (ID, Type: And, Or, Xor, Inhibit, Priority And) | details panel (id, type) | differs: panel | `headless.rs` |
| Intermediate event dialog (ID, Description) | details panel | differs: panel | `headless.rs` |
| Transfer tree dialog (ID) | details panel | differs: panel | |
| Tree text dialog (Description) | tree title field | differs | |
| Symbol palette (12 symbols) | same palette, greyed when no child can be added | same | `toolbar.rs` |
| Toolbar: New, Open, Save, Print, Cut, Copy, Paste, Link, database, Zoom In/Out | same, plus Delete, Undo, Redo, Fit | same plus extras | `toolbar.rs` |

## Primary event database (OpenPED)

| OpenFTA | ftar | Status | Test |
|---|---|---|---|
| Own window, File/Edit/Help menus | own window, same menus | same | `ped_window.rs` |
| Toolbar: New, Open, Save, Print, cut/copy/paste, database | New, Open, Save, Print, Delete, copy to / from tree, Link | differs: events are copied between databases, not cut | `ped_window.rs` |
| Primary Events list, Id, Probability, Constant Failure Rate/Unit Time, Type (5 radios), Dormant, Description | same labels and groups | same | `ped_window.rs` |
| Apply, Clear, Help | same | same | `ped_window.rs` |
| Export text file | File > Export Text File... | same | `ped_window.rs` |

## Preferences

| OpenFTA | ftar | Status | Test |
|---|---|---|---|
| Background, Pen, Highlight colours | same | same | `preferences.rs` |
| Symbol colours (Edit...) | per-symbol colours | same | `preferences.rs` |
| View: Colour / Monochrome | monochrome switch | same | `preferences.rs` |
| Defaults | Reset to defaults | same | `preferences.rs` |
| Web Browser Launch Command | same field with Browse...; empty = system default | same | `preferences.rs` |

## What the matrix shows

Same behaviour for the editing, analysis, report and database commands, the shortcuts, the Help pages, JPEG
export and the browser launch command (#101 to #105 are done). What is left:

1. **Print dialogs (#78):** printer setup, page setup and print preview are one window in ftar, three entries in
   OpenFTA. Undecided.
2. **Omitted on purpose** (see the Analysis dialogs table): "use minimal cut sets on file", the estimated run time
   and Cancel in the analysis panel.
3. **Names:** File > Quit, where OpenFTA says Exit.

Items that stay different on purpose: the analysis tabs instead of dialogs, symbol editing in the details panel,
Redo, the theme choice, CSV export.

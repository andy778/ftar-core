# ftar user guide

ftar is a fault tree analysis tool. It draws and edits OpenFTA `.fta` trees, keeps
primary event data in `.ped` databases, and runs cut set, probability and Monte Carlo
analyses. This guide covers the desktop app (ftar-gui). The command line (`ftar`) and
the automation tools are described in the repository README and docs/mcp.md.

## Getting started

- Start the app with a file to open it directly: `ftar-gui path/to/tree.fta`.
- Or use File > Open... (Ctrl+O) to open a `.fta` file, or File > New (Ctrl+N) for an empty tree.
- The window title shows the file name. An asterisk marks unsaved changes.
- The top event is the root of the tree. Everything else hangs below it.

## Looking at the tree

- Click a node to select it. Its details appear in the panel at the bottom.
- Scroll the mouse wheel to zoom, and drag on the canvas to pan.
- Fit (toolbar) scales the whole tree to the window. Panning or zooming by hand
  turns automatic fitting off until you press Fit again.
- Gates (AND, OR, XOR, priority AND, inhibit) and events (basic, undeveloped, external,
  dormant, condition, intermediate, transfer in and transfer out) are drawn with the
  OpenFTA symbols.

## Editing the tree

- Select a node, then use Add child in the details panel. Choose the kind of node
  first; new nodes go under the selected node, or under the top event when nothing is selected.
- Change a node's type, id, description or probability in the details panel.
- The icon toolbar has New, Open, Save, Print, Cut, Copy, Paste, Link, Undo, Redo, Zoom In, Zoom Out and Fit. The symbol palette on the left adds the clicked symbol to the selected gate or intermediate event (the top event when nothing is selected).
- Edit > Shift Left / Shift Right (or the Shift left / Shift right buttons) move a node one place among its siblings; conditions and transfer-out symbols stay in place. Edit > Selected... (or a double-click) edits the selected symbol: primary events open in the Primary events editor, anything else puts the cursor in the id field.
- Delete (the Delete key, the details panel or Edit > Delete selected) removes the
  selected node and its subtree.
- Undo (Ctrl+Z) and Redo (Ctrl+Y) cover tree and database changes. They are also in the Edit menu.
- Invalid edits are refused and the reason is shown in red. Nothing changes in that case.

## Primary events

- Basic events take their probability (or failure rate per unit time) from a primary
  event database, a `.ped` file. The database field in the toolbar names the database the tree uses.
- Edit > Primary events... opens the primary event editor. It lists the entries of the
  database, with id, probability or failure rate, type, dormant flag and description.
  Apply saves the form to the entry, Clear empties the form and Delete removes the entry.
- The editor also lists the other trees in the same folder that use the selected event,
  so you can see what a change will affect.

## Saving

- File > Save (Ctrl+S) writes the tree back to its `.fta` file. Save As... writes a copy under a new name.
- When you start a New tree, Open a file or Quit with unsaved changes, ftar asks first:
  Save, Discard or Cancel.

## Analysis

The panel on the right runs analyses on the current tree. Press Run to start one.

- Cut sets: the minimal cut sets of the top event. Max order limits the size of the sets
  shown (0 means no limit). Terms sets how many terms the exact calculation keeps.
- Probability: the probability of the top event, computed by inclusion-exclusion over the
  cut sets, with the per-term bounds shown. Event importance is listed with the results.
- Monte Carlo: a simulation with a number of trials, a random seed (the same seed gives the
  same result) and a method. It reports the estimate, its uncertainty and the failure modes seen.
- Unit time is the time used to turn failure rates into probabilities.
- The report menu selects the text report. Export report... saves it as a `.txt` file,
  Copy report copies it to the clipboard, and Export failure modes (CSV)... saves the Monte
  Carlo failure modes as a spreadsheet-friendly file.

## Pictures and printing

- File > Export image... saves the tree as a PNG or SVG drawing.
- File > Print... opens the tree in your browser's print dialog (Ctrl+P). The printout
  can include the selected report and the event probabilities; both are options in the File menu.

## Help menu

- User guide opens this guide inside the app.
- About ftar shows the version, the licence and where the code and the original OpenFTA come from.

## Troubleshooting

- A file that cannot be read shows an error message instead of closing the app. Check that the file is a `.fta` tree.
- Analyses are not re-run automatically after an edit. Press Run again to refresh the results.

## About ftar

ftar is a Rust port of OpenFTA, the fault tree analysis tool by FSC Limited (c) 2005,
which is unmaintained. The port is free software, licensed under the GNU General Public
Licence version 2 or later (GPL-2.0-or-later). The full licence text is the LICENSE file in
the installation or repository; NOTICE lists the attributions. The original OpenFTA
sources are at https://github.com/andy778/OpenFTA (branch dev), and the port is at
https://github.com/andy778/ftar-core.

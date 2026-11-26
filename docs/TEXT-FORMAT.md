# The text format (`.ftt`)

A fault tree as plain text, for writing by hand, reviewing in a pull request and diffing in git.
One file holds the tree and its primary event database (an OpenFTA tree needs a `.fta` and a `.ped`).

The syntax follows the paragraphs of [PFTA](https://github.com/public-fta/pfta): a header line, then
`- key: value` lines. The meaning is ftar's own: gate and event types are OpenFTA's, and the analyses
are the same as for `.fta` files. ftar does not read PFTA files yet. Their keys for time, failure
models and sampling (`times`, `model_type`, `intensity`, ...) give "not supported yet".

```text
# The theatre example of OpenFTA.
- note: Failure Of An Electrical Supply To An Operating Theatre.

Gate: Top
- label: Electrical Power To Operating Theatre Fails.
- type: AND
- inputs: Backup_Fail, MAINS_FAIL

Gate: Backup_Fail
- label: Backup Generator Fails to Start When Mains Power Fails.
- type: OR
- inputs: RELAY_FAIL, GEN_FAIL

Event: MAINS_FAIL
- label: Mains Power Fails.
- probability: 0.03

Event: RELAY_FAIL
- label: Backup Generator Relay Fails.
- probability: 0.05

Event: GEN_FAIL
- label: Backup Generator Fails.
- failure_rate: 2e-2
```

## Lines and paragraphs

- A paragraph starts with a header, `Gate: ID` or `Event: ID`, and goes on with `- key: value` lines.
  A blank line ends it. Each key appears at most once in a paragraph (except `note`).
- Lines starting with `#` are comments. Leading spaces are ignored, and so are CRs before the line end.
- Values start after the first `:`. Leading spaces are dropped; the rest of the line is kept.
- Ids cannot be empty, cannot contain spaces, `,` or `;`, and cannot be `NULL`. Each id names one
  paragraph.
- Unknown keys are errors, so a typo does not go unnoticed.

## Tree properties

An optional first paragraph without a header:

| Key | Meaning |
|---|---|
| `note` | The tree's note. Repeat the key for more lines. |
| `transfer_out` | The tree is used elsewhere through a transfer-in under this name (OpenFTA's transfer-out symbol under the top event). |

## `Gate: ID`

An intermediate event together with its gate. The first `Gate` paragraph is the top event. Every
other gate must be an input of some gate.

| Key | Meaning |
|---|---|
| `label` | Description of the intermediate event. |
| `type` | `AND`, `OR`, `XOR`, `INHIBIT` or `PRIORITY_AND` (any case). Without a type, the inputs hang directly under the intermediate event. OpenFTA uses this for a single transfer-in. |
| `gate_id` | Id of the gate symbol itself (OpenFTA gates may have one). Only with `type`. |
| `inputs` | Comma-separated ids of gates and events. A gate used by several others is copied into each, so it counts once per use. |
| `id` | The intermediate event's own id, when it differs from the paragraph's. The writer uses it when one id names two different subtrees, as in transfer-in trees that reuse names, or when an event already has the id. |
| `unnamed` | `true`: the intermediate event has no id. The paragraph's id only names it in the file (the writer uses `unnamed_1`, `unnamed_2`, ...). |

## `Event: ID`

A leaf of the tree, and an entry in the event database when it has a value.

| Key | Meaning |
|---|---|
| `type` | `basic` (default), `undeveloped`, `house`, `dormant`, `condition`, `condition_not_analysed` or `transfer_in`. For `transfer_in`, the id is the file name of the transferred tree (`.fta` or `.ftt`, in the same folder). |
| `probability` | Probability of failure, 0 to 1. |
| `failure_rate` | Constant failure rate. The probability is `1 - exp(-rate * time)` for the analysis unit time (OpenFTA's `L` events). |
| `label` | Description (database). |
| `group` | Group name (database). |
| `dormant` | `true` for OpenFTA's dormant marker. |

An event needs `probability` or `failure_rate` (not both) to be in the database, and then it may have
a `label` and a `group`. Without a value it is only a leaf, and the analyses report it as missing, as
they do for a `.fta` event that is not in its `.ped`. An `Event` that no gate uses is a database entry
only.

## Limits

A file may expand to at most 100,000 nodes (`ftar_core::text::MAX_NODES`) and 1,000 levels. This
keeps gates that are shared at every level from asking for an exponential tree.

## Converting

```sh
ftar convert theatre.fta theatre.ftt           # tree + its .ped into one text file
ftar convert theatre.ftt copy.fta              # writes copy.fta and copy.ped
ftar convert top.fta all.ftt --inline          # copy transfer-in trees into the output
```

`convert` never overwrites a file. The GUI opens `.ftt` files and saves to them with Save As and a
`.ftt` name. The automation and MCP tools `tree_open` and `tree_save` do the same.

A `.fta` and its `.ped` written to text and read back give the same tree and database, so the
analyses agree. The one change: OpenFTA sometimes wrote a space where a marker is absent, and the text
format keeps only the markers that mean something (dormant `D`, rate `L`). The tests check this for
every OpenFTA example (`crates/core/tests/text.rs`). The writer refuses trees that OpenFTA does not
allow, such as a gate directly under a gate, and ids it cannot write.

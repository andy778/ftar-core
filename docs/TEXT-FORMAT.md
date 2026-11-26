# The text format (`.ftt`)

A fault tree as plain text, for writing by hand, reviewing in a pull request and diffing in git.
One file holds the tree, its events and how they fail over time. An OpenFTA tree needs a `.fta` and
a `.ped` for the same thing.

The format is [PFTA](https://github.com/public-fta/pfta)'s. ftar reads PFTA's files, with its
time-dependent models, distributions, VOTE gates and several top gates, and analyses them as PFTA
does (`ftar time`, below). ftar also writes files that PFTA reads, as long as the tree only uses what
PFTA has. OpenFTA's other gate and event types and its other data are ftar keys on top. PFTA refuses
those.

```text
- times: 0, 1000, 2000, 5000
- time_unit: h
- sample_size: 200

Model: PUMP
- label: Running pump
- model_type: ConstantRate
- failure_rate: lognormal(mu=-9, sigma=0.5)
- mean_repair_time: 24

Gate: NO_FLOW
- label: Too little flow
- type: VOTE(2)
- inputs: P1, P2, P3

Event: P1
- model: PUMP

Event: P2
- model: PUMP

Event: P3
- appearance: Undeveloped
- model_type: ConstantRate
- failure_rate: 1e-4
- repair_rate: 0
```

## Lines and paragraphs

- A paragraph starts with a header, `Gate: ID`, `Event: ID` or `Model: ID`, and goes on with
  `- key: value` lines. A blank line ends it. Each key appears at most once in a paragraph (except
  `note`).
- An optional first paragraph without a header holds the tree properties.
- Lines starting with `#` are comments. Leading spaces are ignored, and so are CRs before the line
  end.
- Values start after the first `:`. Leading spaces are dropped; the rest of the line is kept.
- Ids cannot be empty, cannot contain spaces, `,` or `;`, and cannot be `NULL`. Each id names one
  paragraph. PFTA allows only letters, digits, `_` and `-`.
- Unknown keys are errors, so a typo does not go unnoticed.

ftar reads a little more loosely than PFTA: indented lines, no blank line before a header, and
gate types in any case. It writes what PFTA reads.

## Tree properties

| Key | Meaning |
|---|---|
| `times` | Comma-separated times to quantify at; `nan` means any time. PFTA requires it. ftar writes `1` when a tree has none (OpenFTA's unit time). |
| `time_unit` | Unit shown with intensities and rates. |
| `seed` | Text that seeds the sampling. Without it ftar uses `ftar`, so runs are reproducible; PFTA would pick a random seed. |
| `sample_size` | Samples of the distributions (default 1). |
| `computational_order` | Truncation order of the expansions for gates. 1 is the rare-event approximation. Default: all orders. |
| `computational_tolerance` | Stop an expansion once a term is below this share of the sum, 0 to <1 (default 0). |
| `significant_figures`, `scientific_exponent` | PFTA's drawing options. ftar keeps them and writes them back. |
| `note` | ftar: the tree's note. Repeat the key for more lines. |
| `transfer_out` | ftar: the tree is used elsewhere through a transfer-in under this name. |

## `Gate: ID`

An intermediate event together with its gate. The first gate that no other gate uses is the top
event, shown and edited in the GUI. PFTA allows more top gates. ftar keeps them in the file and
analyses them all with `ftar time`.

| Key | Meaning |
|---|---|
| `label` | Description. |
| `comment` | Comment, kept as written. |
| `is_paged` | `True` to draw the gate on its own page in PFTA (`ftar time` lists it like a top gate). |
| `type` | `AND`, `OR`, `VOTE(k)` (at least k inputs failed) or `NULL` (exactly one input, passed through), or OpenFTA's `XOR`, `INHIBIT` and `PRIORITY_AND`. The OpenFTA types are analysed as OpenFTA does (XOR as OR, the others as AND). ftar also allows no type with any number of inputs, hanging them directly under the intermediate event. |
| `inputs` | Comma-separated ids of gates and events. A gate used by several others is copied into each, so it counts once per use. |
| `gate_id` | ftar: id of the gate symbol itself (OpenFTA gates may have one). |
| `id` | ftar: the intermediate event's own id, when it differs from the paragraph's. The writer uses it when one id names two different subtrees, as in transfer-in trees that reuse names, or when an event already has the id. |
| `unnamed` | ftar: `True` when the intermediate event has no id. The paragraph's id only names it in the file (the writer uses `unnamed_1`, `unnamed_2`, ...). |

The GUI shows a VOTE gate as the OR of ANDs it stands for, an AND or an OR when k is all or one of
the inputs. Saving writes `VOTE(k)` again while that expansion is unchanged.

## `Event: ID`

A leaf of the tree.

| Key | Meaning |
|---|---|
| `label`, `comment` | Description and comment. |
| `appearance` | `Basic` (default), `Undeveloped` or `House`. |
| `model_type` | `Fixed`, `ConstantRate`, `True` or `False`, with the keys below. |
| `model` | Use a `Model:` paragraph instead (not together with `model_type`). |
| `type` | ftar: OpenFTA's `dormant`, `condition`, `condition_not_analysed` or `transfer_in`. For `transfer_in`, the id is the file name of the transferred tree (`.fta` or `.ftt`). Not with `appearance`. |
| `group`, `dormant` | ftar: OpenFTA's group name and dormant marker (`True`). |

`Model: ID` paragraphs take `label`, `comment`, `model_type` and the keys of their type.

### Models

| `model_type` | Keys | Probability `q(t)` and intensity `ω(t)` |
|---|---|---|
| `Fixed` | `probability`, `intensity` | `q = probability`, `ω = intensity`, at every time. |
| `ConstantRate` | `failure_rate` or `mean_failure_time`, and `repair_rate` or `mean_repair_time` | `q(t) = λ/(λ+μ) (1 − e^−(λ+μ)t)`, `ω(t) = λ (1 − q(t))`. An infinite mean repair time is no repair. |
| `True`, `False` | none | Always failed or never failed. Cut sets are simplified with them. |

The rate is `λ(t) = ω/(1 − q)`. ftar also reads two short forms: `probability: p` alone (a fixed
probability with intensity 0) and `failure_rate: λ` alone (no repair, OpenFTA's `L` events). It
writes both forms out in full.

Every value can be a number (`0.01`, `1e-4`, `nan`, `inf`) or a distribution, as in PFTA:
`beta(alpha=, beta=)`, `gamma(alpha=, lambda=)` (shape and rate), `lognormal(mu=, sigma=)` (of the
underlying normal), `loguniform(lower=, upper=)`, `normal(mu=, sigma=)`,
`triangular(lower=, upper=, mode=)` and `uniform(lower=, upper=)`. A sampled probability outside 0..1
or a negative rate, intensity or mean time is an error.

### Events in ftar's tree and database

OpenFTA's analyses (cut sets, `prob`, `mc`, the GUI) read probabilities from the event database.
These models give an entry there:

- a `Fixed` model with a point probability
- a `ConstantRate` model with a point failure rate and no repair, as a rate (`L`)
- `True` (1) and `False` (0)

Models with distributions or repair have no entry, and the OpenFTA analyses report them as missing.
Use `ftar time` for them. The file keeps every model. When an event's probability is changed in the
GUI or with `event_set`, that probability replaces the model on save.

## The analysis: `ftar time`

```sh
ftar time pumps.ftt                         # top gates: q, ω and rate at each time
ftar time pumps.ftt --out pumps.out         # also PFTA's tables as .tsv files
ftar time theatre.fta --times 1,10,100      # OpenFTA trees too
```

Options `--times`, `--samples`, `--seed`, `--order` and `--tolerance` override the file's properties.
The MCP and automation tool is `analysis_time`.

What ftar computes:

- `q`, `ω` and rate of every event, gate and cut set at each time and sample.
- Each gate's minimal cut sets come from ftar's cut-set engine.
- A gate's `q` comes from inclusion-exclusion. Its `ω = ω¹ − ω²` follows the expansion of Vesely's
  kinetic tree theory that PFTA publishes. Both are truncated by `computational_order` and
  `computational_tolerance`.
- A run that would need more than 50 million combinations stops with an error that asks for a
  `computational_order`; it does not run for hours.

`--out DIR` writes PFTA's tables, with PFTA's columns and number style: `models.tsv`, `events.tsv`,
`gates.tsv`, `cut-sets/<gate>.tsv` and `importances/<gate>.tsv`. PFTA's drawings per time are not
written; `ftar draw` draws the tree.

### Importance

For every gate `T` and every event `e` in its cut sets, at each time and sample, ftar computes PFTA's
measures. `T[e=1]` is the gate with `e` always failed, `T[e=0]` the gate with it never failed:

| Measure | Definition |
|---|---|
| marginal (Birnbaum) | `Q(T[e=1]) − Q(T[e=0])` |
| criticality | marginal · `Q(e) / Q(T)` |
| diagnostic (Fussell–Vesely) | `Q(cut sets that contain e) / Q(T)` |
| prognostic | `(Q(T) − Q(T[e=0])) / Q(T)`, which is `1 − 1/RRW` |
| risk achievement worth (RAW) | `Q(T[e=1]) / Q(T)` |
| risk reduction worth (RRW) | `Q(T) / Q(T[e=0])` |

Each `Q` uses the same expansion and truncation as the gate. A division by zero gives `x · ∞` (`inf`,
or `nan` for 0/0), as in PFTA.

`ftar time` prints the measures of each top gate's events at the last time, ordered by criticality,
as means over the samples. `--out` writes them all. The cut-set tables carry PFTA's approximate
importance of each cut set: its share of the summed probabilities and of the summed intensities.
`analysis_time` adds the measures with `"importance": true`.

OpenFTA's own importance (`ftar prob`, the probability report) is unchanged. It is a cut set's
probability summed over the cut sets that contain the event, as a percentage of the top
probability.

Sampling follows PFTA:

- Every event draws its own samples, also when it shares a `Model`.
- Every time gets fresh samples.

ftar's generator is not Python's, so sampled numbers match PFTA's in distribution, not one by one.
Point values give the same numbers. `crates/core/tests/pfta.rs` checks the expansions against brute
force over every state of random trees (exact `q`, and `ω = Σ ω_e (Q(e failed) − Q(e working))`).
It also checks PFTA's worked examples.

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
every OpenFTA example (`crates/core/tests/text.rs`).

A PFTA file read and written again keeps its paragraphs, though their order may change.
`ftar_core::text::pfta_obstacles` names what keeps a file from being a PFTA file. OpenFTA trees
usually have a note and gate ids, which PFTA does not know.

The writer refuses trees that OpenFTA does not allow, such as a gate directly under a gate, and ids it
cannot write.

## Limits

A file may expand to at most 100,000 nodes (`ftar_core::text::MAX_NODES`) and 1,000 levels. Shared
gates and VOTE gates are copied out in the tree, and the limit keeps them from asking for an
exponential one.

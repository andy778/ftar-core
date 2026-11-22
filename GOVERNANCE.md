# Governance

ftar is a personal hobby project with a single maintainer,
[@andy778](https://github.com/andy778), who decides what is merged and released.

## Code review

There is currently no second maintainer, so pull requests are merged without an
independent approving review. Until that changes, these take its place:

- every change goes through a pull request into `master`; direct pushes, force
  pushes and branch deletion are blocked by a repository ruleset;
- the required checks must pass on a branch that is up to date with `master`:
  `cargo xtask ci` on Linux and Windows (tests, clippy, formatting, layering),
  `cargo deny`, CodeQL and ClusterFuzzLite;
- commits on `master` are signed, and all review threads must be resolved;
- behaviour is pinned by tests and compared against the original OpenFTA engine
  ([docs/VERIFICATION.md](docs/VERIFICATION.md)).

The OpenSSF Scorecard *Code-Review* check therefore scores low, and that is
accepted for now. When a second regular contributor joins, the `master`
ruleset will require one approving review from someone other than the author,
and [CODEOWNERS](.github/CODEOWNERS) will list them.

## Becoming a reviewer

Contributors with a few merged pull requests who want to help review are
welcome to say so in an issue.

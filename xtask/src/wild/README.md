# wild-corpus: fmtron against real-world RON

Tasks that collect `.ron` files from public sources, check fmtron against the
reference `ron` crate on every file, and build the redistributable subset in
`test_data/wild/` (attributed in `test_data/wild/manifest.ron`, checked by
`tests/wild_corpus.rs`). Findings from the first run are in
`ADVERSARIAL_FINDINGS_2.md`, round 5.

They are the `wild` tasks of the repository's `xtask` crate, run as
`cargo xtask wild <task>` from the repository root.

All collected data goes to a work directory: `$FMTRON_WILD_WORK`, default
`./wild-work` relative to where you run the tasks. Run them from one
directory. Only `build-corpus` writes into the repository.

## Pipeline

```sh
# 1. Collect (each is resumable and skips what it already has).
cargo xtask wild gh-collect coarse 4 1 200 400 600 800 1100 1500 2000 3000 5000 8500 15000 30000 65537
cargo xtask wild gh-download            # fetch the indexed GitHub files + licenses
cargo xtask wild crates-collect 40      # pages of ron reverse dependencies on crates.io
cargo xtask wild forge-collect gitlab   # GitLab.com project search
cargo xtask wild forge-collect codeberg # Codeberg repository search (narrow)
cargo xtask wild codeberg-collect       # Codeberg, many keywords (broad)

# 2. Check fmtron on every unique file.
cargo xtask wild evaluate               # writes results.jsonl, prints a summary

# 3. Rebuild test_data/wild/ from the results.
cargo build --release
cargo xtask wild build-corpus
```

`gh-collect` needs an authenticated `gh` CLI. GitHub code search allows
10 requests per minute and 1,000 results per query, so it samples by file
size: `coarse <pages> <bounds…>` takes up to `pages × 100` results from each
size range, and `<lo> <hi> <pages>` bisects a range until every slice is
under the cap. Bisecting from 1 byte upward spends its budget on tiny files,
so `coarse` gives a more representative sample. `gh-download` can run
while indexing continues; run it again at the end.

`evaluate` formats every file at the default settings and checks the
output against the `ron` crate (`xtask/src/oracle.rs`, shared with
`cargo xtask matrix-audit`), recording one status per file:

| Status | Meaning |
|---|---|
| `ok` | output accepted by `ron`, same value, idempotent, same tokens in order |
| `both-reject` | fmtron and `ron` both reject it (usually not RON) |
| `rejects-valid` | fmtron rejects a file `ron` accepts |
| `accepts-invalid` | fmtron formats a file `ron` rejects |
| `semantic` / `output-invalid` | the formatted value differs, or is no longer valid |
| `not-idempotent` | formatting the output again changes it |
| `content-changed` | a non-whitespace, non-comma character moved, appeared or vanished (comment moves show up here) |
| `too-deep`, `panic`, `not-utf8` | as named |

`evaluate` also marks `symlink-stub` files: GitHub's raw endpoint returns
a symlink's target path instead of the file.

## Comparing styles

`cargo xtask compare-styles` formats a corpus with several fmtron builds and
compares them: how far each one's output is from the input, how many files it
collapses or expands, and how many differ from the first build. It writes each
build's output for diffing. `PLAN_1_0.md` has the numbers behind the 1.0 style.

Over the last release:

```sh
git worktree add /tmp/fmtron-old v0.10.0
cargo build --release --manifest-path /tmp/fmtron-old/Cargo.toml
cargo build --release
cargo xtask compare-styles style-out \
    old=/tmp/fmtron-old/target/release/fmtron new=target/release/fmtron \
    -- test_data/wild test_data/corpus
```

## Corpus selection

`build-corpus` keeps files that are unique by content, accepted by `ron`
or fmtron, at most 32 KiB, and from a source that declares a recognized
open-source license, with at most 8 files per source. Codeberg's API reports
no license, so Codeberg files are only used for local evaluation.

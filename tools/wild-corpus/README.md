# wild-corpus: fmtron against real-world RON

Scripts that collect `.ron` files from public sources, check fmtron against
the reference `ron` crate on every file, and build the redistributable
subset in `test_data/wild/` (attributed in `test_data/wild/manifest.ron`,
checked by `tests/wild_corpus.rs`). Findings from the first run are in
`ADVERSARIAL_FINDINGS_2.md`, round 5.

All collected data goes to a work directory: `$FMTRON_WILD_WORK`, default
`./wild-work` relative to where you run the scripts. Run them from one
directory. Only `build_corpus.py` writes into the repository.

## Pipeline

```sh
# 1. Collect (each is resumable and skips what it already has).
python3 gh_collect.py coarse 4 1 200 400 600 800 1100 1500 2000 3000 5000 8500 15000 30000 65537
python3 gh_download.py            # fetch the indexed GitHub files + licenses
python3 crates_collect.py 40      # pages of ron reverse dependencies on crates.io
python3 forge_collect.py gitlab   # GitLab.com project search
python3 forge_collect.py codeberg # Codeberg repository search (narrow)
python3 codeberg_collect.py       # Codeberg, many keywords (broad)

# 2. Check fmtron on every unique file.
cargo build --release --manifest-path harness/Cargo.toml
python3 evaluate.py               # writes results.jsonl, prints a summary

# 3. Rebuild test_data/wild/ from the results.
cargo build --release --manifest-path ../../Cargo.toml
python3 build_corpus.py
```

`gh_collect.py` needs an authenticated `gh` CLI. GitHub code search allows
10 requests per minute and 1,000 results per query, so it samples by file
size: `coarse <pages> <bounds…>` takes up to `pages × 100` results from each
size range, and `<lo> <hi> <pages>` bisects a range until every slice is
under the cap. Bisecting from 1 byte upward spends its budget on tiny files,
so `coarse` gives a more representative sample. `gh_download.py` can run
while indexing continues; run it again at the end.

## Checks

`harness/` runs one file per input line and reports a status per file:

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

`evaluate.py` also marks `symlink-stub` files: GitHub's raw endpoint returns
a symlink's target path instead of the file.

## Corpus selection

`build_corpus.py` keeps files that are unique by content, accepted by `ron`
or fmtron, at most 32 KiB, and from a source that declares a recognized
open-source license, with at most 8 files per source. Codeberg's API reports
no license, so Codeberg files are only used for local evaluation.

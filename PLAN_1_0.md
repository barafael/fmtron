# Road to 1.0

Goal: release fmtron 1.0 with a style its users can rely on. Within 1.x,
output only changes for bug fixes. That makes every layout rule effectively
permanent, so the rules have to be settled first. Everything else (stability
policy, API, CLI) is routine once they are.

## Phases

| # | Phase | Effort | Deliverable |
|---|---|---|---|
| 0 | Measure the candidates | S (~1d) | corpus numbers + sample diffs for each candidate rule — **done**, see Results |
| 1 | **Decide** D1–D3 | (you) | one line per decision, recorded below — **done** |
| 2 | Implement the chosen style | M (2–3d) | rules landed, showcase extended, snapshots re-blessed — **done** (uncommitted) |
| 3 | Soak as 1.0.0-rc.N | 2–4 weeks | real users on the new style before it is frozen — **rc.1 published 2026-10-06** |
| 4 | Stability contract | S (~0.5d) | STABILITY.md; `style_edition` in `fmt.ron` and CLI — **done** |
| 5 | Public API for 1.0 | S (~0.5d) | minimal, `#[non_exhaustive]`, no pest types exposed — **done** |
| 6 | CLI and config contract | S | flags and `fmt.ron` fields reviewed; `rust-version` set — **done** (`--max-tab` kept: it is a resource limit, like `--max-depth`) |
| 7 | Release 1.0.0 | S | CHANGELOG.md started; tag, publish |

Phase 3 can overlap with 4–6; they do not change output.

---

## The decisions

### D1 — When does a container go on one line?

Today: any container that fits in `max_width` (100) is joined onto one line.

```
(                              →   (name: "server", port: 8080, debug: false)
    name: "server",
    port: 8080,
    debug: false,
)
```

Baseline: 503 of 3075 real-world files (16%) shrink to less than half their
line count. Hand-written configs and `ron::ser::to_string_pretty` output,
which puts every field on its own line, get collapsed into dense lines like
`level2: (level3: (level4: (…))))`.

| Option | Rule | For | Against |
|---|---|---|---|
| A | Keep: width decides everything | canonical; already pinned | the collapsing above, permanently |
| B | **Preserve the author's break** (Prettier's rule for objects): a named-field struct or map with a line break between its opening bracket and its first item stays broken. Otherwise width decides | the author keeps control; configs stay vertical; points like `(x: 1, y: 2)` stay flat | output depends on input layout (one-way: fmtron never joins a broken struct, though the author still can). Conflicts with the documented promise of `--blank-lines remove` (see below) |
| C | **Narrower width for records** (rustfmt's `struct_lit_width`): a named-field struct or map goes flat only if it is at most N columns wide (rustfmt: 18) | canonical; deterministic | a single N fits nobody: `(x: 1.5, y: -3.25)` is 18 columns, `(r: 255, g: 255, b: 255)` is 24 |

Scope for B and C: named-field structs (`FieldsType`) and maps only. Lists and
tuples stay width-only. Otherwise ron's pretty output (one list element per
line) could never be joined. D2 handles long lists.

For B, `BlankLines::Remove` promises output that "depends only on the input's
tokens and comments, never on its layout". Either `remove` also ignores
author breaks (consistent, but `remove` then means two things), or the promise
is reworded to cover blank lines only. I'd reword it.

Implementation cost: small either way. B records whether the input had a line
break after the opener (the parser already has `newline_between`) and feeds it
into `force_break` in `src/ast/display.rs`. C passes a second width to
`group` for records.

**Recommendation: B.** RON is mostly hand-edited config and game data, where
vertical structs are the norm and authors know which ones should stay
compact. B gives that; C can only guess it from width.

### D2 — How are long lists of short items laid out?

Today: a list that does not fit gets one element per line. A 4-line tank-game
map (`walls: [(0, 0), (1, 0), …]`) becomes 510 lines. 120 of 3075 files (4%)
at least double in length.

| Option | Rule | For | Against |
|---|---|---|---|
| A | Keep: one per line | simplest; inserting an element is a one-line diff | coordinate lists, tile maps, vertex data explode |
| B | **Fill** (rustfmt's `short_array_element_width_threshold`, default 10): when *every* element is short (flat width ≤ T) and none has comments, pack as many per line as fit | data stays dense and scannable | inserting an element reflows the rest of the list (larger diffs); needs a `Fill` combinator in `src/pretty.rs` |

Sub-questions for B, which Phase 0 answers with numbers:

- Which elements count as short: atoms only (numbers, chars, bools), or also
  flat tuples like `(31, 0)`? Without tuples, the tank map is not helped.
- Threshold T: 10 (rustfmt) vs. something larger.
- Does a blank line in the list (kept under `Keep`) split it into separately
  filled runs? It should: blank lines are how authors group rows.

**Recommendation: B, with atoms and flat atom-only tuples, T = 10.** Data
files are a core RON use case, and with this scope, prose-like lists (strings,
structs) keep one element per line.

### D3 — Drop the "break-around" layout for single wrapped containers

Today `Some([…])` has three layouts: flat; `Some(` / list on its own line /
`)`; and hugged `Some([` … `])`. The middle one only applies in a narrow
range of widths, so one added element can move a value through all three.

Option A keeps it. Option B goes straight from flat to hugged, the layout
`ron`'s pretty-printer uses.

**Recommendation: B.** Fewer states means less churn. It is also simpler code
(`Doc::Hug` loses a branch).

### Decisions

```
D1: B — a struct or map broken after its opening bracket stays broken.
D2: B, refined by Phase 0 — pack lists of short elements (atom or tuple of
    atoms, ≤ 16 columns). Keep the input's line breaks between them if it
    already packs them; else pack only numbers, bools and chars (and tuples
    of those); strings and identifiers written one per line stay so.
D3: B — no break-around; wrappers hug.
```

### Results (Phase 0)

3089 files of `test_data/wild` and `test_data/corpus`, 253,605 input lines.
Churn: lines removed plus added between input and output.

| Variant | Churn | Halved | Doubled | Files changed |
|---|---|---|---|---|
| 0.10 | 225,384 | 537 | 69 | — |
| D1 author breaks | 197,804 | 38 | 69 | 1738 |
| D3 hug only | 224,252 | 537 | 69 | 121 |
| D1 + D3 | 197,791 | 38 | 69 | 1750 |
| D1 + D3 + fill by width, threshold 10 | 188,492 | 42 | 64 | 1767 |
| D1 + D3 + fill keeping every input break | 189,483 | 38 | 64 | 1764 |
| D1 + D3 + fill keeping breaks of packed input | 188,524 | 42 | 64 | 1767 |
| **D1 + D3 + chosen fill, threshold 16** | **186,450** | **41** | **62** | **1772** |

Chosen fill at threshold 10, 12, 16, 20: churn 188,361 / 186,710 / 186,419 /
187,063. 16 also keeps author-packed keyword lists (`"authenticate",
"authentication", …`) and packs 2D float points (`(1000.0, 140.0)`).

Findings that shaped D2:

- **Fill by width alone destroys grids.** A 16-wide tile map is reflowed into
  rows of 31; wgpu byte arrays lose their groups of 4; register tables lose
  their 8 per line. Keeping the input's line breaks in a packed list fixes
  all of them.
- **Edit churn.** Inserting one element into a packed line of a formatted file
  (28 corpus files): with input breaks kept, at most 3 changed lines (median
  2); with fill by width alone, up to 699.
- **Strings are not data.** Packing one-per-line lists of names (`"grass",
  "conveyor", "deep_snow"`, `Text(" "), CurrentElapsed`) hurts: those lists
  are one per line on purpose and grow by insertion. Numbers written one per
  line (`ron`'s pretty output of a `Vec<f32>`) pack well.
- **Fill helps fewer files than expected** (about 40 change). Most files that
  still grow a lot are hand-packed lines over 100 columns, which any
  formatter breaks. Keyboard layouts (`16: 'A', 17: 'Z', …`) would need
  packing for maps; not pursued.

Found on the way: hugging without break-around overran the width by the comma
after a wrapper (`Some((…)),` exactly at the limit). Fixed: a hugged child is
measured with what follows it on its line.

### What Phase 2 changed

- `src/ast`: structs and maps record a line break after their opener; list
  elements record whether they start a line; atoms record whether they are
  scalars. `src/pretty.rs` gains `fill` and loses the break-around layout.
- README "Style" section; the `BlankLines::Remove` docs no longer promise
  layout independence.
- `tests/unformat.rs` scrambles every gap except those fmtron now reads (after
  `(` and `{`, around commas in lists); the reference printer in
  `tests/support` packs lists too. Showcase gains a "Line breaks kept from the
  input" section; snapshots re-blessed.
- `tools/wild-corpus/compare_styles.py` reproduces the table above for any two
  builds.

---

### Review round (after Phase 2)

About 30 hand-written files (configs, bevy scenes, item tables, levels,
dialogue trees, animations, state machines, CI pipelines, `ron`'s pretty
output, comment-heavy and Unicode files, CRLF, tabs, narrow widths) were
formatted and judged by eye, and a random generator of packed lists and
tuples (49,000 documents) checked idempotency and width. Changes:

- **R1: tuples are packed like lists.** serde writes arrays (`[u8; 32]`,
  `[f32; 16]`) as tuples; a 32-byte key took 34 lines.
- **R2: grids.** A packed list or tuple broken right after its opening
  bracket stays broken, keeping its rows, even if it would fit: a 3×3
  matrix written as rows no longer collapses onto one line. Without the
  break after the bracket, a short list with stray line breaks still joins.
  Same signal as D1.
- **R3: numbers, bools and chars are short at any width.** One 20-digit id
  made a whole number list one-per-line (and adding it to a packed list
  exploded the list).
- **Bugs fixed:** a grid inside a hug lost its rows and trailing comma; a
  packed element too wide for any line overran the width, and its next
  sibling continued on its closing line; kept line breaks that left every
  element on its own line made the output not idempotent. `tests/style.rs`
  pins each rule and these cases; `tests/fuzz.rs` now generates multi-line
  layouts (line breaks after brackets, rows, blank lines), so it reaches
  these code paths.

Corpus after the round: churn 184,879 (0.10: 225,384), halved 41, doubled
60; 1779 files differ from 0.10.

Judged and accepted as is:

- Siblings in a list of records can mix one-line and broken layouts, each
  decided by width (as in Prettier). An author who wants them uniform breaks
  one after its bracket, and it stays.
- A comment in a list puts every element on its own line. Annotated packed
  rows (`1, 2, 3, // row 1`) would be nicer, but the corpus has none.
- Maps are never packed (keyboard layouts like `16: 'A', 17: 'Z', …` explode).
  Rare; a candidate for a later style edition.

## Phase 0 — Measure the candidates (done)

Build the candidates behind a temporary, undocumented switch (an env var read
in `format_ron` is enough). They never ship as options. Then, for every
candidate combination over `test_data/wild` and `test_data/corpus`:

1. **Adoption churn:** changed lines between input and output, summed and per
   file. Lower means closer to how people already write RON. This is the main
   objective number.
2. **Shape:** the halved/doubled counts above (503 / 120 today).
3. **Edit churn (D2):** for 50 sampled lists, insert one element in the middle,
   format, and count changed lines. This makes fill's cost visible.
4. **Correctness:** idempotency and oracle equivalence on every file, as
   `tests/wild_corpus.rs` already checks.
5. **Eyeball:** 20 random diffs per candidate written to the scratchpad. Read
   them; the numbers only shortlist.

Put this in `tools/wild-corpus/` next to `evaluate.py`, or as an ignored test.
Keep it after 1.0: it is how a future style edition would be evaluated.

## Phase 2 — Implement

- Land the chosen rules. Remove the Phase 0 switch.
- Extend `test_data/stability/showcase.ron` with a case for every rule
  boundary: an author-broken struct, a flat point, a fill list split by a blank
  line, a list just over the short threshold, a hug at each width.
- Re-bless the snapshots (`FMTRON_BLESS=1 cargo test --test stability`) and
  review the diff as a user would.
- README: describe the layout rules in a short "Style" section. Today it only
  describes features.

## Phase 3 — Soak as 0.11.0

Release the new style as 0.11.0 and say in the release notes that this is the
1.0 candidate style. Run it on a few real repositories (bevy examples, a COSMIC
theme repo from the corpus) and fix what comes up. A rule change is still
free here; after 1.0 it is not.

## Phases 4–6 — done

What was decided while doing them, beyond the plan below:

- The public API was shaped around embedding fmtron in a program such as a
  web page that formats RON: `format_ron`, a `Config` built from
  `Config::default()` (`#[non_exhaustive]`, with `with_*` builders), and a
  `ParseError` with `line()`, `column()`, `offset()`, `message()` and
  `with_path()`. `pest` and `ron` types are gone from the API, so those
  dependencies can be upgraded in minor releases.
- The binary is behind a default `cli` feature; the library alone builds for
  `wasm32-unknown-unknown`, checked in CI.
- The reference printer that cross-checks the formatter lives in the crate
  (`src/reference.rs`, test-only), so the `pretty` module and the parser are
  private.
- `rust-version = "1.88"` (let chains), checked in CI; raising it is a minor
  change.
- `--max-tab` stays: it is a resource limit like `--max-depth`, which a
  program formatting untrusted input wants.

## Phase 4 — Stability contract

- README "Stability" section: within 1.x, formatting already-formatted input
  produces the same output, with the same config. Exceptions are bug fixes
  only: output that is invalid RON, changes meaning, loses or moves a comment
  to a different element, is not idempotent, or panics. Bug fixes that change
  output are called out in the release notes.
- `style_edition` in `fmt.ron` and `--style-edition` on the CLI. Accept only
  `2026` for now; reject any other value. Future style changes ship as a new
  edition, opt-in, and become the default only in 2.0 (rustfmt's model).
  Adding the field now costs nothing. Adding it later is what forced
  rustfmt's `version = Two` workaround.
- `tests/stability.rs` pins output per edition.

## Phase 5 — Public API

1.0 also freezes the library's semver. Today the crate exposes:

- `pub mod pretty` (the whole `Doc` algebra), `RonParser`, `pub use
  pest::Parser`, and the AST (`Kind`, `RonFile`, `Value`). Only the
  reference printer in `tests/support/pretty.rs` (used by `wadler_advantage`
  and `fuzz`) uses `pretty`, `RonParser` and `Rule`; it can move into the
  crate as a `#[cfg(test)]` module, and the rest become `pub(crate)`.
- `FormatError::Parse(Box<pest::error::Error<Rule>>)` puts pest 2 in the
  public API. Replace it with an owned error type (message, line, column,
  rendered snippet), which `parse_error.rs` already produces.
- `Config` has only public fields and no `#[non_exhaustive]`, so adding
  `style_edition` (Phase 4) would itself be a breaking change after 1.0. Mark
  `Config`, `FormatError`, `BlankLines` and `FileConfigError`
  `#[non_exhaustive]`, and give `Config` a builder or keep `Default` +
  field updates.
- Decide whether `MAX_INDENT`, `MAX_TAB`, `line_ending` and the
  `FileConfig` methods are public on purpose.

Keep: `format_ron`, `Config`, `BlankLines`, `FormatError`, `FileConfig` and
`CONFIG_FILE_NAMES` (for editor integrations).

## Phase 6 — CLI and config contract

- Review each flag for "will I support this forever". `--max-tab` is the
  doubtful one: its only purpose is to raise a safety limit on `-t`. Folding it
  into `--max-depth`/`MAX_INDENT`, or hiding it, simplifies the contract.
- `fmt.ron`: same review for its fields. Unknown fields are already errors, so
  removing a field later breaks configs. Decide now.
- Set `rust-version` in `Cargo.toml` (MSRV) and say whether raising it counts
  as breaking (common practice: no, for a binary).

## Decided before 1.0.0

- **Empty input is a no-op for the command line.** A file that is empty or
  holds only comments and attributes is left as it is, exit 0, no message;
  the library keeps returning `FormatError::Empty`. A formatter has no say
  on whether a file ought to hold a value; the program loading it does, and
  `ron` says so clearly. The deciding case is format-on-save on a new file,
  which otherwise errors on every save until the first value; `--check` as
  a lint for empty data files was the counter-argument and does not hold,
  since `--check` asks whether a file is formatted, which an empty file is.

## Phase 7 — Release

- Add `CHANGELOG.md`, starting at 1.0.0, with a summary of 0.x.
- Version 1.0.0, tag, `cargo publish`.

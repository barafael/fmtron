# fmtron adversarial audit, round 2

Audited **fmtron 0.7.0** (commit `4345c15`, after the round-1 fixes in
`ADVERSARIAL_FINDINGS.md`). Correctness oracle: the `ron` crate **0.12**
(`ron::from_str::<ron::Value>`).

Method: ~120 hand-written hostile inputs through the CLI; a randomized
generator (5,500 documents) that puts whitespace, newlines, `/* */` and `//`
comments at *every* token boundary, including inside `#![...]` attributes. Each
run checked for panics, rejection of valid input, semantic drift vs. the oracle,
lost comments, and idempotency. Plus resource probes and CLI I/O edge cases.

Once attributes were left out, the generator found nothing new in value bodies:
comment attachment, semantics and idempotency held across 4,000 documents. The
bugs cluster around attributes, keyword prefixes, the depth guard and literal
coverage.

## Summary

| # | Severity | Finding | Type |
|---|----------|---------|------|
| N1 | **High** | A comment between attribute tokens (`# /*x*/ ![…]`, `#![/*x*/ enable(…)]`) panics: `entered unreachable code` at `src/ast/mod.rs:80` | panic |
| N2 | **High** | A comment inside `#![type = …]` / `#![schema = …]` *replaces* the string: `#![type = /*c*/ "x"]` → `#![type = /*c*/]` | data loss, invalid output |
| N3 | **Medium** | A comment inside `enable(…)` is printed as an extension name: `enable(/* c */ implicit_some)` → `enable(/* c */, implicit_some)` | invalid output |
| N4 | **Medium** | About 60k nested block comments (`/*/*/*…*/*/*/`, 120 KB) overflow the stack and abort, because the depth guard ignores comments | crash (SIGABRT) |
| N5 | **Medium** | `true`/`false` shadow longer identifiers: `trueish`, `(a: false_x)`, `{truex: 1}` are rejected | rejects valid |
| N6 | Low | Signed special floats with a suffix (`-NaNf32`, `+inff32`) are rejected. `NaNf32` only works because it parses as an identifier | rejects valid |
| N7 | Low | Byte literals `b'a'`, `b'\x80'`, `b'\n'` are rejected | rejects valid |
| N8 | Low | In-place output has no final newline, except when trailing comments exist; `-d` then prints a blank line | inconsistent output |
| N9 | Low | Comments before or between attributes move below all attributes, e.g. a `// license` header. A same-line `#![…] // why` is split onto its own line | layout / comment order |
| N10 | Low | A lone `\r` ends a `//` comment in fmtron but not in `ron`, so `[1 // c\r, 2]` is accepted and printed as valid RON with a different structure | accepts invalid |
| N11 | Low | `'\x80'` is accepted, but `ron` limits a char's `\x` to `00-7F`, so the output is invalid. Strings may chain `\xHH` into valid UTF-8 (`"\xc3\xa9"`, which the corpus uses). Also accepted: `\u{D800}`, `\u{110000}`, `256u8` | accepts invalid |
| N12 | Low | Amplification within configured bounds: depth-512 `[[[…]]]` with `-t 1024 -w 0` turns 1 KB into 267 MB | resource |
| N13 | Nit | `( )` is normalized to `()`. `ron::Value` reads these differently (empty seq vs unit), but the rewrite only makes a typed parse that failed succeed | normalization |
| N14 | Nit | If the in-place write fails (read-only file), the `.bak` copy is left behind | UX |

## Evidence

```
$ printf '# /*x*/ ![enable(implicit_some)]\n5' > a.ron && fmtron -i a.ron -d
thread 'main' panicked at src/ast/mod.rs:80:18:
internal error: entered unreachable code                        # exit 101   (N1)

$ printf '#![type = /*c*/ "x"]\n5' > a.ron && fmtron -i a.ron -d
#![type = /*c*/]                                                # "x" lost    (N2)

$ printf '#![enable(/* c */ implicit_some)]\n5' > a.ron && fmtron -i a.ron -d
#![enable(/* c */, implicit_some)]                              # ron: error  (N3)

$ python3 -c "print('/*'*60000 + '*/'*60000 + '5')" > a.ron && fmtron -i a.ron -d
thread 'main' has overflowed its stack                          # exit 134    (N4)

$ echo 'trueish' > a.ron && fmtron -i a.ron -d                  # ron: Ok(Unit)  (N5)
  = expected EOI or COMMENT
```

Root causes:

- **N1–N3:** `Attribute::from` (`src/ast/mod.rs:74`) assumes attribute pairs
  contain only grammar children. pest's implicit `COMMENT` shows up as a sibling:
  it hits `unreachable!()`, is read as the attribute string by `.next()`, or is
  collected as an ident.
- **N4:** `nesting_depth` (`src/lib.rs`) skips block comments without counting
  their nesting, while pest's `block_comment` recurses once per level.
- **N5/N6:** `bool` has no identifier-boundary lookahead. `special_float` checks
  `!ident_rest` *before* the optional suffix, so a suffix can never match.
- **N10:** `line_comment` ends at `NEWLINE`, which includes a lone `\r`.

## Tested and passed

- Comments at every token boundary inside values (lists, maps, tuples, structs,
  keys, between an identifier and `(`, between a key and `:`): no lost comments,
  no semantic drift, idempotent (4,000 random docs, widths 1–200).
- Scale: 1M-element flat list 2.0 s; 200k line comments 0.7 s; 5 MB string;
  200k nested items at `-w 10^9`; depth 500 × 20k items.
- `-w 0`, `-w usize::MAX`, `-t 0`, `--max-depth 0`, `/dev/null`, non-UTF-8
  input, read-only target: all clean errors or sane output.

## Disposition

Fixed (regression tests in `tests/adversarial.rs` and `tests/cli.rs`):

- **N1–N3:** an attribute that contains a comment is printed verbatim
  (`Attribute::Verbatim`). Comment-free attributes are still normalized.
  Trade-off: a verbatim attribute keeps its source layout, so it is not
  layout-canonical.
- **N4:** block-comment nesting counts toward `max_nesting`.
- **N5/N6:** `bool` has an identifier-boundary lookahead, and `special_float`
  checks it *after* the optional suffix.
- **N7:** new `byte_char` rule: an ASCII character or a byte escape, no `\u`.
- **N8:** the CLI always writes exactly one final newline, in both modes.
- **N9 (partly):** header comments keep their order relative to attributes.
  A same-line `#![…] // why` still moves to its own line, because output must
  not depend on source layout (`tests/unformat.rs` canonicality harness).
- **N10:** only `\n` ends a line comment.
- **N11 (partly):** a char's `\x` escape is limited to `00-7F`. UTF-8 validity
  of `\x` chains in strings, `\u{…}` range and integer range are value checks
  and stay out of scope for a formatter.

Won't fix: N12 (the user asked for those bounds explicitly), N13 (a harmless
normalization), N14 (the backup is useful when a write fails).

---

# Round 3

Method: a scratch crate links fmtron and `ron` 0.12 in-process, which makes
bulk differential runs fast:

- 600,000 random literals (numbers, strings/chars, identifiers).
- A depth-guard evasion fuzzer: 70,000 random quote/comment/raw-string
  prefixes, each followed by 4,000 nested brackets.
- Backtracking probes on 11 nested shapes up to 800 levels deep, each failing
  at the innermost point.
- Error-message probes on hostile input.

`docs/grammar.md` from the `ron` crate is the tiebreaker where the spec and
the implementation disagree.

| # | Severity | Finding | Type |
|---|----------|---------|------|
| R1 | Medium | Identifiers used "any non-ASCII" instead of XID: `·a` and `١a` were accepted, giving invalid output. Raw identifiers `r#0`, `r#a.b`, `r#a+b-c`, `Foo(r#1: 2)` were rejected | accepts invalid / rejects valid |
| R2 | Low | `'''`, `b'''` and a char holding a raw `\n`/`\r` were rejected. `ron` accepts all four (the spec says `no_apostrophe`; the implementation is lenient) | rejects valid |
| R3 | Medium (latent) | The depth guard's `scan_char` skipped from any `'` to the *next* `'`, however far. Once R2 made `'''` valid, `[''', <600-deep>, 'x']` hid the nesting from the guard while pest recursed into it: a guard bypass leading to a stack overflow | guard bypass |
| R4 | Low | Blank, comment-only and attribute-only files said `expected ron_file`. `FormatError::Empty` was unreachable | UX |
| R5 | Low | One bad byte at the end of a 4 MB single-line file printed about 8 MB to stderr, because pest echoes the line padded out to the caret | UX / resource |
| R6 | Nit | The depth error was labelled "unable to parse RON" and suggested `max_nesting`, a library field; the CLI flag is `--max-depth` | UX |

Fixes:

- **R1:** `ident = "r#" ident_raw_rest+ | ident_first ident_rest*`, using pest's
  `XID_START`/`XID_CONTINUE`. The `bool`/`inf`/`NaN` boundary lookaheads use
  `XID_CONTINUE` too, as `ron` does.
- **R2:** `char_inner` and `byte_char_inner` only exclude `\`.
- **R3:** `scan_char` skips only a well-formed literal: one UTF-8 char, or an
  escape of at most 10 bytes, then `'`. Otherwise the `'` is ordinary input.
  The regression test fails against the old scanner.
- **R4:** a `no_value` rule is tried when `ron_file` fails; if it matches,
  the result is `Empty`.
- **R5:** lines over 200 chars show an 80-char excerpt around the column, in
  pest's own layout.
- **R6:** the CLI reports `input is nested N levels deep, exceeding the limit
  of 512; raise the limit with --max-depth`.

Tested and clean:

- The random literal and structural fuzz (5,000 more docs with the new atoms,
  attributes on) shows no crashes, drift, lost comments or idempotency breaks.
- Depth-guard evasion: no prefix gets real nesting past the guard. The only
  inputs accepted with 4,000 brackets had them inside a string or a `//`
  comment.
- Backtracking stays linear on all 11 shapes (≤35 ms at 800 levels). pest's
  `tuple_type`/`fields_type` alternation never re-parses a subtree more than
  once.

Out of scope / not changed:

- `ron::Value` also parses Rust range syntax (`1..2`, `..5`, `..`). It is not
  in `docs/grammar.md`, so fmtron keeps rejecting it.
- Integer range (`-1u8`, `260i8`), UTF-8 validity of `\xHH` chains in strings,
  and bare `Some` (a round-1 decision) are unchanged.
- pest still lists the implicit `COMMENT` rule in messages
  (`expected COMMENT or value` when `,`/`]` is meant). pest's
  `set_error_detail` yields real expected tokens, but it is a process-global
  switch and made parsing about 2.5× slower in a probe. Not adopted.

---

# Round 4: formatter output and CLI robustness

With `ron` syntax coverage settled, this round targeted fmtron's own output:
layout invariants, line endings, and the CLI's configurable limits. A layout
checker formatted 8,000 random comment-free documents at widths 8/20/40/80
and flagged three things: lines over the width that hold more than one
element, broken containers whose flat form would fit on their line, and
trailing whitespace.

| # | Severity | Finding | Type |
|---|----------|---------|------|
| M1 | Medium | A container map key stayed flat even when its line overran the width, e.g. `Foo(3.5, 1e10): {` at 22 columns with width 20. This was all 179 layout violations found. `fits_rest` counted any following group (the value) as zero width, so `: Foo(` was never charged to the key's line | layout |
| M2 | Medium | CRLF files came out LF, except inside block comments, which kept `\r\n`: mixed line endings | output corruption |
| M3 | Medium | Raising `--max-tab` re-opened round-1 F2: `--max-tab <huge> -t <huge>` panicked with `capacity overflow` (or aborted on allocation), and `indent + n` could wrap | panic |
| M4 | Medium | Raising `--max-depth` let input through that overflowed the stack anyway: `--max-depth 30000` on 20,000-deep input aborted, since the main thread's 8 MiB holds only about 3,000 levels | crash |

Fixes:

- **M1:** a following group is measured in *break* mode, up to its first
  possible line break. `key: Foo(` must fit, but a large value still cannot
  force a small key to break (the existing test for that still passes). The
  checker now reports no violations at all.
- **M2:** `fmtron::line_ending(input)` detects CRLF from the first line break.
  The printer and header/trailer rendering emit that line ending, and the CLI's
  final newline matches it. Line breaks inside strings and comments are never
  rewritten; a raw `\n` in a string of a CRLF file stays `\n`.
- **M3:** new `FormatError::IndentTooWide` when `tab_size × depth` exceeds
  `MAX_INDENT` (1 Mi columns). The CLI says to use a smaller `--tab-size`.
  Nest arithmetic saturates.
- **M4:** the CLI formats on a thread with a
  `1 MiB + 16 KiB × --max-depth` stack. Measured use is about 2.7 KiB per
  level in release and 9.8 KiB in debug, across 8 shapes. An unreservable size
  is a clean `invalid configuration` error.

Checked and fine: tabs in strings count as wide and zero-width characters as
zero, so neither causes wrong breaks. Output stays proportional to
`depth² × tab` when the user explicitly raises `--max-depth`, which is
expected.

---

# Round 5: real-world RON from the wild

fmtron ran over RON files collected from public sources:

- **GitHub:** code search for `extension:ron`, sampled across file sizes.
- **GitLab.com and Codeberg:** project search, then repository trees.
- **crates.io:** `.ron` files inside the published `.crate` archives of about
  4,000 crates that depend on `ron`.

Result: 15,603 files downloaded, **8,306 unique by content, from 2,555
repositories and crates**. Every file got the same checks as before:
acceptance vs. the `ron` oracle, value preservation, validity of the output,
idempotency, and a strict content check (all non-whitespace, non-comma
characters must survive in order).

| Outcome | Files |
|---|---|
| Formatted faithfully | 7,942 |
| Rejected by both fmtron and `ron` | 294 |
| Git symlinks (the raw endpoint returns the link target, e.g. `../../x.ron`) | 53 |
| Comment moved (see W1) | 9 |
| Accepted by fmtron, rejected by `ron::Value` | 4 |
| Not UTF-8 | 4 |

There were **no crashes, no valid files rejected, no value changes, no
invalid output and no idempotency failures** across the 8,306 files.

The 294 both-rejects are not RON. That includes other formats named `.ron`
(Helsinki-NLP's Romanian text, `starfederation/ron`), Git LFS pointer files,
Rust `Debug` dumps, gitui themes with `$var` templates or shell `#` comments,
templates with code after the value, intentionally invalid test fixtures,
and 5 files with a UTF-8 BOM (`ron` rejects those too). The accepts-invalid
cases are one fmtron fixture with an unknown extension, plus 3 Fyrox editor
files that write `Some(code: KeyZ, …)`, which a typed `ron` target reads but
`ron::Value` cannot.

## Fixed

| # | Kind | Finding | Fix |
|---|---|---|---|
| W1 | misformat | A comment between a key/field name and its value was mishandled. A field's `delay: /* seconds */ 5` became `/* seconds */` on its own line above the field. A map's `"delay": /* seconds */ 5` was split after the comment, with the value on the next line at the key's indentation | Block comments there stay inline after the colon (`delay: /* seconds */ 5`, also for `a /* c */ : 1`). Line comments move above the entry (`// Bb2` over `46: [[…]]`), the same for fields and maps. Stored as `Value::inline` so the public `Kind` API is unchanged |
| W2 | misformat | One-element wrappers around a container broke in two levels: `Some(` / `(` / fields / `),` / `),`. RON writes `Option<Struct>` and newtypes as `Some((` … `))`, which `ron`'s own pretty-printer emits | New `pretty::hug` primitive. Flat if it all fits; if the child fits flat on its own line, break around it (`TupleNewtypeTupleStruct(` / `TupleStruct(4, false),` / `)`); only if the child must break anyway, hug: `Some((` … `))`, `Some(((` … `)))`, `Some(Some([` … `]))`. Atoms never hug. The layout-independence harness still passes, and one golden file (`ron_039`) now matches its author's `Some((` |
| W3 | crash | `fmtron -d … \| head` panicked with `failed printing to stdout: Broken pipe` (exit 101) | stdout is written with explicit error handling. A closed pipe ends quietly with exit 0, and other write errors (e.g. a full disk) are a clean `unable to write to stdout` |

## Subjective findings: decided

- **S1: blank lines were always removed.** About 12% of files, from 34 of
  the first 811 checked, use interior blank lines to group entries.
  **Resolved:** new `--blank-lines <keep|remove>` (`Config::blank_lines`),
  defaulting to `keep`. One blank line survives wherever the input has one or
  more between elements, comments or header items; blank lines just inside
  brackets are dropped. `remove` is the previous behaviour, and the
  layout-independence harness now pins that mode.
- **S2: hand-laid-out lists get squashed** when they fit. **Accepted as
  intended.**

## Codeberg, local only

`tools/wild-corpus/codeberg_collect.py` searched Codeberg's API with the
highest-yield keywords first (Bevy/game terms, then dotfiles, gitui, COSMIC,
config). It kept Rust repositories, plus repositories of any language for
config-style keywords, since RON configs live in dotfiles repos. The run was
stopped after 18 of 43 keywords as new files dried up.

Result: **177 unique files from 45 repositories: 174 formatted faithfully
and none failed.** The other three are one invalid file (a single-quoted
multi-character string, `'custom command'`, rejected by both) and two Fyrox
`settings.ron` files, the same `ron::Value` limitation noted above. The files
include gitui, rmpc and LeftWM configs, COSMIC themes and Bevy/Fyrox assets.
Output looked right throughout: `palette: Dark((` hugs like COSMIC's own
serializer, and ASCII-art headers and blank-line grouping survive. None of it
is committed, because Codeberg's API reports no license; 13 of the 45 repos
have a LICENSE file that could be checked by hand.

## Default width

Real-world RON is written wider than fmtron's default of 40 columns. The
authors' own longest line has a median of 56, a 75th percentile of 89 and a
90th percentile of 142 columns. At `-w 40`, 23% of files (457 of 1,950
sampled) come out with a wrapper broken around a lone atom, e.g.
`"3": SwitchToTab(` / `"Playlists",` / `),` between one-line siblings. That
happens 5,508 times; at `-w 100` it is 1% of files (73 times). A default of
100, rustfmt's, would match how RON is written. Not changed: it alters
everyone's output, so it is a maintainer decision.

## Corpus

`test_data/wild/` holds the redistributable subset: 2,958 files from 1,102
sources (about 7 MB of files plus a 0.9 MB manifest). A file is included
when:

- it is unique by content;
- `ron` or fmtron accepts it;
- its source declares a recognized open-source license (GitHub repository
  license, GitLab project license, or crate `license` field);
- it is at most 32 KiB, with at most 8 files per source.

`test_data/wild/manifest.ron` records, per source, the host, repository or
crate, commit or version, and license, plus each file's original path and
source URL. It is itself formatted by fmtron. `tests/wild_corpus.rs` runs all
the checks above over the corpus in about 6 s, and skips when the directory
is missing. `Cargo.toml` excludes `test_data/wild` from the published crate.

No Codeberg files made it in, because Codeberg's API reports no license and
unlicensed sources were excluded. Out of 7,942 good files, 3,671 were left out
for that reason.

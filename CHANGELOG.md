# Changelog

Output changes are marked **output**; see [STABILITY.md](STABILITY.md) for
what may change when.

## Unreleased

- The published crate holds only the library, the binary and their
  documentation: 57 KiB instead of 468 KiB. The tests and their data stay in
  the repository.

## 1.0.0-rc.3 — 2026-10-25

- The command line leaves a file with nothing to format, because it is empty
  or holds only comments and attributes, exactly as it is, with exit status 0
  and no message; `--check` does not report it. Before, such a file failed
  with `no RON data found`, which made format-on-save complain about every
  new file until its first value, and `--check` fail over a placeholder.
  The library still returns `FormatError::Empty`, so a caller can decide for
  itself.

**Output** changes, from an audit of the golden matrix. Over the 2,959
real-world files of `test_data/wild/`, 60 format differently (one for two of
these reasons):

- A comment after an opening bracket, at the end of its line, stays there:
  `Scene( // optional`. Before, it moved to a line of its own above the first
  element, where it reads as a comment on that element. Input affected: a
  container whose opening line ends in a comment (14 of the 60 files).
- A block comment between a map key or field name and its colon stays before
  the colon: `delay /* seconds */: 5`. Before, it moved after the colon, next
  to any comment written there. Input affected: such a comment (2 files).
- A packed row that is too long is wrapped into lines as even in length as
  the fewest lines allow: `"aa", "bb",` / `"cc", "dd",` rather than
  `"aa", "bb", "cc",` / `"dd",`. The number of lines is unchanged. Input
  affected: a packed list or tuple that wraps (39 files).
- A line's width is `max_width`, but at least half of `max_width` past its
  indentation, so a container no longer breaks into lines that are nearly
  all indentation and overrun anyway. Widening `max_width` still never adds
  lines. Input affected: lines indented past half of `max_width` (6 files,
  all nested over 12 levels at the default width).

`cargo xtask bless` regenerates every golden output, and `cargo xtask
matrix-audit` checks the golden matrix and a wider set of settings; CI runs
the audit. The wild-corpus harness runs inside `cargo xtask wild evaluate`
instead of as a separate crate under `tools/`.

## 1.0.0-rc.2 — 2026-10-07

The library API for 1.0, as a program embedding fmtron needs it:

- `FormatError::Parse` holds fmtron's own `ParseError`, with `line()`,
  `column()`, `offset()`, `message()` and `with_path()`; `pest` is no longer
  part of the API, nor is `ron` (`FileConfigError::Parse` holds a
  `ConfigParseError`).
- `Config` is `#[non_exhaustive]` and gains `with_*` builder methods; the
  parser, the pretty-printer and the AST are private.
- `style_edition` in `fmt.ron`, `--style-edition` on the command line and
  `Config::style_edition`: the set of layout rules, named by year. `2026` is
  the only edition.
- The binary is behind the default `cli` feature. Without it, the library has
  no file-system or terminal dependencies and builds for `wasm32`.
- `rust-version = "1.88"`.
- STABILITY.md states what may change in 1.x.

**Output** changes:

- An attribute with a comment in it, which fmtron keeps as written, now has
  its line breaks rewritten to the file's line ending and the spaces before a
  break in its layout dropped; a break inside a string, char or raw literal
  in it is still part of that literal's value, and a block comment in it
  keeps its text, spaces included, as block comments do everywhere. Input
  affected: a `#![...]` holding both a comment and a line break whose ending
  differs from the first line break of the file, or a space before such a
  break.
- `format_ron` never ends its output with a line break. Before, one whose
  input had a comment after its value did, so appending `line_ending` to it,
  as the documentation says, gave a blank line at the end. Input affected: a
  file whose last line is a comment. The command line is unchanged: it always
  wrote exactly one final newline.

## 1.0.0-rc.1 — 2026-10-06

The 1.0 style, released for real-world use before it is frozen. **Output**
changes, measured over 3,089 real-world files (details in PLAN_1_0.md):

- A struct or map whose input breaks the line right after its opening bracket
  stays broken, even if it would fit on one line. 0.10 joined every container
  that fit.
- A list or tuple of short elements (numbers, bools and chars at any width;
  other atoms and tuples of atoms up to 16 columns) is packed several to a
  line. Elements the input already packs keep their line breaks; scalars are
  packed even when written one per line; strings written one per line stay
  so.
- A wrapper around a single container always hugs it (`Some([` … `])`); the
  layout that broke around the container is gone.
- Fixed: a hugged child could overrun the width by the comma after its
  wrapper.

## 0.10.0 and earlier

See the git history.

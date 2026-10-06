# Changelog

Output changes are marked **output**; see [STABILITY.md](STABILITY.md) for
what may change when.

## Unreleased

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

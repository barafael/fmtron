# Stability

What fmtron 1.x promises, and what it does not.

## Output

The same input with the same settings formats the same way in every 1.x
release, and formatting fmtron's output again changes nothing.

The exceptions are bug fixes: output that

- is not valid RON, or means something else than the input (the `ron` crate
  decides);
- loses, duplicates or reorders a comment;
- changes when formatted again;
- comes from input that is not RON, or is refused for input that is;
- or a crash.

Changes of taste are not bug fixes. A new layout rule is a new **style
edition**, chosen with `style_edition` in `fmt.ron` or `--style-edition`. The
default edition, 2026, changes only in fmtron 2.0.

Every change to the output is listed in [CHANGELOG.md](CHANGELOG.md), with
the kind of input it affects.

Not promised: that every line fits within `max_width` (it is a soft limit),
or the wording of error messages.

## Command line

Stable: every flag, its meaning and its default; the exit codes; reading
stdin and writing stdout; where `fmt.ron` is looked for and how its settings
combine with flags; the `fmt.ron` fields. Flags and fields may be added in
minor releases and are removed only in 2.0.

Not stable: the text of messages and diagnostics, of `--help`, and of the
diffs `--check` prints.

## Library

The `fmtron` crate follows semver. Its public API is what the documentation
shows: `format_ron`, `Config` and its settings, `FormatError` and
`ParseError`, `line_ending`, the `fmt.ron` types and the constants. No type
of another crate is part of it, so dependencies are upgraded in minor
releases.

Settings and error variants may be added in minor releases: `Config`,
`FileConfig`, `FormatError`, `FileConfigError` and `StyleEdition` are
`#[non_exhaustive]`, so start a `Config` from `Config::default()`.

Without its default `cli` feature, the library has no file-system or terminal
dependencies and builds for `wasm32-unknown-unknown`. That stays so.

## Rust version

The minimum supported Rust version is `rust-version` in `Cargo.toml`, checked
in CI. Raising it is a minor change, never a patch.

## Before 1.0.0

Release candidates (`1.0.0-rc.N`) carry the 1.0 style; the library API may
still change between them.

# fmtron - simple autoformatting tool for RON

Published as www.crates.io/crates/fmtron.

**Try it online:** the [online RON formatter](https://barafael.github.io/fmtron/)
formats in your browser and can show a file's structure as nested shapes; the
[fmt.ron editor](https://barafael.github.io/fmtron/ui/) edits a project's
configuration file with a live preview. Both run this crate compiled to
WebAssembly, entirely in your browser. Their sources live in [`wasm/`](wasm),
[`www/`](www) and [`ui/`](ui), deployed by
[.github/workflows/pages.yml](.github/workflows/pages.yml).

# About the name

This project was originally started under the name `ronfmt`. The original crate
author attempted to transfer ownership of that name, but could not prove their
identity to the crates.io team. The project was therefore renamed to `fmtron`.

## How to install

`cargo install fmtron`

## How to use

```sh
fmtron assets/level.ron          # format a file in place
fmtron assets/ config.ron        # several files and directories
fmtron --check .                 # CI: print diffs, exit 1 if anything would change
fmtron < in.ron > out.ron        # stdin to stdout (also `fmtron -`)
```

- Directories are searched for `*.ron` files, skipping hidden files and anything your `.gitignore` excludes; a file named explicitly is always formatted
- Files are only rewritten if their formatting changes, and the new content replaces the old atomically. Symlinks and file permissions are preserved, and read-only files are refused
- `--check` writes nothing; it prints a unified diff for every file that would change
- `-d` / `--stdout` prints the formatted result of a single file instead of writing it
- With stdin, `--stdin-filepath <path>` says which file the input stands for: it picks the `fmt.ron` and names the input in messages. This is what editor integrations need
- `--backup` copies each changed file to `<file>.bak` before replacing it (no backups are made by default)
- Exit status: 0 on success; 1 if any file could not be formatted, or, with `--check`, if any file needs formatting. All files are processed and every error is reported
- A file with nothing to format, because it is empty or holds only comments and attributes, is left exactly as it is, without a message: whether it ought to hold a value is for the program that loads it to say
- Set tab size with `-t <size>` (4 by default)
- Set max line width with `-w <width>` (100 by default). This is a soft limit, so long or deeply-nested values may sometimes overrun it
- Cap container nesting with `--max-depth <depth>` (512 by default, at least 1); deeper input is rejected cleanly instead of overflowing the stack
- Set the upper bound enforced on `-t` with `--max-tab <size>` (1024 by default); a larger `-t` is rejected
- Choose how blank lines are treated with `--blank-lines <keep|remove>`. `keep` (the default) preserves one blank line wherever the input separates elements or comments with one or more; `remove` drops them all
- Choose the set of layout rules with `--style-edition <year>` (2026, the only one so far). A future change of style will be a new edition that stays opt-in until fmtron 2.0; see [STABILITY.md](STABILITY.md) for what may change when

### In CI and editors

```yaml
# GitHub Actions
- run: cargo install fmtron && fmtron --check .
```

```yaml
# .pre-commit-config.yaml
- repo: local
  hooks:
    - id: fmtron
      name: fmtron
      entry: fmtron
      language: system
      files: \.ron$
```

For format-on-save, have the editor pipe the buffer through
`fmtron --stdin-filepath <path of the file>` and replace it with the output.
Parse errors point at the problem in terms of RON, for example:

```
unable to parse RON:
parse error:  --> levels/one.ron:2:2
  |
2 |  b: 2)
  |  ^---
  |
  = expected `,` or `)`, found `b`
```

## Configuration file: `fmt.ron`

Like `rustfmt.toml`, a `fmt.ron` (or hidden `.fmt.ron`) sets formatting
options for a project. fmtron looks for one in the input file's directory
and then in each parent directory; the nearest one is used, and files are
not merged. The [fmt.ron editor](https://barafael.github.io/fmtron/ui/)
edits one in the browser with a live preview; see
[fmt.ron.ui](#fmtronui) below for the desktop version.

The file is RON itself: one struct with these fields, every one optional:

```ron
// fmt.ron
(
    max_width: 100,      // -w
    tab_size: 4,         // -t
    blank_lines: Keep,   // --blank-lines: Keep or Remove
    max_depth: 512,      // --max-depth
    max_tab: 1024,       // --max-tab
    style_edition: 2026, // --style-edition
)
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `max_width` | integer, at least 1 | `100` | Soft maximum line width; long or deeply nested values may overrun it |
| `tab_size` | integer, at least 1 | `4` | Indentation per nesting level, in spaces; at most `max_tab` |
| `blank_lines` | `Keep` or `Remove` | `Keep` | Keep one blank line wherever the input has one or more, or drop them all |
| `max_depth` | integer, at least 1 | `512` | Deeper input is rejected instead of overflowing the stack |
| `max_tab` | integer, at least 1 | `1024` | Upper bound on `tab_size`; a larger `tab_size` is an error |
| `style_edition` | year | `2026` | The set of layout rules; see [STABILITY.md](STABILITY.md) |

- A missing field keeps its default. Since each field is an `Option`, a value may also be written as `Some(100)`, and `None` means "not set"
- Comments (`//` and nested `/* */`), blank lines, a struct name (`FileConfig(…)`) and `#![enable(…)]` attributes are all fine: the file is parsed with the `ron` crate
- Settings are applied in this order, later ones winning: built-in defaults, `fmt.ron`, command-line flags
- Unknown fields, values of the wrong type, and unusable sizes are errors, reported with the file's path and position, so a typo like `max_widht` cannot silently do nothing. In particular, `max_width`, `tab_size`, and `max_tab` must be at least 1, and `tab_size` may not exceed `max_tab`
- `--config <path>` uses a specific file instead of searching; `--no-config` ignores config files
- `--print-config` prints the effective settings as a complete `fmt.ron`, with the file they came from; with `-i`, it resolves the config the way formatting that file would
- A `fmt.ron` is a `.ron` file in the project, so `fmtron .` formats it too, with its own settings

### fmt.ron.ui

A small window for editing a `fmt.ron`: the six fields as a form, a sample
formatted live with those settings, and the file's text. It is the `ui/`
crate, and the same program runs
[in the browser](https://barafael.github.io/fmtron/ui/), where opening uses
the file picker and saving downloads the file.

```sh
cargo run --release --manifest-path ui/Cargo.toml          # the fmt.ron in the current directory
cargo run --release --manifest-path ui/Cargo.toml -- path/to/fmt.ron
```

- Ctrl+O opens a file, Ctrl+S saves, Ctrl+Shift+S saves as; `j`/`k` move between fields, Space sets a field or unsets it to inherit the default, ←/→ step a value, Enter types it, `h` lists the keys
- Changing a field rewrites only that entry, then lays the file out as fmtron would with the file's own settings. Comments stay; a comment after an entry on the same line goes with the entry when it is unset
- The file's text is editable directly, for anything the form does not cover
- Without a `fmt.ron` in the current directory, saving creates one there

## Style

fmtron lays out values by these rules:

- A container goes on one line if it fits within the line width, and otherwise puts one element per line, indented, with a trailing comma. A line's width is `max_width`, but at least half of `max_width` past its indentation: where indentation alone would use up the width, breaking a container only adds lines that overrun anyway, so a deeply indented line keeps its content together
- A struct or map whose input breaks the line right after its opening bracket stays broken, even if it would fit on one line. To have fmtron join it, remove that line break
- A broken list or tuple whose elements are all short is packed several elements to a line, like words in a paragraph. Short means a number, bool or char, or a string, identifier or tuple of atoms at most 16 columns wide. Tuples count because serde writes arrays such as `[u8; 32]` as tuples. They are packed in two cases:
  - If the input already puts two or more of them on one line, each line break between them is kept and only a line that grows too long is wrapped, into lines as even in length as the fewest lines allow, so editing one line never reflows the others. If the input also breaks the line right after the opening bracket, the list is a grid: it stays broken even if it would fit on one line, so its rows stay rows
  - If they are all numbers, bools or chars, or tuples of those, they are packed even when written one per line

  Strings and identifiers written one per line stay that way. A blank line between elements starts a new line, after a blank one
- A tuple or newtype variant around a single container, such as `Some((…))` or `Wrapper([…])`, hugs it: if it does not fit on one line, the container breaks inside the parentheses (`Some((` … `))`), as `ron`'s own pretty-printer writes it
- Comments force their container to break, one element per line. A comment stays next to what it is written beside: after an opening bracket at the end of its line (`Scene( // optional`), between a key and its colon (`delay /* seconds */: 5`), or after the colon (`delay: /* seconds */ 5`). A comment on a line of its own goes before the next element

## As a library

The same formatter is a crate, for an editor plugin, a build step or a web
page that formats RON:

```toml
[dependencies]
fmtron = { version = "1", default-features = false }
```

Without the default `cli` feature it has no file-system or terminal
dependencies and builds for `wasm32-unknown-unknown`.

```rust
use fmtron::{Config, FormatError, format_ron};

let config = Config::default().with_max_width(80);
match format_ron(input, &config) {
    Ok(formatted) => editor.replace(formatted),
    // Where and what, for the editor to mark: 1-based line and column, and
    // a message such as "expected `,` or `)`, found `b`".
    Err(FormatError::Parse(e)) => editor.mark(e.line(), e.column(), e.message()),
    Err(e) => editor.notify(e.to_string()),
}
```

The API is documented on [docs.rs](https://docs.rs/fmtron); what it promises
across releases is in [STABILITY.md](STABILITY.md).

## Features

- Preserves comments (line, block, and nested block comments) across a format round-trip. The later lines of a block comment move with it when it is re-indented, keeping their indentation relative to it
- Keeps the input's line endings (LF or CRLF), decided by its first line break outside string literals and used inside block comments and comment-bearing attributes too; line breaks inside strings are part of their value and never change
- Supports the full RON surface: numeric suffixes, special floats, byte/raw strings, raw identifiers, Unicode identifiers, and `#![...]` attributes
- Formatting is idempotent and semantically equivalent to the input, validated against the official `ron` crate as an oracle

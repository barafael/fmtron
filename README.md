# fmtron - simple autoformatting tool for RON

Published as www.crates.io/crates/fmtron.

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
- Set tab size with `-t <size>` (4 by default)
- Set max line width with `-w <width>` (100 by default). This is a soft limit, so long or deeply-nested values may sometimes overrun it
- Cap container nesting with `--max-depth <depth>` (512 by default); deeper input is rejected cleanly instead of overflowing the stack
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
and then in each parent directory; the nearest one is used. Every field is
optional:

```ron
// fmt.ron
(
    max_width: 100,     // -w
    tab_size: 4,        // -t
    blank_lines: Keep,  // --blank-lines: Keep or Remove
    max_depth: 512,     // --max-depth
    max_tab: 1024,      // --max-tab
    style_edition: 2026, // --style-edition
)
```

- Settings are applied in this order, later ones winning: built-in defaults, `fmt.ron`, command-line flags
- Unknown fields and values of the wrong type are errors, reported with the file's path and position, so a typo like `max_widht` cannot silently do nothing
- `--config <path>` uses a specific file instead of searching; `--no-config` ignores config files
- `--print-config` prints the effective settings as a complete `fmt.ron`, with the file they came from; with `-i`, it resolves the config the way formatting that file would

## Style

fmtron lays out values by these rules:

- A container goes on one line if it fits within the line width, and otherwise puts one element per line, indented, with a trailing comma
- A struct or map whose input breaks the line right after its opening bracket stays broken, even if it would fit on one line. To have fmtron join it, remove that line break
- A broken list or tuple whose elements are all short is packed several elements to a line, like words in a paragraph. Short means a number, bool or char, or a string, identifier or tuple of atoms at most 16 columns wide. Tuples count because serde writes arrays such as `[u8; 32]` as tuples. They are packed in two cases:
  - If the input already puts two or more of them on one line, each line break between them is kept and only a line that grows too long is wrapped, so editing one line never reflows the others. If the input also breaks the line right after the opening bracket, the list is a grid: it stays broken even if it would fit on one line, so its rows stay rows
  - If they are all numbers, bools or chars, or tuples of those, they are packed even when written one per line

  Strings and identifiers written one per line stay that way. A blank line between elements starts a new line, after a blank one
- A tuple or newtype variant around a single container, such as `Some((…))` or `Wrapper([…])`, hugs it: if it does not fit on one line, the container breaks inside the parentheses (`Some((` … `))`), as `ron`'s own pretty-printer writes it
- Comments force their container to break, one element per line

## As a library

The same formatter is a crate, for an editor plugin, a build step or a web
page that formats RON:

```toml
[dependencies]
fmtron = { version = "1.0.0-rc.2", default-features = false }
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
- Keeps the input's line endings (LF or CRLF), decided by its first line break outside string literals and used inside block comments too; line breaks inside strings are part of their value and never change
- Supports the full RON surface: numeric suffixes, special floats, byte/raw strings, raw identifiers, Unicode identifiers, and `#![...]` attributes
- Formatting is idempotent and semantically equivalent to the input, validated against the official `ron` crate as an oracle

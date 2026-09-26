# fmtron - simple autoformatting tool for RON

Published as www.crates.io/crates/fmtron.

# About the name

This project was originally started under the name `ronfmt`. The original crate
author attempted to transfer ownership of that name, but could not prove their
identity to the crates.io team. The project was therefore renamed to `fmtron`.

## How to install

`cargo install fmtron`

## How to use

`fmtron -i file_to_format.ron`

- On use, the tool will create a backup file called `<source_file_name>.bak` in the same directory. Only the latest backup is kept. Add `*.bak` to your `.gitignore` if you would like to keep your repo clean.
- Use `-d` flag to write the formatted output to the terminal instead of overwriting the source file
- Set tab size with `-t <size>` (4 by default)
- Set max line width with `-w <width>` (100 by default). This is a soft limit, so long or deeply-nested values may sometimes overrun it
- Cap container nesting with `--max-depth <depth>` (512 by default); deeper input is rejected cleanly instead of overflowing the stack
- Set the upper bound enforced on `-t` with `--max-tab <size>` (1024 by default); a larger `-t` is rejected
- Choose how blank lines are treated with `--blank-lines <keep|remove>`. `keep` (the default) preserves one blank line wherever the input separates elements or comments with one or more; `remove` drops them all, so the output depends only on the input's tokens and comments

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
)
```

- Settings are applied in this order, later ones winning: built-in defaults, `fmt.ron`, command-line flags
- Unknown fields and values of the wrong type are errors, reported with the file's path and position, so a typo like `max_widht` cannot silently do nothing
- `--config <path>` uses a specific file instead of searching; `--no-config` ignores config files
- `--print-config` prints the effective settings as a complete `fmt.ron`, with the file they came from; with `-i`, it resolves the config the way formatting that file would

## Features

- Preserves comments (line, block, and nested block comments) across a format round-trip
- Keeps the input's line endings (LF or CRLF)
- Supports the full RON surface: numeric suffixes, special floats, byte/raw strings, raw identifiers, Unicode identifiers, and `#![...]` attributes
- Formatting is idempotent and semantically equivalent to the input, validated against the official `ron` crate as an oracle

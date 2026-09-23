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
- Set max line width with `-w <width>` (40 by default). This is a soft limit, so long or deeply-nested values may sometimes overrun it
- Cap container nesting with `--max-depth <depth>` (512 by default); deeper input is rejected cleanly instead of overflowing the stack
- Set the upper bound enforced on `-t` with `--max-tab <size>` (1024 by default); a larger `-t` is rejected

## Features

- Preserves comments (line, block, and nested block comments) across a format round-trip
- Supports the full RON surface: numeric suffixes, special floats, byte/raw strings, raw identifiers, Unicode identifiers, and `#![...]` attributes
- Formatting is idempotent and semantically equivalent to the input, validated against the official `ron` crate as an oracle

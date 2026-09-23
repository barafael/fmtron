# fmtron adversarial audit — findings

Audited **fmtron 0.6.1** (`target/release/fmtron`, commit `e275117`) on Linux.

Approach: black-box first (feed hostile inputs/args through the CLI, watch for crashes,
hangs, panics, wrong output), gray-box second (read `src/ron.pest`, `src/ast/*`,
`src/pretty.rs` to pin root causes). Correctness was cross-checked against the official
`ron` crate **0.12** as an oracle (`ron::from_str::<ron::Value>`).

Scope / endurance: ~70 directed CLI cases, ~110 directed oracle cases, a comment-preservation
suite, 300 seeded random deep documents × 5 widths (oracle semantics + idempotency),
20,000 corrupted-input mutations (panic hunt), 300 real-world corpus files formatted and
re-formatted, the project's own `gaps/` golden files, plus performance probes.

---

## Summary

| # | Severity | Finding | Type |
|---|----------|---------|------|
| F1 | **High** | Stack overflow → abort on ~3,293-deep nesting (3.3 KB input) | crash (SIGABRT) |
| F2 | **High** | Unbounded `-t <tab_size>`: giant output, hang, or `capacity overflow` panic | DoS / panic |
| F3 | **Medium** | Digit-separator literals (`1_000`, `0xFF_FF`, …) rejected though the `ron` crate accepts them (incl. the repo's own new fixture) | acceptance bug |
| F4 | **Medium** | Invalid base digits accepted & echoed: `0b2`, `0b102`, `0o8`, `0o18` → output is *not* valid RON | correctness bug |
| F5 | **Medium** | `inf`/`NaN` prefix shadowing: `inf32`, `infinity`, `inf8`, `NaNfoo` (valid identifiers) rejected everywhere | acceptance bug |
| F6 | **Low** | Over-acceptance vs oracle: invalid escapes `"\z"`, bare `Some`/`Some()`, arbitrary `#![enable(…)]` | acceptance bug |
| F7 | **Low** | Stale golden file `gaps/formatted/unicode_identifiers.ron` disagrees with current (correct) column-accurate output | maintenance |
| F8 | **Nit** | Some parse errors report position 1:1 instead of the offending token (raw-string hash mismatches) | UX |

Everything else probed (semantic preservation, idempotency, comment text preservation,
malformed-input robustness, error handling paths, performance, CRLF) came back clean — see
[Tested and passed](#tested-and-passed) for the evidence.

---

## F1 — Stack overflow, process abort on ~3,300-deep container nesting

**Severity: High — a 3.3 KB file kills the process.**

```
$ python3 -c "print('['*3293 + ']'*3293)" > d.ron
$ fmtron -i d.ron -d -w 200
thread 'main' has overflowed its stack
fatal runtime error: stack overflow, aborting    # exit code 134 (SIGABRT)
```

- Nesting depth **3293** aborts; depth **3292** succeeds (hard cliff, no graceful degradation).
- Affects lists (`[...]`), tuples (`(...)`/`Foo(...)`), and fields–structs at similar depths.
- Applies to only **valid RON** — no malformed input needed.
- Root cause: the whole pipeline is recursion-based with no depth guard:
  - pest parse of nested `list`/`tuple_type` (`src/ron.pest`),
  - `Value::from` AST build (`src/ast/mod.rs:140`),
  - `value_doc`/`kind_doc`/`container` and the Wadler `best()`/`fits_probe()` printer
    (`src/ast/display.rs:112`, `src/pretty.rs:101`).
- Note: in `-i` mode the input file is left untouched (error precedes writes), but the
  process dies on a few KB of input — a trivial local DoS / crash.

## F2 — `-t <tab_size>` is unbounded → giant output, hang, or panic

**Severity: High (resource exhaustion; panic).** `tab_size` is accepted with no upper
bound. When a container breaks, the printer emits `" ".repeat(indent)` where `indent`
accumulates `tab_size` per nesting level (`src/pretty.rs:118-126`).

```
$ printf '[[1]]' > i.ron                       # 6 bytes
$ fmtron -i i.ron -d -t 100000000 -w 1 | wc -c
400000012                                        # 400 MB output from a 6-byte input
```

- `-t 18446744073709551615 -w 1` → **panic** `capacity overflow` at
  `library/alloc/src/raw_vec/mod.rs:28:5`, exit code 101.
- `-t 3000000000` → **hang** (writes gigabytes, exceeded the 8 s timeout).
- Output size scales as `O(tab_size × broken lines)`; in `-i` (in-place) mode the
  *user's own file* is overwritten with that giant output, so a mistyped `-t` both fills
  the disk and destroys the formatted file's content.
- Root cause: unvalidated CLI value; `tab_size` feeds `nest()` and then `.repeat()`.

## F3 — Digit-separated numbers rejected (valid RON, per the `ron` crate)

**Severity: Medium — fmtron can't format valid files.** The grammar has no `_` in any
number rule (`src/ron.pest:30-48`), but `ron` 0.12 accepts separators everywhere:

| input | fmtron | ron 0.12 |
|-------|:------:|:--------:|
| `1_000`, `0xFF_FF`, `0x1_2`, `1e3_0`, `3.14_15`, `1.0_5`, `1_0.5`, `0_1`, `-0_1`, `0x1_`, `1e1_`, `.5_` | ✗ parse error | ✓ |

Notably, *the repo's own new fixture* `test_data/gaps/unformatted/digit_separators.ron`
(created before this audit) is exactly this case, so `gaps/formatted/digit_separators.ron`
is unattainable today.

## F4 — Invalid base digits accepted and echoed → output is not valid RON

**Severity: Medium — a formatter that turns clean input into invalid output.**

`with_base = { "0" ~ ("x" | "b" | "o") ~ ASCII_HEX_DIGIT+ }` (`src/ron.pest:36`) uses hex
digits for *all* bases, so invalid binary/octal literals parse and are printed verbatim:

| input | fmtron | ron 0.12 | formatted output |
|-------|:------:|:--------:|------------------|
| `0b2`, `0b21`, `0b102` | ✓ | ✗ | unchanged (invalid RON) |
| `0o8`, `0o18` | ✓ | ✗ | unchanged (invalid RON) |

The tool should reject these at parse time instead of pretty-printing invalid RON.

## F5 — `inf`/`NaN` prefix shadowing rejects valid identifiers

**Severity: Medium — valid RON rejected anywhere they appear.**

`value = { float | … | unit_type }` tries `float` first, and `special_float`
(`src/ron.pest:48`) is atomic (`@`), so `inf`/`NaN` match greedily and the parser never
backtracks to `ident`. Identifiers beginning with `inf`/`NaN` become unparseable:

| input | fmtron | ron 0.12 |
|-------|:------:|:--------:|
| `inf32`, `infinity`, `inf8`, `NaNfoo` | ✗ | ✓ (Unit value / identifier) |
| `Some(inf32)`, `{inf32: 1}`, `[inf32]`, `inf32(1)`, `Foo(NaNfoo)`, `(a: inf32)` | ✗ | ✓ |

`-inf32` / `-NaNfoo` are rejected by *both* (consistent). Uppercase `INF` works (case
sensitive). Root cause: atomic `special_float` steals the prefix and blocks the `unit_type`
fallback (a pure-PEG parser would backtrack and accept these).

## F6 — Over-acceptance vs the official oracle

**Severity: Low.** fmtron formats things the `ron` crate rejects, so its output is not
always re-parseable by `ron`:

| input | fmtron | ron 0.12 |
|-------|:------:|:--------:|
| `"\z"` (any `\` + char escape, `src/ron.pest:58`) | ✓ | ✗ |
| `Some` (bare), `Some()` | ✓ | ✗ |
| `#![enable(foo)] #![enable(bar, baz)]` (arbitrary feature names) | ✓ | ✗ |

(These never crash — just widen the accepted language beyond the reference parser.)

## F7 — Stale golden file

`gaps/formatted/unicode_identifiers.ron` expects

```
(
    名前: "rafael",
    …
)
```

but fmtron correctly keeps `(名前: "rafael", 数: 42, Ωmega: 3.14)` flat at the default
`-w 40` because that line is **37 display columns** (the golden was generated with
byte-based width before the "column-accurate widths" fix). Tool behavior is right; the
golden needs regenerating.

## F8 — Imprecise parse-error positions (nit)

Raw-string hash-count mismatches (e.g. `r##"x"#`, `r` + 50k `#` unclosed) report
`--> 1:1` instead of the true failure position.

---

## Tested and passed (with the same rigor as the failures)

- **Semantic preservation** (`ron::from_str` before == after): 110 directed cases +
  300 seeded random documents (depth ≤ 12, comment-injected, 5 widths). **Zero drift.**
- **Idempotency** (`format(format(x)) == format(x)`): same corpus. **Zero failures.**
- **Comment text preservation**: nested `/*/*/`, `//` inside block, hash-raw strings
  containing comment-like text, trailing/dangling/leading on every shape. **All kept.**
- **Malformed input**: 20,000 random character/byte mutations of valid documents →
  **no panics**; ~50 adversarial malformed inputs (unterminated strings/comments/blocks,
  stray braces, NULs, control chars, CR-only, BiDi) → clean typed `FormatError`s, rc=1.
- **Huge but valid inputs**: `-w`/`-t` extremes that *do* parse don't hang; 200k-hash raw
  string = 6 ms; 40k-element lists/maps = ~100 ms (linear, no quadratic Wadler blowup).
- **CLI error paths**: missing file, directory-as-input, permission errors, backup path
  that is a directory, read-only dir, parse error (file untouched, no backup created),
  `*.bak` name collisions, CRLF round-trip — all graceful, correct messages, rc=1.
- **Aesthetic output** on nested/forced-break/empty-container/width-0 cases: sensible.
  Width is a *documented soft limit*; the only overruns observed come from unbreakable
  atoms, glued inline comments, or accumulated indentation (inherent to the design).

## Repro cheatsheet

```bash
BIN=/path/to/target/release/fmtron
# F1  (stack overflow -> exit 134)
python3 -c "print('['*3293+']'*3293)" | $BIN -i /dev/stdin -d
# F2  (panic / giant output / hang)
printf '[[1]]' | $BIN -i /dev/stdin -d -t 18446744073709551615 -w 1
printf '[[1]]' | $BIN -i /dev/stdin -d -t 100000000 -w 1 | wc -c
# F3 / F4 / F5 / F6  (acceptance divergences vs `ron` 0.12)
for x in '1_000' '0b102' '0o8' 'inf32' 'infinity' 'NaNfoo' '"\\z"' 'Some'; do
  echo "$x" | $BIN -i /dev/stdin -d
done
```

Gray-box root-cause pointers: `src/ron.pest:36` (`with_base`), `:48` (`special_float`),
`:58` (`\`-escape laxness); `src/pretty.rs:118-126` (`" ".repeat(indent)`),
`src/ast/display.rs:112` + `src/pretty.rs:101` (unbounded recursion).
# Fix plan for the fmtron adversarial findings

Reference: `ADVERSARIAL_FINDINGS.md` (findings F1–F8). This plan fixes F1–F5 (real defects),
plus cheap follow-ups (F7, F6/F8 policy calls). Each item lists the change, files touched,
and the regression tests that pin the behavior.

Open decisions (recommended values in brackets) are flagged with ⚙; pick once and I'll
implement.

---

## Phase 1 — F1: bounded nesting (crash → typed error)

**Problem:** `Value::from` / pest parse / Wadler printer all recurse without a depth
guard; ~3,293-deep containers abort (SIGABRT). 3.3 KB of *valid* RON kills the process.

**Fix:** enforce `MAX_NESTING` with a one-pass, non-recursive lexical scan performed in
`format_ron()` *before* parsing:

```rust
pub const MAX_NESTING: usize = 512;   // ⚙ 512 (wide margin below the ~3290 crash cliff)

#[error("nesting depth {depth} exceeds the supported maximum of {max}")]
TooDeep { depth: usize, max: usize },
```

- `nesting_depth(&str) -> usize` scans bytes, **correctly skipping**: double/raw/byte
  strings (`"…"`, `r…"…"…`, `b"…"`, `br…"…"…`), char literals, nested block comments,
  line comments, and counting only `[ { (` / decrementing on closers. Returns the max run
  depth (also catches malformed input early — a bonus).
- If depth > `MAX_NESTING` → `Err(FormatError::TooDeep)`.
- Since depth is capped up front, all downstream recursions (pest include depth, AST,
  renderer) are bounded; no other code changes needed.

**Files:** `src/lib.rs` (`MAX_NESTING`, `TooDeep`, scanner + wire-in), `src/ast/mod.rs`
(no change needed).

**Tests (`tests/`):**
- `format_ron("…".repeat(513 + …))` returns `TooDeep`, and depth 512 still formats.
- Deep raw-string/binary-bytes/comment content does *not* falsely trip the scanner
  (`r#"[[["#`, `'['`, `/* [[[ */`).
- CLI: a 4,000-deep file now exits rc=1 with the `TooDeep` message instead of SIGABRT.

---

## Phase 2 — F2: bound `tab_size` (panic / giga-output → validated CLI)

**Problem:** unvalidated `-t` feeds `" ".repeat(indent)`; `-t usize::MAX` panics with
`capacity overflow`, `-t 1e8` emits 400 MB for a 6-byte input, `-t 3e9` hangs.

**Fix (two layers):**

1. **CLI validation** in `src/arguments.rs`:
   ```rust
   #[arg(short, default_value_t = 4, value_parser = clap::value_parser!(usize).range(0..=MAX_TAB))]
   pub tab_size: usize,
   ```
   `clap` then rejects `-t 2^63` with a clear message (rc=2) — no panic, no allocation.
   `MAX_TAB = 1024` ⚙ (generous; real formatters never exceed 8).
2. **Library clamp** in `format_ron()` (defense-in-depth for API users):
   `let config = Config { tab_size: config.tab_size.min(MAX_TAB), ..*config };`
   so even a direct API call can't OOM. With F1's cap, worst-case indent is
   `MAX_TAB × MAX_NESTING ≈ 512 KB/line` — bounded, no hang.

**Files:** `src/lib.rs` (`MAX_TAB` + clamp), `src/arguments.rs` (clap range).

**Tests:**
- CLI: `-t 18446744073709551615` → rc=2, "not in range" (no panic);
  `-t 1024` accepted.
- Library: `format_ron(x, Config{ tab_size: usize::MAX, .. })` still returns
  successfully with capped indentation and finishes instantly.

---

## Phase 3 — F4 (invalid base digits) + F3 (digit separators): number grammar

Both live in `src/ron.pest` and are related; do them together with one digit-alphabet
refactor and one oracle matrix `tests/reference` case.

**F4 fix — per-radix alphabets** (`with_base` today reuses `ASCII_HEX_DIGIT` for every
radix, accepting `0b2`, `0o8`, `0b102`, `0o18`):

```text
with_base = { "0" ~ ( "x" ~ hex_digits | "b" ~ bin_digits | "o" ~ oct_digits ) }
hex_digits = @{ ASCII_HEX_DIGIT+ ~ ("_" ~ ASCII_HEX_DIGIT+)* ~ "_"? }
bin_digits = @{ bin_digit+ ~ ("_" ~ bin_digit+)* ~ "_"? }   // bin_digit = _{ "0" | "1" }
oct_digits = @{ oct_digit+ ~ ("_" ~ oct_digit+)* ~ "_"? }   // oct_digit = _{ "0".."7" }
```

**F3 fix — separators in decimal/float literals** (`src/ron.pest:30-47`):

```text
dec_digits = @{ ASCII_DIGIT+ ~ ("_" ~ ASCII_DIGIT+)* ~ "_"? }   // trailing "_" per ron 0.12
unsigned_int = { with_base | dec_digits }
float_std  = { sign? ~ dec_digits ~ "." ~ dec_digits? ~ float_exp? ~ float_suffix? }
float_frac = { sign? ~ "." ~ dec_digits ~ float_exp? ~ float_suffix? }
float_sci  = { sign? ~ dec_digits ~ float_exp ~ float_suffix? }
float_int_suffix = { sign? ~ dec_digits ~ float_suffix }
float_exp  = { ("e" | "E") ~ sign? ~ dec_digits }
```

Notes:
- Atoms echo `as_str()`, so separators round-trip unchanged and semantics stay intact.
- Pin the exact separator rules (single vs. consecutive `_`, trailing `_`) against the
  `ron` 0.12 oracle in a matrix test before finalizing the grammar shape — the probe
  already shows the oracle accepts trailing `_` (`.5_`, `0x1_`, `1e1_`); **double
  underscores are unverified** and the matrix should lock that down.
- After the fix, `gaps/unformatted/digit_separators.ron` must format to exactly
  `gaps/formatted/digit_separators.ron` (regenerate golden if the tool's normalized
  output differs from the current hand-written one).

**Tests:**
- `0b2`, `0o8`, `0b102`, `0o18` → `Parse` error; `0b101`, `0o77`, `0x1F` still OK.
- Oracle-accepted separators (`1_000`, `0xFF_FF`, `3.14_15`, `1e3_0`, `1_0.5`, `.5_`,
  `0x1_`, …) parse and round-trip; oracle-rejected separator shapes rejected.
- `gaps/` golden validation (`tests/gap_validation.rs`) passes, incl. `digit_separators`.

---

## Phase 4 — F5: `inf`/`NaN` prefix shadowing

**Problem:** atomic `special_float` (`src/ron.pest:48`) greedily matches `inf`/`NaN`,
blocking the `value → unit_type` fallback, so valid identifiers `inf32`, `infinity`,
`inf8`, `NaNfoo` (and `Some(inf32)`, `{inf32: 1}`…) are rejected.

**Fix:** add an identifier-boundary lookahead so special floats only match when not
followed by an identifier character, letting `unit_type` win otherwise:

```text
special_float = @{ sign? ~ ("inf" | "NaN") ~ !ident_char ~ float_suffix? }
ident_char = { ASCII_ALPHANUMERIC | "_" | non_ascii }
```

- `inf` / `-inf` / `NaN` / `+NaN` still parse as floats; `infinity`, `inf32`, `NaNfoo`
  fall through to `unit_type` (Uppercase `INF` is already unaffected — case-sensitive).
- Confirm `-inf32`/`-NaNfoo` stay rejected by **both** (oracle rejects them too; our
  probe showed agreement), i.e. keep `sign` outside the new guard.

**Files:** `src/ron.pest` only.

**Tests:** the full probe matrix from the audit (`Some(inf32)`, `{inf32: 1}`,
`(a: inf32)`, `inf32(1)`, …) must agree with the oracle, plus `-inf`/`+NaN`/`inf` still
format as floats.

---

## Phase 5 — F7: regenerate stale golden + regression sweep

- Regenerate `test_data/gaps/formatted/unicode_identifiers.ron` with the current
  (correct, column-accurate) output at default `-w 40`.
- If the F3 work causes any other golden drift, regenerate those too — but only after
  confirming via the oracle that the *tool's* output (not the golden) is semantics-correct.

---

## Phase 6 (policy / optional) — F6 over-acceptance + F8 error positions

Decide scope with maintainers; all are non-crashing, so they can stay for a later release:

- **F6a — lax escapes (recommend fix):** `string_std_inner` and `char_inner`
  (`src/ron.pest:51-58`) accept any `\` + char. Restrict to valid RON escapes
  (`\n \r \t \\ \" \' \0 \xHH \u{…}`) → `"\z"` becomes a parse error like the oracle.
- **F6b — bare `Some`/`Some()` (recommend defer):** fmtron treats `Some` as a plain unit
  type; `ron` special-cases `Some`/`None`/`Ok`/`Err`. Matching that exactly is a larger
  semantic change with little user value; keep over-acceptance (formatting a superset is
  harmless here) unless strict parity is desired.
- **F6c — arbitrary `#![enable(…)]` (recommend keep):** a formatter shouldn't reject
  toolchain-specific extension names the parser doesn't know about.
- **F8 — raw-string error position = 1:1 (known pest limitation):** leave as-is unless a
  cheap workaround (post-parse span probing) is wanted; lowest priority.

---

## Verification (everything, after each phase)

```bash
cargo test                        # full suite incl. new regression tests
cargo clippy -- -D warnings
# corpus + oracle sweep (100% must format idempotently & semantics-preserve)
find test_data -name '*.ron' -print0 | xargs -0 -n1 fmtron -i /dev/stdin -d -w 80 >/dev/null
```

## Suggested commit sequence

1. `fix: bound nesting depth; cap tab_size (crash + DoS)` → F1 + F2
2. `fix: per-radix digit alphabets and digit separators in number grammar` → F3 + F4
3. `fix: special floats must not shadow inf/NaN-prefixed identifiers` → F5
4. `test: regenerate unicode_identifiers golden` → F7
5. *(optional)* `fix: reject unknown string/char escapes` → F6a

Note: the repo currently has unrelated uncommitted changes (digit_separators fixtures,
`tests/reference_validation.rs`, etc.) — F3 depends on and finalizes those, so land it
after/reviewed-with them.
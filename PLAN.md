# Path A — Roadmap to a complete, comment-preserving RON formatter

Goal: turn fmtron into a complete, comment-preserving, edge-case-respecting RON
formatter. The official `ron` crate is used **only as a validation oracle**
(the `tests/gap_validation.rs` harness). The hard parts of a formatter are the
lossless parse and the printer; the lexical gaps are mechanical.

## Current state

`pest grammar (lossy) → minimal AST (precomputed lengths) → greedy,
column-unaware Display`, with `COMMENT` silent (`src/ron.pest:3`) and formatter
config in global atomics (`src/lib.rs:7-8`).

## Phases

| # | Phase | Effort | Risk | Deliverable |
|---|---|---|---|---|
| 0 | Harden the test harness | S (~0.5d) | none | conformance suite stops `unwrap`-panic'ing; oracle + idempotency gates |
| 1 | Close the 6 lexical gaps | M (1–2d) | low | `gap_validation` registry flipped to green |
| 2 | Structural grammar audit | M (1–2d) | med | tuple/newtype/named-struct/map disambiguation proven |
| 3 | Comment capture + attachment | L (2–3d) | med | comments survive a format round-trip |
| 4 | Column-aware printer | M (2d) | low | no overruns; global atomics gone |
| 5 | CLI robustness + corpus fuzz | S (1d) | none | graceful errors; ron's own corpus passes |

Estimated ~8–12 days of careful solo work. Highest-value, highest-risk pieces
are Phase 3 (comments) and Phase 2 (structural ambiguity); Phase 1 is mostly
typing.

---

## Phase 0 — Harden the harness (do first)

- `tests/format_ron.rs:45` does `format_ron(&input).unwrap()` inside the loop —
  one unsupported file aborts the whole suite. Change it to collect per-file
  results and report all failures.
- Generalize the gap-validation oracle from "gaps only" to the **entire
  corpus**: `ron::from_str::<ron::Value>(original) == ron::from_str::<ron::Value>(formatted)`
  for every case. Permanent regression net.
- Add an **idempotency** test: `format(format(x)) == format(x)`.
- Pull the `ron` crate's own `tests/**/*.ron` files in as an extra conformance
  corpus (free, authoritative, large).

## Phase 1 — Close the 6 lexical gaps

Each is a small `src/ron.pest` edit; the AST already stores atoms as `String`
(`src/ast/mod.rs:46`), so literal forms round-trip once parsed.

1. **Signed exponents** — `float_exp` add `sign?` (`ron.pest:35`).
2. **Special floats** — add `inf`/`+inf`/`-inf`/`NaN` to `float` (ordering: put
   these before `signed_int`/`unit_type` so they are not misread as idents).
3. **Numeric suffixes** — optional `( "i8" | … | "u128" | "f32" | "f64" )` tail
   on int/float.
4. **Byte strings** — `b"…"` (escaped) and `br#"…"` (raw), mirroring the
   existing string rules. Add a `Rule::byte_string` arm in `ast/mod.rs`.
5. **Raw identifiers** — extend `ident` with a `"r#"?` prefix; **ordering
   hazard**: `r#"…"` raw strings must be tried *before* `ident` in `value`, or
   `r#` will grab a bare `r`.
6. **Unicode identifiers** — replace `ASCII_ALPHA`/`ASCII_ALPHANUMERIC` with
   Unicode classes (`ALPHABETIC`/`ALPHANUMERIC`). Verify pest support; if
   unavailable, approximate with `\p{L}`-style classes or a permissive ident
   rule validated post-parse with the `unicode-ident` crate (a formatter only
   needs a *superset* of XID, since it does not validate).

Flip each `tests/gap_validation.rs` registry entry to `currently_supported =
true` as it lands.

## Phase 2 — Structural grammar audit (the real parsing risk)

A **formatter** has it easier than the `ron` deserializer: it needs only surface
syntax, not semantic struct-type resolution. The official parser's O(N)
lookahead exists to feed serde the right `deserialize_*` call; here we only need
"does this `(...)` contain `name:` separators or not."

- Write adversarial cases (generate from the oracle): `(a)`, `(a,)`, `(a, b)`,
  `(a: 1)`, `Name(a, b)`, `Name(a: 1)`, nested `(a: (b: (c: 1)))`, empty `()`.
- Current grammar (`ron.pest:65-76`) splits `tuple_type` vs `fields_type` as
  ordered alternatives → PEG backtracking. If backtracking proves slow or wrong,
  **refactor to one unified paren rule** that records per-entry whether a name
  was present: `paren_entry = { ident ~ ":" ~ value | value }`. This sidesteps
  the ordered choice entirely.
- Add `#![type(...)]` / `#![schema(...)]` attributes (currently only
  `enable(...)` at `ron.pest:13`); preserve verbatim.

## Phase 3 — Comment capture + attachment (the differentiating work)

**(a) Make comments visible in the parse tree.** pest auto-inserts the built-in
`COMMENT` rule between tokens of non-atomic rules. It is currently silent
(`COMMENT = _{ … }`, `ron.pest:3`). Make it non-silent (`COMMENT = { … }`) so
matches appear as pairs with readable spans. *Verify* pest still auto-inserts a
non-silent `COMMENT`; if not, reference it explicitly where needed.

**(b) Attachment model.** Standard rustfmt/prettier rules, decided by source
line numbers (the CST must therefore carry spans — `Pair::as_span()` gives
line/col):

- **Trailing**: comment on the *same line* as the preceding token → attach to
  that node (`x: 1, // trailing`).
- **Leading**: comment on its own line(s) → attach to the *next* node.
- **Dangling**: comment inside an empty/sparse container with no node to bind to
  (`[ // only comment\n ]`) → attach to the enclosing bracket and emit inline.

This forces an **AST restructure**: `Value` must carry `span`,
`leading_comments`, `trailing_comments`, not just `Kind`. Atom children keep
their literal text verbatim (so `0xFF` stays `0xFF`).

**Gate:** property test that comment count and relative ordering survive a
format.

## Phase 4 — Column-aware printer

Replace the greedy heuristic (`display.rs:27`, `tabs*tab + len > width`, which
never knows the real column) with a Wadler/Leijen `Doc` algebra
(`text / line / group / nest`) plus a `best(width, col)` renderer that does the
real `fits()` check. ~150–300 lines, dependency-free, well-understood. Two
cleanups fall out:

- Delete the precomputed-length field and the `+2`/`+4` hacks
  (`ast/mod.rs:52,66,85,105`) — `Doc` rendering measures inline.
- Kill the global atomics (`src/lib.rs:7-8`); thread a
  `Config { tab_size, max_width }` through, so `format_ron(input, &config)` is
  composable/thread-safe.

## Phase 5 — Robustness & polish

- `src/main.rs:26` `.expect("unable to parse RON")` panics on bad input.
  Surface the pest span as a readable error, exit non-zero.
- Run the ron crate's test corpus + a quick fuzz generating random valid RON
  via the oracle; assert fmtron accepts and preserves semantics.

## Recommended sequence

Phase 0 first (cheap, protects everything after). Then Phase 1 (low-risk, fully
covered by the oracle test). Defer Phase 3 until 1 and 2 are solid, since the
AST restructure touches everything. Write Phase 2's adversarial tests early so a
wrong PEG formulation fails loudly.

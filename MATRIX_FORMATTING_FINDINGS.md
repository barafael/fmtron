# fmtron golden matrix — formatting-quality findings

Audited the golden test matrix of `tests/fmt_ron_matrix.rs`: the 109-file corpus in
`test_data/ron_corpus/` formatted under every layout-relevant config. Only `max_width`,
`tab_size` and `blank_lines` change layout (`max_depth`/`max_tab` only reject, and
`style_edition` has a single value), so the matrix reduces to **385 unique settings**
(the manifest's `total_settings`) × 109 inputs = **41,965 cells**. The matrix was then
**broadened with 59 new configs** (odd tabs, width-boundary probes, extreme tabs, extra
`remove` variants) for another 6,431 cells — **48,396 cells total**.

Approach: a driver re-formats the whole corpus under each config and dumps every
output; scanners then look for layouts that are valid but read poorly, and every finding
is pinned to a concrete config + input. The original driver and scanners lived outside
the repo and are gone; they are rebuilt as `cargo xtask matrix-audit` (see
[Reproducing](#reproducing)).

This is an aesthetic audit, not a correctness audit: every output below is valid RON and
a faithful re-format. See [Tested and passed](#tested-and-passed) for what came back clean.

Every example below is reproducible with the binary at this checkout (fmtron 1.0.0-rc.2,
commit `dc5a3d9`): the **Command** line shows the exact invocation (`-d` prints to stdout
instead of rewriting the file), and the config is stated both as CLI flags and as the
equivalent `fmt.ron` fields. Click an example to expand.

---

## Summary

| # | Severity | Finding | Cells hit |
|---|----------|---------|-----------|
| F1 | **Medium** | When indentation alone reaches `max_width`, the formatter still breaks containers that would fit inline — nearly every line overflows either way, but the output multiplies in lines (up to 4.2×) | 3,079 |
| F2 | **Low** | A newtype overrunning the width by ≤ `tab_size` chars is split into a 3-line `Some(…)` with a trailing comma, instead of overrunning by one character | 46 |
| F3 | **Low** | A block comment written before a map-key colon is relocated past the colon and merged into a run with the value's leading comment | 1 input class |
| F4 | **Low** | Wrapping an over-long packed row strands a single element on its own line (`3/1/3/1` instead of `2/2/2/2`) | 107 |
| F5 | **Low** | A comment written after an opening delimiter is detached from that delimiter and reads as a comment on the first child instead | 1 input class |

F1 is the only finding that changes line layout at an otherwise ordinary 100-column
setting (e.g. `w100` + `t24` on deep input); F2 and F4 are narrow-width issues, while F3
and F5 are comment-placement polish.

### Status

Resolved after the audit (see CHANGELOG.md, Unreleased):

- **F1** — fixed with a rule that keeps widening monotone: a line's width is `max_width`,
  but at least half of `max_width` past its indentation. The rule proposed below
  ("inline once indent ≥ `max_width`") would have made `edge_tuples.ron` at `t32` take
  5 lines at `w32` but 21 at `w40`. `edge_tuples.ron` at `w32-t32` is now 9 lines (was
  21), `edge_depth_boundary.ron` at `w100-t24` 11 (was 15). The proposed output for the
  latter also joined a map its input breaks, which the stay-broken rule forbids.
- **F2** — closed as by design: the split follows the documented rule, and any width
  limit has a column where the layout flips.
- **F3**, **F5** — fixed: those comments keep their anchors.
- **F4** — fixed: a wrapped row is split into lines as even as the fewest lines allow.
- **Width-metric note** — fixed: `tests/matrix_invariants.rs` measures display columns.

---

## F1 — Pointless splitting when indentation eats the whole width budget

**Severity: Medium — 4× line-count blowup with zero width benefit; nearly every output
line still overflows.**

The break rule (README:141) is "a container goes on one line if it fits within the line
width, and otherwise puts one element per line". When the current indent is already at or
past `max_width`, nothing can ever fit, so *everything* breaks — even elements that would
fit comfortably in a normal file — and the result overflows the width on nearly every
line while also being several times longer than necessary.

<details>
<summary><b>Example:</b> <code>edge_tuples.ron</code> at <code>max_width: 32, tab_size: 32</code> — 21 lines, all overflowing</summary>

**Config** — `fmt.ron`, CLI equivalent `-w 32 -t 32`:

```rust
(
    max_width: 32,
    tab_size: 32,
)
```

**Command**

```console
$ ./target/release/fmtron -d -w 32 -t 32 test_data/ron_corpus/edge_tuples.ron
```

**Input** — `test_data/ron_corpus/edge_tuples.ron` (5 lines, fits any normal width)

```rust
(
    pair: (1, 2),
    triple: ("a", 2, true),
    nested: ((1, 2), (3, 4)),
)
```

**Output** — 21 lines; every line but the outer brackets exceeds 32 columns (max 98)

```rust
(
                                pair: (
                                                                1,
                                                                2,
                                ),
                                triple: (
                                                                "a",
                                                                2,
                                                                true,
                                ),
                                nested: (
                                                                (
                                                                                                1,
                                                                                                2,
                                                                ),
                                                                (
                                                                                                3,
                                                                                                4,
                                                                ),
                                ),
)
```

The `1,` under `pair` sits at column 65 and the deepest one at column 97 — of a 32-column
budget. Inlining `pair: (1, 2),` would overrun by 13 columns; the split overruns by 34
and takes 4 lines instead of 1.

**Better (proposed)** — once the indent alone has eaten the width, a split cannot help,
so keep leaf containers inline. Same overflow class, 5 lines instead of 21:

```rust
(
                                pair: (1, 2),
                                triple: ("a", 2, true),
                                nested: ((1, 2), (3, 4)),
)
```

</details>

<details>
<summary><b>Example:</b> same input at <code>max_width: 32, tab_size: 2</code> — the baseline the formatter could stay close to</summary>

**Config** — `fmt.ron`, CLI equivalent `-w 32 -t 2`:

```rust
(
    max_width: 32,
    tab_size: 2,
)
```

**Command**

```console
$ ./target/release/fmtron -d -w 32 -t 2 test_data/ron_corpus/edge_tuples.ron
```

**Output** — 5 lines, all within 32 columns (identical input as the previous example)

```rust
(
  pair: (1, 2),
  triple: ("a", 2, true),
  nested: ((1, 2), (3, 4)),
)
```

**Better (proposed)** — nothing to change here; this baseline is shown to mark the gap
the `t32` output above falls short of. The proposed fix for F1 brings the degenerate
configs toward this shape.

</details>

<details>
<summary><b>Example:</b> <code>edge_depth_boundary.ron</code> at <code>max_width: 100, tab_size: 24</code> — realistic width, no extreme config needed</summary>

**Config** — `fmt.ron`, CLI equivalent `-w 100 -t 24`:

```rust
(
    max_width: 100,
    tab_size: 24,
)
```

**Command**

```console
$ ./target/release/fmtron -d -w 100 -t 24 test_data/ron_corpus/edge_depth_boundary.ron
```

**Input** — `test_data/ron_corpus/edge_depth_boundary.ron`

```rust
[
    [
        [
            [
                {
                    "deep": [1, 2, 3],
                },
            ],
        ],
    ],
]
```

**Output** — 15 lines; the innermost five lines overrun the 100-column budget (max 146
columns) and are almost pure indentation — the indent ladder climbs 24 → 48 → 72 → 96 →
120 → 144:

```rust
[
                        [
                                                [
                                                                        [
                                                                                                {
                                                                                                                        "deep": [
                                                                                                                                                1,
                                                                                                                                                2,
                                                                                                                                                3,
                                                                                                                        ],
                                                                                                },
                                                                        ],
                                                ],
                        ],
]
```

Even the innermost `"deep": [1, 2, 3],` — 18 columns of content — is exploded into 5
lines, where nothing can fit.

**Better (proposed)** — stop breaking once `indent ≥ max_width`; the innermost map and
list have no chance of fitting broken *or* inline, and inline is 30 columns narrower and
5 lines fewer:

```rust
[
                        [
                                                [
                                                                        [
                                                                                                {"deep": [1, 2, 3]},
                                                                        ],
                                                ],
                        ],
]
```

</details>

Same input, `w16-t16`, `edge_packing_mixed.ron`: 34 lines where every line but the outer
brackets overflows (max 50 columns), where `w16-t2` needs 18 and the input itself is 5
lines. The broadened matrix reproduced the pattern at `w13-t13`, `w2-t4`, `w3-t4` and
`w100-t64`.

**Scale:** 3,079 of 48,396 cells (2,502 existing + 577 extended) contain at least one
line whose leading indentation alone is ≥ `max_width`; 2,497 cells run `tab_size` ≥
`max_width` outright. Worst measured line-count ratio vs the same width at `t2`: 4.2×
(`edge_tuples` at `w32-t32`).

**Suggested fix:** a split is only worth making if it can help — once
`indent + smallest_element_width ≥ max_width` (or simply `indent ≥ max_width`), keep
containers inline instead of exploding them. This preserves the documented rule where it
matters and only changes the degenerate corner.

---

## F2 — One-character overrun explodes a newtype into three lines

**Severity: Low — valid, but a 13-char expression becomes 3 lines to save 1 column.**

<details>
<summary><b>Example:</b> <code>edge_nested_options.ron</code> at <code>max_width: 16, tab_size: 4</code> — 1 char over triggers a 3-line split</summary>

**Config** — `fmt.ron`, CLI equivalent `-w 16 -t 4`:

```rust
(
    max_width: 16,
    tab_size: 4,
)
```

**Command**

```console
$ ./target/release/fmtron -d -w 16 -t 4 test_data/ron_corpus/edge_nested_options.ron
```

**Input** — `test_data/ron_corpus/edge_nested_options.ron`

```rust
(
    one: Some(1),
    nested: Some(Some("value")),
    none: None,
    list: [Some(1), None, Some(2)],
)
```

**Output** — `one: Some(1),` at indent 4 is 17 columns, one past the 16-column budget,
and becomes 3 lines:

```rust
(
    one: Some(
        1,
    ),
    nested: Some(Some(
        "value",
    )),
    none: None,
    list: [
        Some(1),
        None,
        Some(2),
    ],
)
```

The split "succeeds" (all three lines fit) but reads worse than a one-character overrun
of a 13-character expression, and the trailing comma after `1` is noise on a newtype.
The soft-width contract (README:104, "long or deeply nested values may overrun it")
already tolerates overruns; this is the formatter choosing to split where an overrun is
the friendlier option.

**Better (proposed)** — keep the newtype inline and overrun by one column; the rest of
the file is unaffected:

```rust
(
    one: Some(1),
```

</details>

<details>
<summary><b>Contrast:</b> same input at <code>max_width: 16, tab_size: 3</code> — one column of indent decides it</summary>

**Config** — `fmt.ron`, CLI equivalent `-w 16 -t 3`:

```rust
(
    max_width: 16,
    tab_size: 3,
)
```

**Command**

```console
$ ./target/release/fmtron -d -w 16 -t 3 test_data/ron_corpus/edge_nested_options.ron
```

**Output** — indent 3 puts the inline field at exactly 16 columns, so it stays joined:

```rust
(
   one: Some(1),
   nested: Some(Some(
      "value",
   )),
   none: None,
   list: [
      Some(1),
      None,
      Some(2),
   ],
)
```

**Better (proposed)** — the `t3` rendering is the shape `t4` should also produce: the
single-column difference between 16 and 17 should not flip a field from one line to
three.

</details>

**Scale:** 339 three-line `Some(<scalar>)` splits across the matrix; 46 of them are
questionable in this sense (the inline form fits within `max_width + tab_size`, i.e. the
split is triggered by an overrun of at most one indent step). Concentrated in
`edge_nested_options.ron` and the `ron_036`/`ron_037` fixtures; reproduced in the
extended set at `w13-t4`.

**Suggested fix:** when breaking a single-element newtype would only shave the overrun by
less than one indent step (or the split's own lines are barely shorter than the inline
form), prefer keeping it inline and overrunning.

---

## F3 — Comment before a map-key colon is relocated past the colon

**Severity: Low — comment text and order survive, but its anchor token changes.**

<details>
<summary><b>Example:</b> <code>edge_map_key_comments.ron</code> at the default <code>max_width: 100, tab_size: 4</code> — no narrow width needed</summary>

**Config** — `fmt.ron` (defaults), CLI equivalent `-w 100 -t 4`:

```rust
(
    max_width: 100,
    tab_size: 4,
)
```

**Command**

```console
$ ./target/release/fmtron -d -w 100 -t 4 test_data/ron_corpus/edge_map_key_comments.ron
```

**Input** — `test_data/ron_corpus/edge_map_key_comments.ron`

```rust
{
    /* key */ "alpha" /* colon */: /* value */ 1,
    "beta": 2, // trailing
}
```

**Output** — every width and tab behaves the same:

```rust
{
    /* key */
    "alpha": /* colon */ /* value */ 1,
    "beta": 2, // trailing
}
```

Two relocations: `/* key */` is promoted from same-line to its own line (consistent with
the leading-comment model), and `/* colon */` — written *before* the colon — is moved
*after* it and rendered adjacent to `/* value */`, so the reader sees a
`/* colon */ /* value */` run that the author never wrote together. The comment's visual
contract ("this annotates the colon/key boundary") is lost. Root cause: the AST attaches
comments as leading/inner/trailing of elements; there is no slot between a key and its
colon, so a key-trailing comment falls to the value side.

**Better (proposed)** — when the key stays on the same line as its colon, emit
key-trailing comments before the colon, keeping each comment next to the token the author
anchored it to:

```rust
{
    /* key */
    "alpha" /* colon */: /* value */ 1,
    "beta": 2, // trailing
}
```

</details>

**Scale:** one corpus input exercises this (`edge_map_key_comments.ron`, all configs);
adjacent-comment runs like this can appear in any map with both key- and value-attached
block comments. `edge_block_comments_fields.ron` was checked and is *not* affected — its
comments keep their anchors.

**Suggested fix:** when the key stays on the same line as its colon, emit key-trailing
comments before the colon.

---

## F4 — Greedy wrap strands single elements on their own row

**Severity: Low — documented trade-off, but the wrap could rebalance instead.**

<details>
<summary><b>Example:</b> <code>edge_packing_boundary_16.ron</code> at <code>max_width: 24, tab_size: 2</code> — a 3/1/3/1 staircase of stranded singles</summary>

**Config** — `fmt.ron`, CLI equivalent `-w 24 -t 2`:

```rust
(
    max_width: 24,
    tab_size: 2,
)
```

**Command**

```console
$ ./target/release/fmtron -d -w 24 -t 2 test_data/ron_corpus/edge_packing_boundary_16.ron
```

**Input** — `test_data/ron_corpus/edge_packing_boundary_16.ron` (four packed atoms per
input line)

```rust
[
    "aa", "bb", "cc", "dd",
    "ee", "ff", "gg", "hh",
]
```

**Output** — each input line is 25 columns at the new indent 2, one over the budget, and
the greedy wrap (README:144: input line breaks between packed atoms are kept, only
over-long lines are wrapped) strands one element per row:

```rust
[
  "aa", "bb", "cc",
  "dd",
  "ee", "ff", "gg",
  "hh",
]
```

The `3/1/3/1` shape is uneven. Global greedy packing would give `3/3/2` — but that would
reflow neighbouring input lines, which the documented rule deliberately refuses to do
("editing one line never reflows the others"). The finding is that the *within-line* wrap
need not be greedy.

**Better (proposed)** — rebalance the wrapped segments of each input line into `2/2`
(13 columns each): even rows, still within budget, and the reflow-stability guarantee is
untouched because no element crosses an input line break:

```rust
[
  "aa", "bb",
  "cc", "dd",
  "ee", "ff",
  "gg", "hh",
]
```

</details>

**Scale:** 107 cells (99 existing + 8 extended) end a broken packed list with a lone
element after multi-element rows; concentrated in `edge_packing_mixed.ron`,
`edge_packing_boundary_16/17.ron` and `edge_nested_options.ron`.

---

## F5 — Comments after an opening delimiter lose their anchor

**Severity: Low — text is preserved, but the comment now appears to describe the wrong
thing.**

This is the same class of anchor drift as F3, but at `(`/`{`/`[` rather than at a map-key
colon.

<details>
<summary><b>Example:</b> <code>ron_113_64fd2990fd3d.ron</code> at the default <code>max_width: 100, tab_size: 4</code> — three opener comments become child comments</summary>

**Config** — `fmt.ron` (defaults), CLI equivalent `-w 100 -t 4`:

```rust
(
    max_width: 100,
    tab_size: 4,
)
```

**Command**

```console
$ ./target/release/fmtron -d -w 100 -t 4 test_data/ron_corpus/ron_113_64fd2990fd3d.ron
```

**Input** — `test_data/ron_corpus/ron_113_64fd2990fd3d.ron`, verbatim (comments on the
opening lines of a struct, a map and a list; the file also starts with a blank line and
ends with a whitespace-only line, both normalized away):

```rust

        Scene( // class name is optional
            materials: { // this is a map
                "metal": (
                    reflectivity: 1.0,
                ),
                "plastic": (
                    reflectivity: 0.5,
                ),
            },
            entities: [ // this is an array
                (
                    name: "hero",
                    material: "metal",
                ),
                (
                    name: "monster",
                    material: "plastic",
                ),
            ],
        )
```

**Output** — all widths and tabs behave the same:

```rust
Scene(
    // class name is optional
    materials: {
        // this is a map
        "metal": (
            reflectivity: 1.0,
        ),
        "plastic": (
            reflectivity: 0.5,
        ),
    },
    entities: [
        // this is an array
        (
            name: "hero",
            material: "metal",
        ),
        (
            name: "monster",
            material: "plastic",
        ),
    ],
)
```

Each comment still survives in the same relative position, but its visual anchor changes:

- `// class name is optional` was written after `Scene(`; it now sits immediately above
  `materials:` and reads like a field comment.
- `// this is a map` was attached to the map opener; it now sits above the first map
  entry, where it can read as a comment on the `"metal"` entry.
- `// this is an array` was attached to the list opener; it now sits above the first
  entity tuple and reads like a comment on that tuple.

The container was going to break anyway because it contains comments, so keeping the
comment on the opening line costs no extra layout. The current model has no slot for a
trailing comment on an opening delimiter, so it falls through as the leading comment of
the first child.

**Better (proposed)** — emit the comment as a trailing comment on the opener, preserving
the author's anchoring (already valid RON, and idempotent under re-format):

```rust
Scene( // class name is optional
    materials: { // this is a map
        "metal": (
            reflectivity: 1.0,
        ),
        ...
    },
    entities: [ // this is an array
        (
            name: "hero",
            material: "metal",
        ),
        ...
    ],
)
```

</details>

**Scale:** one input class, but a wide one: `ron_113` is formatted in 375 of the 444
matrix configs (318 existing + 57 extended); it is rejected only by the small-depth
configs. No width/tab interaction.

**Suggested fix:** when a comment follows the opening delimiter on the same input line
and the container is multi-line, emit it as a trailing comment on the opener. If this is
intentionally normalized to the leading-child model, it should be stated in the style
docs alongside the F3 behavior, because both are surprising to a reader who cares about
comment placement.

---

## Width-metric note — the invariant is looser than the formatter

This is a checker observation, not a layout finding.

The formatter measures display columns with `unicode-width` (`src/pretty.rs`), so its fit
decisions count CJK and emoji as two columns. `tests/matrix_invariants.rs` instead measures
with `line.chars().count()`. That makes the matrix's overrun check more permissive than the
formatter for wide characters.

<details>
<summary><b>Example:</b> <code>edge_unicode_strings.ron</code> at <code>max_width: 24, tab_size: 4</code> — a line that fits as characters but not as columns</summary>

**Config** — `fmt.ron`, CLI equivalent `-w 24 -t 4`:

```rust
(
    max_width: 24,
    tab_size: 4,
)
```

**Command**

```console
$ ./target/release/fmtron -d -w 24 -t 4 test_data/ron_corpus/edge_unicode_strings.ron
```

**Input** — `test_data/ron_corpus/edge_unicode_strings.ron`

```rust
(
    wide: "日本語のテキスト",
    emoji: "🦀🦀🦀",
    combining: "e\u{301}",
)
```

**Output** — identical to the input:

```rust
(
    wide: "日本語のテキスト",
    emoji: "🦀🦀🦀",
    combining: "e\u{301}",
)
```

The `wide:` line is 21 scalar values but **29 display columns** — over the 24-column
budget by display width, under it by character count. The invariant accepts it because a
quoted atom is allowed to overrun, and in this case the overrun is unavoidable: the
string atom itself is wider than the remaining 14 columns, and fmtron does not split a
key from its value. The same applies to `// 日本語のコメント` at `w13-t4` (11 scalars,
19 display columns), which is an unbreakable comment. Across the dumps, 45 cells
(42 existing + 3 extended) contain 60 lines whose scalar count fits but whose display
width exceeds `max_width`.

**Suggestion** — no formatting defect to fix: every observed case is an unbreakable
string or comment, which the soft-width contract already permits. But if the matrix
invariant is meant to police the README's "columns", it should reuse the formatter's
display-width metric (and define a policy for combining marks and ZWJ emoji); as written
it would not catch a future display-width regression in a *breakable* line.

</details>

---

## Ordinary-settings spot check

At `w100-t4` (default width/tab, both blank-line modes), the full 109-input corpus reads
consistently with the documented style: one-line containers join cleanly, broken
structs/maps stay broken, wrapper hugging (`Some((`/`Some([`) is legible, blank lines
survive exactly, comments and doc comments keep their general positions, raw/binary
strings are untouched, and the `ron_*` fixtures produce no additional layout class beyond
the findings above. The default setting is not where the odd output lives; narrow widths,
large tabs, and the two comment-anchor cases are.

A few intentional trade-offs are worth recording because they look odd at first:

<details>
<summary><b>Example:</b> <code>ron_035_c7053bd47919.ron</code> at <code>max_width: 56, tab_size: 4</code> — strings resist vertical packing</summary>

**Config** — `fmt.ron`, CLI equivalent `-w 56 -t 4`:

```rust
(
    max_width: 56,
    tab_size: 4,
)
```

**Command**

```console
$ ./target/release/fmtron -d -w 56 -t 4 test_data/ron_corpus/ron_035_c7053bd47919.ron
```

**Input / Output** — the input already lists the strings one per line; the output keeps
them that way (only the missing trailing comma is added):

```rust
BuildSystem(
    version: "1.0.0",
    flags: [
        "--enable-thing",
        "--enable-other-thing",
        ("some-conditional", ["--enable-third-thing"]),
    ],
)
```

The first two strings would fit together (`"--enable-thing", "--enable-other-thing",` is
49 columns at indent 8, within 56), and the shape looks under-packed. This follows the
documented rule that strings/identifiers written one per line stay that way — a stability
choice, not a packing bug; numeric atoms in the same position would pack.

**Better (if packing strings were desired)** — it would look like this, but adopting it
would reflow the author's lines, which the style deliberately avoids:

```rust
    flags: [
        "--enable-thing", "--enable-other-thing",
        ("some-conditional", ["--enable-third-thing"]),
    ],
```

</details>

<details>
<summary><b>Example:</b> <code>edge_depth_boundary.ron</code> at the default <code>max_width: 100, tab_size: 4</code> — a broken child pins its parents</summary>

**Config** — `fmt.ron` (defaults), CLI equivalent `-w 100 -t 4`:

```rust
(
    max_width: 100,
    tab_size: 4,
)
```

**Command**

```console
$ ./target/release/fmtron -d -w 100 -t 4 test_data/ron_corpus/edge_depth_boundary.ron
```

**Output** — 11 lines for what is a 27-column expression:

```rust
[
    [
        [
            [
                {
                    "deep": [1, 2, 3],
                },
            ],
        ],
    ],
]
```

Every bracket in the input is followed by a line break, so the stay-broken/grid rules
pin each level open even though the whole file would fit on one line. This is consistent
with the documented rules, even though the bracket ladder is visually tall.

**Better (author-side)** — the formatter is faithful to the input's anchoring; to get the
compact form, write the input inline and fmtron will keep it there (verified with the
same command):

```rust
[[[[{"deep": [1, 2, 3]}]]]]
```

</details>

- **Unbreakable atoms overrun.** Long identifiers, long strings, comments, and attributes
  are emitted whole rather than split; the matrix invariant permits the overrun for those
  reasons. This is the correct trade-off (splitting would change syntax or meaning), but it
  means narrow-width outputs are not uniformly within the nominal width.

---

## Broadened matrix

59 new configs were added on top of the 385 (dumps under `/tmp/opencode/out/extended/`):

- **odd tab sizes** at `w100`: `t3, t5, t6, t7, t9, t10, t12`
- **width-boundary probes**: `w63`/`w65` (around 64) and `w99`/`w100`/`w101` (around 100)
  at several tabs, to catch flips exactly at the soft limit
- **extreme tabs**: `t20, t24, t32, t40, t48, t64` at `w100`/`w120`/`w512`/`w1000000`/
  unlimited width, and `w1-t64`
- **tiny widths**: `w2-t1`, `w2-t4`, `w3-t4`, `w5-t4`, `w13-t4`, `w13-t13`
- **mid widths**: `w40-t2/t8`, `w64-t2/t4/t8`, `w96-t4`, `w120-t2/t8`, `w128-t4`,
  `w200-t4`, `w512-t4/t24`
- **extra `blank_lines: remove` variants** across the above
- **small depths** `d1, d2, d3, d5, d7` at `w100-t4`

Observations:

- F1, F2 and F5 reproduce in the extended set (`w13-t13`, `w2-t4`, `w3-t4`, `w100-t64`,
  `w13-t4`). The width-metric caveat also shows up there in three cells
  (`edge_unicode_comments.ron` at `w13-t4`/`w13-t13`). No *new* anomaly class appeared —
  the extended configs mostly confirmed that behaviour between sampled widths is stable
  (the `w63`/`w65` and `w99`/`w100`/`w101` flips are pure width arithmetic, nothing
  config-specific).
- `blank_lines: remove` never interacted with width or tab in a way `keep` did not
  already show.
- Idempotency, reference-parser validity and width-monotonicity held on all 6,431 new
  cells exactly as on the existing 41,965.

---

## Tested and passed

Run over all 48,396 cells (both sets):

- **Idempotency** — re-formatting every output under the same config reproduces it
  byte-for-byte: 0 failures.
- **Validity** — every dumped output (44,995; rejected inputs have no output) re-parses
  with the reference `ron` crate: 0 failures.
- **Monotonicity** — across 16,786 width-increase comparisons (same input, tab, blank-line
  mode), line count never increases: 0 violations.
- **Blank lines / whitespace hygiene** — no stray trailing whitespace, no blank line at
  file start/end or after an opening bracket, `remove` mode leaves zero blank lines.
- **Depth/tab ceilings** — `max_depth`/`max_tab` reject exactly as configured and never
  alter layout of inputs that parse.

The overrun check in `tests/matrix_invariants.rs` uses scalar counts, not the formatter's
display-width metric; see the [width-metric note](#width-metric-note--the-invariant-is-looser-than-the-formatter).

## By design (not findings)

Confirmed intentional per README/style rules, listed to close them out: struct/map inputs
that break after the opening bracket stay broken (`Dbg(`, `Scene`); `Some((`/`Some([`
hugging; number/atom packing rules; blank lines dropped after open brackets; attribute,
string and identifier atoms overrun narrow widths unbroken (they are unbreakable atoms);
`TooDeep` rejections at small `max_depth`.

## Reproducing

Every example above is a direct `fmtron -d` invocation and can be run as shown. The bulk
audit is `cargo xtask matrix-audit --extended` (the `--extended` settings are the
broadened matrix; `--dump <dir>` writes every output to `<dir>/<row>/<input>`). It
checks validity against the reference `ron` crate, idempotency, width monotonicity and
line hygiene, and prints the scan counts the findings above are based on. CI runs it.
Counts differ somewhat from the numbers above, which came from the original, lost
scanners.

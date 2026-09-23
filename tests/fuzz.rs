//! Seeded fuzz test: generate valid RON documents with a deterministic PRNG
//! and check the printer on each one.
//!
//! Override the seed with `FMTRON_FUZZ_SEED=...` (decimal or `0x…` hex) to
//! explore new inputs; a failure message always reports the seed, so any
//! failing case is reproducible.
//!
//! Properties checked per generated document:
//! - `format_ron` succeeds (the generator only produces grammar-valid RON);
//! - every output line is at most `max_width` characters — the generator's
//!   atoms and nesting depth are bounded so that the widest *unbreakable*
//!   line (an atom-key/atom-value map entry at the deepest indent, or a
//!   struct field line) still fits the smallest tested width;
//! - formatting is semantics-preserving (the `ron` crate is the oracle);
//! - formatting is idempotent.

mod support;

use fmtron::{Config, format_ron};
use support::pretty::max_line_len;

const DEFAULT_SEED: u64 = 0xC0FFEE;
/// Containers nest at most this deep; the deepest indent is `4 * MAX_DEPTH`.
/// Kept at 3 so `4*3` columns of indent plus the widest atom map entry
/// (`6 + ": " + 7 + ","`) stays within the smallest tested width.
const MAX_DEPTH: usize = 3;
const ITERATIONS: usize = 400;

const IDENTS: &[&str] = &["a", "b", "x1", "y", "foo", "bar", "P", "Pt", "T", "r#type"];
const STRINGS: &[&str] = &[
    "\"\"",
    "x",
    "hi",
    "abc",
    "yolo",
    "\"a\\nb\"",
    "r#\"\"#",
    "r#\"q\"#",
];
const NUMBERS: &[&str] = &[
    "0", "1", "-7", "42", "0x1F", "0b101", "0o17", "0o755", "0xAbC", "255u8", "100u64", "-128i8",
    "1e3", "0.5", "-2.5", "1e-3", "1e300", "inf", "-inf", "NaN", "1.5f32", "2.0f64", "7f32",
    "1f64", "-.25", ".5",
];
const BYTES: &[&str] = &["b\"\"", "b\"ab\"", "br#\"\"#"];
const CHARS: &[&str] = &["'a'", "'\\''", "'\\n'", "'z'", "'\\u{7}'"];

fn seed() -> u64 {
    match std::env::var("FMTRON_FUZZ_SEED") {
        Ok(s) => {
            let stripped = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"));
            match stripped {
                Some(hex) => u64::from_str_radix(hex, 16),
                None => s.parse::<u64>(),
            }
            .unwrap_or_else(|_| panic!("FMTRON_FUZZ_SEED must be an integer, got {s:?}"))
        }
        Err(_) => DEFAULT_SEED,
    }
}

/// xorshift64* PRNG — deterministic across platforms.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, pct: usize) -> bool {
        self.below(100) < pct
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

fn leaf(rng: &mut Rng) -> String {
    match rng.below(6) {
        0 => rng.pick(NUMBERS).to_string(),
        1 => {
            if rng.chance(50) {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        2 => rng.pick(STRINGS).to_string(),
        3 => rng.pick(CHARS).to_string(),
        4 => rng.pick(BYTES).to_string(),
        _ => "None".to_string(),
    }
}

fn value(rng: &mut Rng, depth: usize) -> String {
    if depth >= MAX_DEPTH || rng.chance(55) {
        return leaf(rng);
    }
    match rng.below(6) {
        0 => format!("Some({})", value(rng, depth + 1)),
        1 => {
            let n = rng.below(5);
            let items: Vec<String> = (0..n).map(|_| value(rng, depth + 1)).collect();
            format!("[{}]", items.join(", "))
        }
        2 => {
            let n = rng.below(4);
            let mut items = Vec::new();
            for _ in 0..n {
                items.push(format!("{}: {}", leaf(rng), value(rng, depth + 1)));
            }
            format!("{{{}}}", items.join(", "))
        }
        3 => {
            let n = rng.below(4);
            let items: Vec<String> = (0..n).map(|_| value(rng, depth + 1)).collect();
            if rng.chance(30) {
                format!("{}({})", rng.pick(IDENTS), items.join(", "))
            } else {
                format!("({})", items.join(", "))
            }
        }
        _ => {
            let n = rng.below(4);
            let mut items = Vec::new();
            for _ in 0..n {
                items.push(format!("{}: {}", rng.pick(IDENTS), value(rng, depth + 1)));
            }
            if rng.chance(30) {
                format!("{}({})", rng.pick(IDENTS), items.join(", "))
            } else {
                format!("({})", items.join(", "))
            }
        }
    }
}

fn normalize(s: &str) -> String {
    s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
}

#[test]
fn seeded_fuzz_never_breaks_the_invariants() {
    let s = seed();
    let mut rng = Rng(s);
    let mut checked = 0;
    for width in [30usize, 60] {
        for _ in 0..ITERATIONS {
            let mut input = value(&mut rng, 0);
            if rng.chance(10) {
                input = format!("// head comment\n{input}");
            }
            let cfg = Config {
                tab_size: 4,
                max_width: width,
                ..Config::default()
            };
            let ctx = format!("seed {s}, width {width}");
            let out = format_ron(&input, &cfg)
                .unwrap_or_else(|e| panic!("{ctx}: format failed for {input:?}: {e}"));

            assert!(
                max_line_len(&out) <= width,
                "{ctx}: overran max_width for {input:?}:\n{out}"
            );

            let before: ron::Value = ron::from_str(&input).expect("generator emitted invalid RON");
            let after: ron::Value = ron::from_str(&out)
                .unwrap_or_else(|e| panic!("{ctx}: output unparseable for {input:?}\n{out}: {e}"));
            assert_eq!(before, after, "{ctx}: semantic drift for {input:?}\n{out}");

            let again = format_ron(&out, &cfg).expect("second formatting pass failed");
            assert_eq!(
                normalize(&out),
                normalize(&again),
                "{ctx}: not idempotent for {input:?}\n{out}"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no fuzz cases generated");
}

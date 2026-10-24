//! Checking fmtron's output against the reference `ron` crate: shared by the
//! wild-corpus evaluation and the matrix audit.

use fmtron::{Config, FormatError, format_ron};

/// The verdict on formatting one input, as `wild evaluate` records it.
pub enum Verdict {
    /// Output accepted by `ron`, same value, idempotent, same tokens in order.
    Ok(String),
    /// fmtron rejects the input: `TooDeep` or another error, with whether
    /// `ron` accepts it.
    Rejected {
        error: FormatError,
        ron: Result<(), String>,
    },
    Panic,
    Semantic,
    OutputInvalid(String),
    AcceptsInvalid(String),
    NotIdempotent(String),
    ContentChanged(String),
}

impl Verdict {
    /// The status and detail `results.jsonl` records.
    pub fn status(&self) -> (&'static str, String) {
        match self {
            Verdict::Ok(_) => ("ok", String::new()),
            Verdict::Rejected {
                error: FormatError::TooDeep { depth, .. },
                ..
            } => ("too-deep", depth.to_string()),
            Verdict::Rejected { error, ron } => {
                let msg = error.to_string().replace('\n', " ⏎ ");
                match ron {
                    Ok(()) => ("rejects-valid", msg),
                    Err(e) => ("both-reject", format!("{msg} || ron: {e}")),
                }
            }
            Verdict::Panic => ("panic", String::new()),
            Verdict::Semantic => ("semantic", String::new()),
            Verdict::OutputInvalid(e) => ("output-invalid", e.clone()),
            Verdict::AcceptsInvalid(e) => ("accepts-invalid", e.clone()),
            Verdict::NotIdempotent(d) => ("not-idempotent", d.clone()),
            Verdict::ContentChanged(d) => ("content-changed", d.clone()),
        }
    }
}

/// Format `input` and check the output: valid RON with the input's value,
/// unchanged by formatting again, and the same tokens in the same order.
/// Panics are caught; install a quiet panic hook to keep them off stderr.
pub fn check(input: &str, config: &Config) -> Verdict {
    let oracle_in = ron::from_str::<ron::Value>(input).map(norm);
    let out = match std::panic::catch_unwind(|| format_ron(input, config)) {
        Err(_) => return Verdict::Panic,
        Ok(Err(error)) => {
            return Verdict::Rejected {
                error,
                ron: oracle_in.map(drop).map_err(|e| e.to_string()),
            };
        }
        Ok(Ok(out)) => out,
    };
    match (&oracle_in, ron::from_str::<ron::Value>(&out).map(norm)) {
        (Ok(a), Ok(b)) if *a != b => return Verdict::Semantic,
        (Ok(_), Err(e)) => return Verdict::OutputInvalid(e.to_string()),
        (Err(e), _) => return Verdict::AcceptsInvalid(e.to_string()),
        _ => {}
    }
    match format_ron(&out, config) {
        Err(e) => Verdict::NotIdempotent(format!("reformat fails: {e}")),
        Ok(again) if again != out => Verdict::NotIdempotent(first_diff(&out, &again)),
        _ if skeleton(input) != skeleton(&out) => {
            Verdict::ContentChanged(first_diff(&skeleton(input), &skeleton(&out)))
        }
        _ => Verdict::Ok(out),
    }
}

/// Everything but whitespace and commas, in order: formatting may only change
/// those, so any other difference is lost, duplicated or moved content.
fn skeleton(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect()
}

/// `( )` (an empty sequence to `ron::Value`) and `()` (unit) deserialize
/// alike into every struct/tuple type, and fmtron normalizes the former to
/// the latter, so compare values with the two identified.
fn norm(v: ron::Value) -> ron::Value {
    use ron::Value::*;
    match v {
        Seq(s) if s.is_empty() => Unit,
        Seq(s) => Seq(s.into_iter().map(norm).collect()),
        Option(o) => Option(o.map(|b| Box::new(norm(*b)))),
        Map(m) => Map(m.into_iter().map(|(k, v)| (norm(k), norm(v))).collect()),
        other => other,
    }
}

/// The context around the first difference between `a` and `b`.
pub fn first_diff(a: &str, b: &str) -> String {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let i = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let ctx = |v: &[char]| {
        v[i.saturating_sub(15).min(v.len())..(i + 25).min(v.len())]
            .iter()
            .collect::<String>()
    };
    format!("in: {:?} | out: {:?}", ctx(&a), ctx(&b))
}

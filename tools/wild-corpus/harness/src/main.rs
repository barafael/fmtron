// Evaluate fmtron on real-world files. Reads file paths (one per line) on
// stdin, prints one tab-separated line per file:
//   path  status  detail
// status is one of: ok, not-utf8, rejects-valid, both-reject, accepts-invalid,
// semantic, output-invalid, not-idempotent, content-changed, too-deep, panic.
// Files are formatted at the default config (width 40, tab 4).
use std::io::BufRead;

/// Everything but whitespace and commas, in order: formatting may only change
/// those, so any other difference is lost, duplicated or moved content.
fn skeleton(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && *c != ',').collect()
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

fn first_diff(a: &str, b: &str) -> String {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let i = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let ctx = |v: &[char]| v[i.saturating_sub(15)..(i + 25).min(v.len())].iter().collect::<String>();
    format!("in: {:?} | out: {:?}", ctx(&a), ctx(&b))
}

fn main() {
    let cfg = fmtron::Config::default();
    std::panic::set_hook(Box::new(|_| {}));
    for path in std::io::stdin().lock().lines() {
        let path = path.unwrap();
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(input) = String::from_utf8(bytes) else {
            println!("{path}\tnot-utf8\t");
            continue;
        };
        let oracle_in = ron::from_str::<ron::Value>(&input).map(norm);
        let res = std::panic::catch_unwind(|| fmtron::format_ron(&input, &cfg));
        let (status, detail) = match res {
            Err(_) => ("panic", String::new()),
            Ok(Err(fmtron::FormatError::TooDeep { depth, .. })) => ("too-deep", depth.to_string()),
            Ok(Err(e)) => {
                let msg = e.to_string().replace('\n', " ⏎ ");
                match &oracle_in {
                    Ok(_) => ("rejects-valid", msg),
                    Err(oe) => ("both-reject", format!("{msg} || ron: {oe}")),
                }
            }
            Ok(Ok(out)) => {
                let oracle_out = ron::from_str::<ron::Value>(&out).map(norm);
                match (&oracle_in, &oracle_out) {
                    (Ok(a), Ok(b)) if a != b => ("semantic", String::new()),
                    (Ok(_), Err(e)) => ("output-invalid", e.to_string()),
                    (Err(e), _) => ("accepts-invalid", e.to_string()),
                    _ => match fmtron::format_ron(&out, &cfg) {
                        Err(e) => ("not-idempotent", format!("reformat fails: {e}")),
                        Ok(o2) if o2 != out => ("not-idempotent", first_diff(&out, &o2)),
                        _ if skeleton(&input) != skeleton(&out) => (
                            "content-changed",
                            first_diff(&skeleton(&input), &skeleton(&out)),
                        ),
                        _ => ("ok", String::new()),
                    },
                }
            }
        };
        println!("{path}\t{status}\t{}", detail.replace(['\t', '\n'], " "));
    }
}

use std::io::Write;
use std::process::Command;

fn run_cli(input: &str, args: &[&str]) -> std::process::Output {
    let mut file = tempfile::NamedTempFile::new().expect("temp file");
    write!(file, "{input}").expect("write temp file");
    Command::new(env!("CARGO_BIN_EXE_fmtron"))
        .args(["-i", file.path().to_str().unwrap()])
        .args(args)
        .output()
        .expect("run fmtron")
}

#[test]
fn invalid_input_is_a_clean_error_not_a_panic() {
    let out = run_cli("not valid ron ((((", &["-d"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unable to parse RON"));
    assert!(!stderr.contains("panicked"));
}

#[test]
fn missing_file_is_a_clean_error_not_a_panic() {
    let out = Command::new(env!("CARGO_BIN_EXE_fmtron"))
        .args(["-d", "-i", "/nonexistent/fmtron/no.ron"])
        .output()
        .expect("run fmtron");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unable to read"));
    assert!(!stderr.contains("panicked"));
}

#[test]
fn valid_input_succeeds_and_writes_formatted_output() {
    let out = run_cli("(a:1,b:[1,2,3])", &["-d"]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "(a: 1, b: [1, 2, 3])\n"
    );
}

#[test]
fn tab_size_above_max_tab_is_a_clean_error() {
    let out = run_cli("(a: 1)", &["-d", "-t", "2048"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("exceeds the --max-tab ceiling"));
    assert!(!stderr.contains("panicked"));
}

#[test]
fn max_tab_override_allows_larger_indentation() {
    let out = run_cli("(a: 1)", &["-d", "-t", "2048", "--max-tab", "4096"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn too_deep_input_is_a_clean_error() {
    let deep = format!("{}1{}", "[[[[[".repeat(120), "]]]]]".repeat(120));
    let out = run_cli(&deep, &["-d"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("levels deep"), "stderr: {stderr}");
    assert!(!stderr.contains("panicked"));
}

#[test]
fn max_depth_override_admits_deeper_input() {
    let deep = format!("{}1{}", "[[[[[".repeat(120), "]]]]]".repeat(120));
    let out = run_cli(&deep, &["-d", "--max-depth", "2048"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

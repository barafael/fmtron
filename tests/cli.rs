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
    // R6: a depth rejection is not a parse error, and names the CLI flag.
    assert!(stderr.contains("--max-depth"), "stderr: {stderr}");
    assert!(!stderr.contains("unable to parse"), "stderr: {stderr}");
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

// N8: output always ends in exactly one newline, both in-place and with -d,
// whether or not it ends in a comment.
#[test]
fn output_ends_with_exactly_one_newline() {
    for input in ["[1,2]", "[1,2] // c", "[1,2]\n// d\n"] {
        let out = run_cli(input, &["-d"]);
        assert!(out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.ends_with('\n') && !stdout.ends_with("\n\n"),
            "-d output for {input:?}: {stdout:?}"
        );

        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        write!(file, "{input}").expect("write temp file");
        let status = Command::new(env!("CARGO_BIN_EXE_fmtron"))
            .args(["-i", file.path().to_str().unwrap()])
            .status()
            .expect("run fmtron");
        assert!(status.success());
        let written = std::fs::read_to_string(file.path()).unwrap();
        assert_eq!(
            written, stdout,
            "in-place output differs from -d for {input:?}"
        );
        let _ = std::fs::remove_file(format!("{}.bak", file.path().display()));
    }
}

// M2: the CLI's final newline matches the input's line ending.
#[test]
fn crlf_input_gets_a_crlf_final_newline() {
    let out = run_cli("[1,\r\n2]\r\n", &["-d", "-w", "3"]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "[\r\n    1,\r\n    2,\r\n]\r\n"
    );
}

// M3: a huge --max-tab/--tab-size pair is a clean error, not a panic.
#[test]
fn huge_tab_size_is_a_clean_error() {
    let max = usize::MAX.to_string();
    let out = run_cli("[[1]]", &["-d", "-w", "1", "--max-tab", &max, "-t", &max]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--tab-size"), "stderr: {stderr}");
    assert!(!stderr.contains("panicked"), "stderr: {stderr}");
}

// M4: raising --max-depth admits input deeper than the default thread stack
// can handle, because the CLI sizes its stack to the limit; an absurd limit
// is a clean error.
#[test]
fn raised_max_depth_does_not_overflow_the_stack() {
    let depth = 5000;
    let input = format!("{}1{}", "[".repeat(depth), "]".repeat(depth));
    let out = run_cli(&input, &["-d", "-w", "100000", "--max-depth", "6000"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), input);

    let out = run_cli("[1]", &["-d", "--max-depth", &usize::MAX.to_string()]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--max-depth"), "stderr: {stderr}");
    assert!(!stderr.contains("panicked"), "stderr: {stderr}");
}

// W3: a reader that stops early (`fmtron -d … | head`) is not an error; the
// CLI used to panic with "failed printing to stdout: Broken pipe".
#[test]
fn closed_stdout_is_not_a_panic() {
    use std::io::Read;
    use std::process::Stdio;
    let mut file = tempfile::NamedTempFile::new().expect("temp file");
    write!(file, "[{}]", "1,".repeat(200_000)).expect("write temp file");
    let mut child = Command::new(env!("CARGO_BIN_EXE_fmtron"))
        .args(["-d", "-w", "5", "-i", file.path().to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run fmtron");
    let mut first = [0u8; 16];
    child.stdout.take().unwrap().read_exact(&mut first).unwrap();
    // stdout is dropped here, closing the pipe mid-output.
    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("panicked"), "stderr: {stderr}");
    assert!(
        out.status.success(),
        "status {:?}, stderr: {stderr}",
        out.status
    );
}

// S1: blank lines are kept by default; `--blank-lines remove` drops them.
#[test]
fn blank_lines_option() {
    let input = "(\n    a: 1,\n\n\n    b: 2,\n)";
    let keep = run_cli(input, &["-d"]);
    assert!(keep.status.success());
    assert_eq!(
        String::from_utf8_lossy(&keep.stdout),
        "(\n    a: 1,\n\n    b: 2,\n)\n"
    );
    let remove = run_cli(input, &["-d", "--blank-lines", "remove"]);
    assert!(remove.status.success());
    assert_eq!(String::from_utf8_lossy(&remove.stdout), "(a: 1, b: 2)\n");
}

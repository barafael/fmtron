use indoc::indoc;
use std::io::Write;
use std::process::Command;

fn run_cli(input: &str, args: &[&str]) -> std::process::Output {
    let mut file = tempfile::NamedTempFile::new().expect("temp file");
    write!(file, "{input}").expect("write temp file");
    Command::new(env!("CARGO_BIN_EXE_fmtron"))
        .args(args)
        .arg(file.path())
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
        .args(["-d", "/nonexistent/fmtron/no.ron"])
        .output()
        .expect("run fmtron");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no such file or directory"),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("panicked"));
}

/// A file with nothing to format is left as it is, and is not an error:
/// a formatter has no say on whether a file ought to hold a value.
#[test]
fn a_file_without_a_value_is_left_alone() {
    for input in [
        "",
        "\n\n",
        "   \n",
        "// TODO: fill in\n",
        "#![enable(implicit_some)]\n// soon\n",
    ] {
        let shown = run_cli(input, &["-d"]);
        assert!(
            shown.status.success(),
            "{input:?}: {}",
            String::from_utf8_lossy(&shown.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&shown.stdout), input, "{input:?}");
        assert!(
            shown.stderr.is_empty(),
            "{input:?}: {}",
            String::from_utf8_lossy(&shown.stderr)
        );

        let checked = run_cli(input, &["--check"]);
        assert!(checked.status.success(), "{input:?} --check");
        assert!(
            checked.stdout.is_empty(),
            "{input:?} --check printed a diff"
        );

        // In place: untouched, down to the modification time.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("empty.ron");
        std::fs::write(&file, input).unwrap();
        let before = std::fs::metadata(&file).unwrap().modified().unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_fmtron"))
            .arg(dir.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "{input:?} in place");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), input);
        assert_eq!(
            std::fs::metadata(&file).unwrap().modified().unwrap(),
            before
        );
    }
    // Stdin too.
    let mut child = Command::new(env!("CARGO_BIN_EXE_fmtron"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
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
    assert!(
        stderr.contains("exceeds the ceiling of 1024"),
        "stderr: {stderr}"
    );
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
            .arg(file.path())
            .status()
            .expect("run fmtron");
        assert!(status.success());
        let written = std::fs::read_to_string(file.path()).unwrap();
        assert_eq!(
            written, stdout,
            "in-place output differs from -d for {input:?}"
        );
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
        .args(["-d", "-w", "5"])
        .arg(file.path())
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
    let input = indoc! {"
        (
            a: 1,


            b: 2,
        )"};
    let keep = run_cli(input, &["-d"]);
    assert!(keep.status.success());
    assert_eq!(
        String::from_utf8_lossy(&keep.stdout),
        indoc! {"
            (
                a: 1,

                b: 2,
            )
        "}
    );
    let remove = run_cli(input, &["-d", "--blank-lines", "remove"]);
    assert!(remove.status.success());
    assert_eq!(
        String::from_utf8_lossy(&remove.stdout),
        indoc! {"
            (
                a: 1,
                b: 2,
            )
        "}
    );
}

/// A project tree: `<root>/fmt.ron` plus an input file two directories down.
fn project(config: &str, input: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("temp dir");
    let dir = root.path().join("assets/levels");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(root.path().join("fmt.ron"), config).unwrap();
    let file = dir.join("level.ron");
    std::fs::write(&file, input).unwrap();
    (root, file)
}

fn fmtron_on(file: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fmtron"))
        .args(args)
        .arg(file)
        .output()
        .expect("run fmtron")
}

// fmt.ron is found from the input's directory upward; flags override it;
// --no-config ignores it; --config picks another file.
#[test]
fn fmt_ron_is_discovered_and_flags_take_precedence() {
    let (root, file) = project(
        "(max_width: 20, tab_size: 2)",
        "(a: [1, 2, 3], b: (c: 1, d: 2))",
    );
    let stdout = |out: std::process::Output| {
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(
        stdout(fmtron_on(&file, &["-d"])),
        indoc! {"
            (
              a: [1, 2, 3],
              b: (c: 1, d: 2),
            )
        "}
    );
    assert_eq!(
        stdout(fmtron_on(&file, &["-d", "-t", "4"])),
        indoc! {"
            (
                a: [1, 2, 3],
                b: (c: 1, d: 2),
            )
        "}
    );
    assert_eq!(
        stdout(fmtron_on(&file, &["-d", "--no-config"])),
        "(a: [1, 2, 3], b: (c: 1, d: 2))\n"
    );
    let other = root.path().join("wide.ron");
    std::fs::write(&other, "(max_width: 100, tab_size: 8)").unwrap();
    assert_eq!(
        stdout(fmtron_on(
            &file,
            &["-d", "--config", other.to_str().unwrap()]
        )),
        "(a: [1, 2, 3], b: (c: 1, d: 2))\n"
    );
}

// A broken fmt.ron is a clean error naming the file, not silently ignored.
#[test]
fn invalid_fmt_ron_is_a_clean_error() {
    let (_root, file) = project("(max_widht: 80)", "(a: 1)");
    let out = fmtron_on(&file, &["-d"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("fmt.ron") && stderr.contains("max_widht"),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("panicked"), "stderr: {stderr}");
}

// --print-config shows the merged settings as a fmt.ron that parses back.
#[test]
fn print_config_emits_a_valid_fmt_ron() {
    let (_root, file) = project("(blank_lines: Remove)", "(a: 1)");
    let out = fmtron_on(&file, &["--print-config", "-w", "80"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let printed = String::from_utf8(out.stdout).unwrap();
    let parsed: fmtron::FileConfig = printed.parse().expect("valid fmt.ron");
    assert_eq!(parsed.max_width, Some(80));
    assert_eq!(parsed.blank_lines, Some(fmtron::BlankLines::Remove));
    assert!(printed.starts_with("// Effective fmtron configuration. Config file: "));
}

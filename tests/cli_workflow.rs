//! The CLI in everyday workflows: several files and directories per run,
//! `--check` for CI, stdin/stdout for editors, and safe in-place writes.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn fmtron(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fmtron"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run fmtron")
}

fn fmtron_stdin(dir: &Path, args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fmtron"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run fmtron");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

fn write(root: &Path, rel: &str, contents: &str) -> PathBuf {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, contents).unwrap();
    path
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap()
}

/// A git working tree with nested, ignored and hidden files, and a nested
/// fmt.ron.
fn tree() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    std::fs::create_dir(r.join(".git")).unwrap();
    write(r, ".gitignore", "generated/\n");
    write(r, "a.ron", "(a:1)");
    write(r, "levels/b.ron", "[1,2]");
    write(r, "levels/narrow/fmt.ron", "(max_width: 5, tab_size: 2)");
    write(r, "levels/narrow/c.ron", "[1,2]");
    write(r, "generated/ignored.ron", "[1,2]");
    write(r, ".hidden/h.ron", "[1,2]");
    write(r, "notes.txt", "not ron");
    root
}

#[test]
fn directories_are_walked_respecting_gitignore_and_nearest_config() {
    let root = tree();
    let r = root.path();
    let out = fmtron(r, &["."]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(read(r, "a.ron"), "(a: 1)\n");
    assert_eq!(read(r, "levels/b.ron"), "[1, 2]\n");
    // Formatted with its own directory's fmt.ron.
    assert_eq!(read(r, "levels/narrow/c.ron"), "[\n  1,\n  2,\n]\n");
    // Ignored, hidden and non-RON files are left alone.
    assert_eq!(read(r, "generated/ignored.ron"), "[1,2]");
    assert_eq!(read(r, ".hidden/h.ron"), "[1,2]");
    assert_eq!(read(r, "notes.txt"), "not ron");
    // A file named explicitly is formatted even though it is ignored.
    let out = fmtron(r, &["generated/ignored.ron"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(read(r, "generated/ignored.ron"), "[1, 2]\n");
}

#[test]
fn check_reports_diffs_and_writes_nothing() {
    let root = tree();
    let r = root.path();
    let out = fmtron(r, &["--check", "."]);
    assert_eq!(out.status.code(), Some(1));
    let stdout = text(&out.stdout);
    assert!(stdout.contains("Diff in a.ron:"), "{stdout}");
    assert!(
        stdout.contains("-(a:1)\n") && stdout.contains("+(a: 1)\n"),
        "{stdout}"
    );
    assert!(!stdout.contains("ignored.ron"), "{stdout}");
    assert_eq!(read(r, "a.ron"), "(a:1)", "--check must not write");

    assert!(fmtron(r, &["."]).status.success());
    let out = fmtron(r, &["--check", "."]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stdout));
    assert!(out.stdout.is_empty());
}

#[test]
fn stdin_is_formatted_to_stdout() {
    let root = tree();
    let r = root.path();
    for args in [&["-"][..], &[][..]] {
        let out = fmtron_stdin(r, args, "(a:[1,2])");
        assert!(out.status.success(), "{}", text(&out.stderr));
        assert_eq!(text(&out.stdout), "(a: [1, 2])\n");
    }
    // --stdin-filepath chooses the fmt.ron and names the input in errors.
    let out = fmtron_stdin(r, &["--stdin-filepath", "levels/narrow/x.ron"], "[1,2]");
    assert_eq!(text(&out.stdout), "[\n  1,\n  2,\n]\n");
    let out = fmtron_stdin(r, &["--stdin-filepath", "levels/x.ron"], "[1 2]");
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("levels/x.ron:1:4"),
        "{}",
        text(&out.stderr)
    );
    // --check on stdin: exit code only, stdin untouched.
    let out = fmtron_stdin(r, &["--check", "-"], "[1,2]");
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stdout).contains("Diff in <stdin>:"));
}

#[test]
fn unchanged_files_are_not_rewritten_and_backups_are_opt_in() {
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    let done = write(r, "done.ron", "(a: 1)\n");
    let before = std::fs::metadata(&done).unwrap().modified().unwrap();
    write(r, "todo.ron", "(a:1)");
    assert!(fmtron(r, &["done.ron", "todo.ron"]).status.success());
    assert_eq!(
        std::fs::metadata(&done).unwrap().modified().unwrap(),
        before
    );
    assert!(!r.join("todo.ron.bak").exists(), "no backup by default");

    write(r, "todo.ron", "(b:2)");
    assert!(fmtron(r, &["--backup", "todo.ron"]).status.success());
    assert_eq!(read(r, "todo.ron"), "(b: 2)\n");
    assert_eq!(read(r, "todo.ron.bak"), "(b:2)");
    assert!(
        !r.join("done.ron.bak").exists(),
        "unchanged files get no backup"
    );
}

#[cfg(unix)]
#[test]
fn in_place_writes_keep_symlinks_and_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    let real = write(r, "real/t.ron", "[1,2]");
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640)).unwrap();
    std::os::unix::fs::symlink("real/t.ron", r.join("link.ron")).unwrap();
    assert!(fmtron(r, &["link.ron"]).status.success());
    assert!(
        std::fs::symlink_metadata(r.join("link.ron"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(read(r, "real/t.ron"), "[1, 2]\n");
    let mode = std::fs::metadata(&real).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o640);

    // A read-only file is refused, as before, not replaced.
    let ro = write(r, "ro.ron", "[3,4]");
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o444)).unwrap();
    let out = fmtron(r, &["ro.ron"]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("read-only"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(read(r, "ro.ron"), "[3,4]");
}

#[test]
fn one_bad_file_does_not_stop_the_others() {
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    write(r, "a_good.ron", "[1,2]");
    write(r, "b_bad.ron", "[1 2]");
    write(r, "c_good.ron", "[3,4]");
    let out = fmtron(r, &[".", "missing.ron"]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(stderr.contains("b_bad.ron:1:4"), "{stderr}");
    assert!(stderr.contains("missing.ron: no such file"), "{stderr}");
    assert_eq!(read(r, "a_good.ron"), "[1, 2]\n");
    assert_eq!(read(r, "c_good.ron"), "[3, 4]\n");
}

#[test]
fn stdout_mode_takes_a_single_input() {
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    write(r, "a.ron", "[1,2]");
    write(r, "b.ron", "[3,4]");
    let out = fmtron(r, &["-d", "a.ron"]);
    assert_eq!(text(&out.stdout), "[1, 2]\n");
    assert_eq!(read(r, "a.ron"), "[1,2]", "-d must not write");
    let out = fmtron(r, &["-d", "a.ron", "b.ron"]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("--check"),
        "{}",
        text(&out.stderr)
    );
}

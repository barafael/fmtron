use fmtron::format_ron;
use fs_walk::WalkOptions;
use std::path::Path;

#[test]
fn empty_input() {
    use fmtron::format_ron;
    let result = format_ron("");
    assert!(result.is_err());
}

#[test]
fn invalid_input() {
    let result = format_ron("This is not RON!");
    assert!(result.is_err());
}

#[test]
fn formats_test_file() {
    let content = include_str!("../test_data/test.ron");
    let _ron = format_ron(content).expect("unable to format RON");
}

fn normalize(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn inofficial_improvised_ron_conformance_suite() {
    let unformatted_dir = "test_data/unformatted";
    let formatted_dir = "test_data/formatted";
    let walker = WalkOptions::new()
        .files()
        .extension("ron")
        .walk(unformatted_dir);
    for entry in walker.flatten() {
        let filename = entry.as_path().strip_prefix(unformatted_dir).unwrap();
        let formatted_path = Path::new(formatted_dir).join(filename);
        let input = std::fs::read_to_string(entry.as_path()).unwrap();
        let expected = std::fs::read_to_string(&formatted_path).unwrap();
        let ron = fmtron::format_ron(&input).unwrap();
        assert_eq!(
            normalize(&ron),
            normalize(&expected),
            "Failed case: {}",
            filename.display()
        );
    }
}

//! `fmt.ron` parsing, application and discovery (`fmtron::FileConfig`).

use fmtron::{BlankLines, Config, FileConfig};
use std::str::FromStr;

#[test]
fn every_field_is_optional() {
    assert_eq!(FileConfig::from_str("()").unwrap(), FileConfig::default());
    let partial = FileConfig::from_str("(max_width: 80)").unwrap();
    assert_eq!(
        partial,
        FileConfig {
            max_width: Some(80),
            ..FileConfig::default()
        }
    );
}

#[test]
fn full_file_with_comments_parses() {
    let file = FileConfig::from_str(
        "// fmt.ron\n(\n    max_width: 80, // columns\n    tab_size: 2,\n    \
         /* Keep | Remove */ blank_lines: Remove,\n    max_depth: 1000,\n    max_tab: 16,\n)\n",
    )
    .unwrap();
    assert_eq!(
        file,
        FileConfig {
            max_width: Some(80),
            tab_size: Some(2),
            blank_lines: Some(BlankLines::Remove),
            max_depth: Some(1000),
            max_tab: Some(16),
        }
    );
    // `Some(..)` is accepted as well as the bare value.
    assert_eq!(
        FileConfig::from_str("(tab_size: Some(2))")
            .unwrap()
            .tab_size,
        Some(2)
    );
}

#[test]
fn mistakes_are_errors() {
    let typo = FileConfig::from_str("(max_widht: 80)")
        .unwrap_err()
        .to_string();
    assert!(
        typo.contains("max_widht") && typo.contains("max_width"),
        "{typo}"
    );
    assert!(FileConfig::from_str("(max_width: \"wide\")").is_err());
    assert!(FileConfig::from_str("(blank_lines: Sometimes)").is_err());
    assert!(FileConfig::from_str("(max_width: -1)").is_err());
    assert!(FileConfig::from_str("max_width: 80").is_err());
}

#[test]
fn apply_overrides_only_what_is_set() {
    let mut config = Config::default();
    FileConfig::from_str("(tab_size: 2, blank_lines: Remove)")
        .unwrap()
        .apply(&mut config);
    let expected = Config {
        tab_size: 2,
        blank_lines: BlankLines::Remove,
        ..Config::default()
    };
    assert_eq!(
        config.to_file_config_string(),
        expected.to_file_config_string()
    );
}

#[test]
fn nearest_config_wins_and_fmt_ron_beats_hidden() {
    let root = tempfile::tempdir().unwrap();
    let deep = root.path().join("a/b/c");
    std::fs::create_dir_all(&deep).unwrap();
    // Nothing inside the temp dir: the search continues past it.
    assert_eq!(
        FileConfig::find(&deep),
        FileConfig::find(root.path().parent().unwrap())
    );

    std::fs::write(root.path().join("fmt.ron"), "()").unwrap();
    assert_eq!(FileConfig::find(&deep), Some(root.path().join("fmt.ron")));

    std::fs::write(root.path().join("a/.fmt.ron"), "()").unwrap();
    assert_eq!(
        FileConfig::find(&deep),
        Some(root.path().join("a/.fmt.ron"))
    );

    std::fs::write(root.path().join("a/fmt.ron"), "()").unwrap();
    assert_eq!(FileConfig::find(&deep), Some(root.path().join("a/fmt.ron")));
}

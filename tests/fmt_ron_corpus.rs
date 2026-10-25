//! `fmt.ron` corpus: every file under `test_data/fmt_ron/` is a valid
//! configuration file. The corpus varies syntax (attributes, comments, raw
//! identifiers, options, numeric forms), layout (one line, one field per line,
//! CRLF, blank lines), and settings, so it exercises both `FileConfig` parsing
//! and the formatter on the configuration format itself.
//!
//! `test_data/fmt_ron/manifest.ron` records, for each file, the settings it
//! must yield. The golden outputs in `test_data/fmt_ron_formatted/` are
//! generated at the default width, like `test_data/ron_corpus_formatted/`.

#[path = "support/test_data.rs"]
mod test_data;

use fmtron::{BlankLines, Config, FileConfig, StyleEdition, format_ron};
use ron::extensions::Extensions;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

const ROOT: &str = "test_data/fmt_ron";
const GOLDEN: &str = "test_data/fmt_ron_formatted";

#[derive(Debug, Deserialize)]
struct Manifest {
    total_files: usize,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    file: String,
    category: String,
    /// Kept for readers of the manifest; the test only needs the file and the
    /// settings it must yield.
    #[allow(dead_code)]
    note: String,
    yields: FileConfig,
}

fn manifest() -> Manifest {
    let path = Path::new(test_data::require(ROOT)).join("manifest.ron");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    ron::Options::default()
        .with_default_extension(Extensions::IMPLICIT_SOME)
        .from_str(&text)
        .unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()))
}

/// The `.ron` files in the corpus, sorted, excluding its manifest.
fn corpus_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(test_data::require(ROOT))
        .unwrap_or_else(|e| panic!("cannot read {ROOT}: {e}"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "ron")
                && path.file_name().is_some_and(|name| name != "manifest.ron")
        })
        .collect();
    files.sort();
    files
}

/// Parse a corpus file, naming it in the failure.
fn parse(file: &Path, text: &str) -> FileConfig {
    text.parse().unwrap_or_else(|e| {
        panic!(
            "{}: not a valid fmt.ron: {e}",
            file.strip_prefix(ROOT).unwrap_or(file).display()
        )
    })
}

/// Format a corpus file with `config`, naming it in the failure.
fn format(file: &Path, text: &str, config: &Config) -> String {
    format_ron(text, config).unwrap_or_else(|e| {
        panic!(
            "{}: formatting failed: {e}",
            file.strip_prefix(ROOT).unwrap_or(file).display()
        )
    })
}

/// The comments in `src`, normalized for movement: line comments lose trailing
/// whitespace, and the later lines of a block comment lose their indentation
/// (which follows the surrounding layout). Sorted, so the order may change.
fn comments(src: &str) -> Vec<String> {
    let b: Vec<char> = src.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let next = b.get(i + 1).copied();
        if b[i] == '/' && next == Some('/') {
            let mut k = i;
            while k < b.len() && b[k] != '\n' {
                k += 1;
            }
            found.push(b[i..k].iter().collect::<String>().trim_end().to_string());
            i = k;
        } else if b[i] == '/' && next == Some('*') {
            let (mut k, mut depth) = (i + 2, 1);
            while k < b.len() && depth > 0 {
                if b[k] == '/' && b.get(k + 1) == Some(&'*') {
                    depth += 1;
                    k += 2;
                } else if b[k] == '*' && b.get(k + 1) == Some(&'/') {
                    depth -= 1;
                    k += 2;
                } else {
                    k += 1;
                }
            }
            let text: String = b[i..k].iter().collect();
            found.push(
                text.split('\n')
                    .enumerate()
                    .map(|(n, line)| {
                        let line = line.strip_suffix('\r').unwrap_or(line);
                        if n == 0 { line } else { line.trim_start() }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            i = k;
        } else if b[i] == '"' || b[i] == '\'' {
            // These corpus files hold no strings or chars, but skip literals
            // anyway so a future case cannot confuse the scanner.
            let close = b[i];
            let mut k = i + 1;
            while k < b.len() && b[k] != close {
                k += if b[k] == '\\' { 2 } else { 1 };
            }
            i = (k + 1).min(b.len());
        } else {
            i += 1;
        }
    }
    found.sort();
    found
}

#[test]
fn every_case_parses_to_the_documented_settings() {
    let manifest = manifest();
    let files = corpus_files();
    let listed: BTreeSet<_> = manifest.cases.iter().map(|c| c.file.as_str()).collect();

    assert_eq!(
        manifest.total_files,
        manifest.cases.len(),
        "manifest total_files does not match its cases"
    );
    assert_eq!(
        files.len(),
        manifest.cases.len(),
        "the corpus has {} files but the manifest lists {}",
        files.len(),
        manifest.cases.len()
    );
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(
            listed.contains(name.as_ref()),
            "{name} is in the corpus but missing from manifest.ron"
        );
    }

    for case in &manifest.cases {
        let path = Path::new(ROOT).join(&case.file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: cannot read: {e}", case.file));
        let parsed = parse(&path, &text);

        assert_eq!(
            parsed, case.yields,
            "{} ({}) parsed to the wrong settings",
            case.file, case.category
        );

        parsed
            .validate()
            .unwrap_or_else(|e| panic!("{}: invalid settings: {e}", case.file));
        // A configured tab size must fit under its ceiling, the only
        // cross-field rule the CLI enforces on a `fmt.ron`.
        let mut config = Config::default();
        parsed.apply(&mut config);
        assert!(
            config.tab_size <= config.max_tab,
            "{}: tab size {} exceeds its ceiling of {}",
            case.file,
            config.tab_size,
            config.max_tab
        );

        // Every setting must be usable by the formatter. A shallow sample
        // keeps `max_depth: 1` cases valid.
        let sample = "(alpha: 1, beta: 2, gamma: 3)";
        let output = format_ron(sample, &config)
            .unwrap_or_else(|e| panic!("{}: settings are unusable: {e}", case.file));
        assert_eq!(
            format_ron(&output, &config).unwrap(),
            output,
            "{}: formatter is not idempotent with these settings",
            case.file
        );

        // fmt.ron is itself a `.ron` file: a project's fmt.ron is formatted
        // with its own settings. That must keep it parseable and unchanged on
        // a second run.
        let own = format_ron(&text, &config)
            .unwrap_or_else(|e| panic!("{}: cannot format itself: {e}", case.file));
        assert_eq!(
            parse(&path, &own),
            case.yields,
            "{}: formatting with its own settings changed them",
            case.file
        );
        assert_eq!(
            format_ron(&own, &config).unwrap(),
            own,
            "{}: formatting with its own settings is not idempotent",
            case.file
        );
    }
}

#[test]
fn formatting_preserves_settings_comments_and_line_endings() {
    let manifest = manifest();
    let configs = [
        Config::default(),
        Config::default()
            .with_max_width(32)
            .with_tab_size(2)
            .with_blank_lines(BlankLines::Remove),
        Config::default()
            .with_max_width(1)
            .with_tab_size(1)
            .with_max_tab(1)
            .with_blank_lines(BlankLines::Remove),
    ];

    for case in &manifest.cases {
        let path = Path::new(ROOT).join(&case.file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: cannot read: {e}", case.file));
        let before_comments = comments(&text);

        for (index, config) in configs.iter().enumerate() {
            let output = format(&path, &text, config);
            let reparsed = parse(&path, &output);
            assert_eq!(
                reparsed, case.yields,
                "{}: formatting with config {index} changed the settings",
                case.file
            );
            assert_eq!(
                format(&path, &output, config),
                output,
                "{}: formatting with config {index} is not idempotent",
                case.file
            );
            assert_eq!(
                fmtron::line_ending(&output),
                fmtron::line_ending(&text),
                "{}: formatting with config {index} changed the line endings",
                case.file
            );
            assert_eq!(
                comments(&output),
                before_comments,
                "{}: formatting with config {index} lost or duplicated a comment",
                case.file
            );
        }
    }
}

/// One of the six settings, with the value the generated cases use.
#[derive(Clone, Copy, Debug)]
enum Field {
    MaxWidth,
    TabSize,
    BlankLines,
    MaxDepth,
    MaxTab,
    StyleEdition,
}

const FIELDS: [Field; 6] = [
    Field::MaxWidth,
    Field::TabSize,
    Field::BlankLines,
    Field::MaxDepth,
    Field::MaxTab,
    Field::StyleEdition,
];

impl Field {
    fn name(self) -> &'static str {
        match self {
            Field::MaxWidth => "max_width",
            Field::TabSize => "tab_size",
            Field::BlankLines => "blank_lines",
            Field::MaxDepth => "max_depth",
            Field::MaxTab => "max_tab",
            Field::StyleEdition => "style_edition",
        }
    }

    fn value(self) -> &'static str {
        match self {
            Field::MaxWidth => "80",
            Field::TabSize => "2",
            Field::BlankLines => "Remove",
            Field::MaxDepth => "64",
            Field::MaxTab => "16",
            Field::StyleEdition => "2026",
        }
    }

    fn set(self, config: &mut FileConfig) {
        match self {
            Field::MaxWidth => config.max_width = Some(80),
            Field::TabSize => config.tab_size = Some(2),
            Field::BlankLines => config.blank_lines = Some(BlankLines::Remove),
            Field::MaxDepth => config.max_depth = NonZeroUsize::new(64),
            Field::MaxTab => config.max_tab = Some(16),
            Field::StyleEdition => config.style_edition = Some(StyleEdition::Edition2026),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Form {
    Bare,
    Some,
    None,
}

fn full_config() -> FileConfig {
    let mut config = FileConfig::default();
    for field in FIELDS {
        field.set(&mut config);
    }
    config
}

fn permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for index in 0..items.len() {
        let mut rest = items.to_vec();
        let head = rest.remove(index);
        for mut permutation in permutations(&rest) {
            permutation.insert(0, head.clone());
            out.push(permutation);
        }
    }
    out
}

/// Parse `text` as a `fmt.ron`, then format it and parse the result; both must
/// yield `expected` and formatting must be a fixed point.
fn check_generated(label: &str, text: &str, expected: &FileConfig) {
    let parsed = text
        .parse::<FileConfig>()
        .unwrap_or_else(|e| panic!("{label}: not a valid fmt.ron: {e}"));
    assert_eq!(&parsed, expected, "{label}: parsed to the wrong settings");
    let output = format_ron(text, &Config::default())
        .unwrap_or_else(|e| panic!("{label}: formatting failed: {e}"));
    let reparsed = output
        .parse::<FileConfig>()
        .unwrap_or_else(|e| panic!("{label}: formatted output is not a fmt.ron: {e}"));
    assert_eq!(
        &reparsed, expected,
        "{label}: formatting changed the settings"
    );
    assert_eq!(
        format_ron(&output, &Config::default()).unwrap(),
        output,
        "{label}: formatting is not idempotent"
    );
}

/// The static corpus covers interesting examples; this generated half fills in
/// the combinatorial space behind it: all 720 field orders in three layouts,
/// and all 3^6 combinations of bare/Some/None values.
#[test]
fn every_field_order_and_option_form_parses_to_the_same_settings() {
    let expected = full_config();
    let orders = permutations(&FIELDS);
    assert_eq!(orders.len(), 720);

    for order in &orders {
        let inline = order
            .iter()
            .map(|field| format!("{}: {}", field.name(), field.value()))
            .collect::<Vec<_>>()
            .join(", ");
        let one_per_line = order
            .iter()
            .map(|field| format!("    {}: {},", field.name(), field.value()))
            .collect::<Vec<_>>()
            .join("\n");
        for (kind, text) in [
            ("inline", format!("({inline})")),
            ("trailing comma", format!("({inline},)")),
            ("one per line", format!("(\n{one_per_line}\n)")),
        ] {
            check_generated(&format!("{kind}: {inline}"), &text, &expected);
        }
    }

    for code in 0..3usize.pow(FIELDS.len() as u32) {
        let mut expected = FileConfig::default();
        let mut fields = Vec::new();
        let mut form_code = code;
        for field in FIELDS {
            let form = match form_code % 3 {
                0 => Form::Bare,
                1 => Form::Some,
                _ => Form::None,
            };
            form_code /= 3;
            let token = match form {
                Form::Bare => field.value().to_string(),
                Form::Some => format!("Some({})", field.value()),
                Form::None => "None".to_string(),
            };
            if !matches!(form, Form::None) {
                field.set(&mut expected);
            }
            fields.push(format!("{}: {token}", field.name()));
        }
        let inline = fields.join(", ");
        // Half the cases also enable implicit_some explicitly; the attribute
        // must not change how explicit Some/None or bare values parse.
        let text = if code % 2 == 0 {
            format!("({inline})")
        } else {
            format!("#![enable(implicit_some)]\n({inline})")
        };
        check_generated(&format!("option form {code}: {inline}"), &text, &expected);
    }
}

#[test]
fn corpus_matches_golden_output() {
    let manifest = manifest();
    for case in &manifest.cases {
        let path = Path::new(ROOT).join(&case.file);
        let golden_path = Path::new(GOLDEN).join(&case.file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: cannot read: {e}", case.file));
        let golden = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|e| panic!("{}: cannot read golden: {e}", golden_path.display()));
        let output = format!(
            "{}{}",
            format(&path, &text, &Config::default()),
            fmtron::line_ending(&text)
        );
        assert_eq!(
            output,
            golden,
            "{}: formatted output differs from {}",
            case.file,
            golden_path.display()
        );
    }
}

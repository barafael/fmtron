//! Generated `fmt.ron` settings cases.
//!
//! The static corpus in `test_data/fmt_ron/` covers syntax and layout. This
//! module fills in the settings space behind it: boundary widths, every
//! supported tab/ceiling pair, depth limits, and both blank-line policies.
//! A case carries both the RON text and the `Config` that text must produce.
//!
//! `all()` is the broad parsing suite. `matrix_subset()` is the smaller,
//! deterministic selection used by the golden matrix, so increasing syntax
//! variety does not multiply every golden cell.

use fmtron::{BlankLines, Config, MAX_NESTING, MAX_TAB};

pub struct Case {
    /// A stable name for matrix rows and failure messages.
    pub name: String,
    #[allow(dead_code)]
    pub text: String,
    pub config: Config,
}

const WIDTHS: [usize; 15] = [
    1,
    8,
    16,
    24,
    32,
    40,
    56,
    72,
    80,
    100,
    120,
    160,
    240,
    4096,
    usize::MAX,
];

const TAB_CEILINGS: [(usize, usize); 8] = [
    (1, 1),
    (2, 4),
    (3, 8),
    (4, MAX_TAB),
    (8, 16),
    (16, 16),
    (32, MAX_TAB),
    (4, usize::MAX),
];

const DEPTHS: [usize; 9] = [1, 2, 3, 4, 8, 16, 64, MAX_NESTING, 1000];

/// Every generated settings combination.
pub fn all() -> Vec<Case> {
    let mut cases = Vec::with_capacity(WIDTHS.len() * TAB_CEILINGS.len() * DEPTHS.len() * 2);
    let mut index = 0;
    for &max_width in &WIDTHS {
        for &(tab_size, max_tab) in &TAB_CEILINGS {
            for &max_nesting in &DEPTHS {
                for blank_lines in [BlankLines::Keep, BlankLines::Remove] {
                    let config = Config::default()
                        .with_max_width(max_width)
                        .with_tab_size(tab_size)
                        .with_max_tab(max_tab)
                        .with_max_nesting(max_nesting)
                        .with_blank_lines(blank_lines);
                    cases.push(Case {
                        name: case_name(&config),
                        text: render(index, &config),
                        config,
                    });
                    index += 1;
                }
            }
        }
    }
    cases
}

/// The settings covered by the golden matrix: boundary axes, pairwise
/// width/depth grids, and a smaller Remove sample. This is a subset of
/// [`all`], selected without relying on random numbers.
#[allow(dead_code)]
pub fn matrix_subset() -> Vec<Case> {
    all()
        .into_iter()
        .filter(|case| {
            let config = case.config;
            let keep = config.blank_lines == BlankLines::Keep;
            let base_tab = config.tab_size == 4 && config.max_tab == MAX_TAB;
            let remove_base_tab = config.blank_lines == BlankLines::Remove
                && (config.max_nesting == 1
                    || config.max_nesting == 16
                    || config.max_nesting == MAX_NESTING);
            (keep && config.max_nesting == 16)
                || (keep && config.max_width == 80)
                || (keep && base_tab)
                || (remove_base_tab && (base_tab || config.max_width == 80))
        })
        .collect()
}

fn case_name(config: &Config) -> String {
    format!(
        "generated-w{}-t{}-m{}-d{}-{}.ron",
        config.max_width,
        config.tab_size,
        config.max_tab,
        config.max_nesting,
        match config.blank_lines {
            BlankLines::Keep => "keep",
            BlankLines::Remove => "remove",
        }
    )
}

/// Render a full config in one of several valid RON forms. Cycling through
/// the forms exercises bare values, `Some(..)`, attributes, struct names,
/// comments, trailing commas, radix prefixes, and CRLF without changing the
/// settings each case represents.
fn render(index: usize, config: &Config) -> String {
    let fields = field_values(config);
    let order = rotated(&fields, index % fields.len());
    let form = index % 12;
    let inline = render_inline(&order, false);
    let inline_trailing = render_inline(&order, true);
    let multiline = render_multiline(&order, "    ", false);
    let some_inline = order
        .iter()
        .map(|(name, value)| format!("{name}: Some({value})"))
        .collect::<Vec<_>>()
        .join(", ");
    let some_multiline = order
        .iter()
        .map(|(name, value)| format!("    {name}: Some({value}),\n"))
        .collect::<String>();
    let commented = order
        .iter()
        .map(|(name, value)| format!("    {name}: {value}, // {name}\n"))
        .collect::<String>();
    let block_commented = order
        .iter()
        .map(|(name, value)| format!("    /* {name} */ {name}: {value},\n"))
        .collect::<String>();

    match form {
        0 => format!("({inline})"),
        1 => format!("({inline_trailing})"),
        2 => format!("FileConfig(\n{multiline})"),
        3 => format!("r#FileConfig(\n{multiline})"),
        4 => format!("({some_inline})"),
        5 => format!("(\n{some_multiline})"),
        6 => format!("#![enable(implicit_some)]\n({inline})"),
        7 => format!("#![enable(implicit_some, unwrap_newtypes)]\n(\n{multiline})"),
        8 => format!("// generated settings\n(\n{commented})"),
        9 => format!("/* generated settings */\n(\n{block_commented})"),
        10 => format!(
            "/* {index} */\r\n(\r\n{}\r\n)",
            render_multiline(&order, "    ", true)
        ),
        _ => format!("({})", render_inline(&order, true)),
    }
}

fn field_values(config: &Config) -> [(&'static str, String); 6] {
    [
        ("max_width", config.max_width.to_string()),
        ("tab_size", config.tab_size.to_string()),
        ("blank_lines", format!("{:?}", config.blank_lines)),
        ("max_depth", config.max_nesting.to_string()),
        ("max_tab", config.max_tab.to_string()),
        ("style_edition", config.style_edition.to_string()),
    ]
}

fn rotated<T: Clone>(items: &[T], by: usize) -> Vec<T> {
    items
        .iter()
        .cycle()
        .skip(by)
        .take(items.len())
        .cloned()
        .collect()
}

fn render_inline(fields: &[(&str, String)], trailing: bool) -> String {
    let body = fields
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect::<Vec<_>>()
        .join(", ");
    if trailing { format!("{body},") } else { body }
}

fn render_multiline(fields: &[(&str, String)], indent: &str, crlf: bool) -> String {
    let newline = if crlf { "\r\n" } else { "\n" };
    let mut out = String::new();
    for (name, value) in fields {
        out.push_str(indent);
        out.push_str(name);
        out.push_str(": ");
        out.push_str(value);
        out.push(',');
        out.push_str(newline);
    }
    out
}

//! fmtron's layout rules, one test each, as the README's "Style" section
//! states them. Every case is also checked to be a fixed point.

use fmtron::{Config, format_ron};
use indoc::indoc;

fn fmt(input: &str, width: usize) -> String {
    let config = Config::default().with_max_width(width);
    let out = format_ron(input, &config).expect("valid RON");
    let again = format_ron(&out, &config).expect("output is valid RON");
    assert_eq!(again, out, "not a fixed point; first output:\n{out}");
    out
}

#[test]
fn a_container_that_fits_goes_on_one_line() {
    assert_eq!(
        fmt(r#"(a: 1, b: [1, 2], c: {"k": Some(3)})"#, 100),
        r#"(a: 1, b: [1, 2], c: {"k": Some(3)})"#
    );
    assert_eq!(
        fmt(
            r#"(name: "a name long enough", tags: ["one"], nested: (x: 1))"#,
            40
        ),
        indoc! {r#"
            (
                name: "a name long enough",
                tags: ["one"],
                nested: (x: 1),
            )"#}
    );
}

#[test]
fn a_struct_or_map_broken_after_its_opening_bracket_stays_broken() {
    let broken = indoc! {"
        (
            a: 1,
            b: 2,
        )"};
    assert_eq!(fmt("(\n    a: 1, b: 2)", 100), broken);
    assert_eq!(
        fmt("Point(\nx: 1, y: 2)", 100),
        indoc! {"
            Point(
                x: 1,
                y: 2,
            )"}
    );
    assert_eq!(
        fmt("{\n\"k\": 1}", 100),
        indoc! {r#"
            {
                "k": 1,
            }"#}
    );
    // A line break anywhere else does not count.
    assert_eq!(fmt("(a: 1,\n    b: 2)", 100), "(a: 1, b: 2)");
    // Nor does one after the bracket of a list of elements that are not
    // short.
    let list = indoc! {"
        [
            (a: 1),
            (a: 2),
        ]"};
    assert_eq!(fmt(list, 100), "[(a: 1), (a: 2)]");
    // Its parents break around it.
    assert_eq!(
        fmt("[(\n    a: 1), 2]", 100),
        indoc! {"
            [
                (
                    a: 1,
                ),
                2,
            ]"}
    );
}

#[test]
fn short_scalars_are_packed_even_when_written_one_per_line() {
    let input = format!(
        "[\n{}]",
        (1..=30).map(|i| format!("    {i},\n")).collect::<String>()
    );
    assert_eq!(
        fmt(&input, 40),
        indoc! {"
            [
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
                12, 13, 14, 15, 16, 17, 18, 19, 20,
                21, 22, 23, 24, 25, 26, 27, 28, 29,
                30,
            ]"}
    );
    // Tuples of scalars, and tuples (serde writes arrays as tuples).
    assert_eq!(
        fmt("[\n(1, 2),\n(3, 4),\n(5, 6),\n(7, 8),\n]", 20),
        indoc! {"
            [
                (1, 2), (3, 4),
                (5, 6), (7, 8),
            ]"}
    );
    assert_eq!(
        fmt("(\n1,\n2,\n3,\n4,\n5,\n6,\n7,\n8,\n)", 20),
        indoc! {"
            (
                1, 2, 3, 4, 5,
                6, 7, 8,
            )"}
    );
    // Numbers, bools and chars of any width.
    assert_eq!(
        fmt("[1, 2, 3, 123456789012345678901234567890]", 30),
        indoc! {"
            [
                1, 2, 3,
                123456789012345678901234567890,
            ]"}
    );
}

#[test]
fn strings_written_one_per_line_stay_one_per_line() {
    let strings = indoc! {r#"
        [
            "alpha",
            "beta",
            "gamma",
        ]"#};
    assert_eq!(fmt(strings, 20), strings);
    // Identifiers too, and a mix of strings and scalars.
    let idents = indoc! {"
        [
            A,
            B,
            C,
            D,
        ]"};
    assert_eq!(fmt(idents, 8), idents);
    let mixed = indoc! {r#"
        [
            1,
            "b",
            3,
        ]"#};
    assert_eq!(fmt(mixed, 8), mixed);
}

#[test]
fn short_elements_written_several_to_a_line_stay_packed() {
    assert_eq!(
        fmt(r#"["alpha", "beta", "gamma", "delta"]"#, 30),
        indoc! {r#"
            [
                "alpha", "beta", "gamma",
                "delta",
            ]"#}
    );
    // Every line break between them is kept; only a long row wraps.
    assert_eq!(
        fmt("[a, b, c,\nd,\ne, f, g, h, i, j, k, l, m]", 20),
        indoc! {"
            [
                a, b, c,
                d,
                e, f, g, h, i,
                j, k, l, m,
            ]"}
    );
    // Without a line break after the bracket, a list that fits joins.
    assert_eq!(fmt("[1, 2,\n3, 4]", 100), "[1, 2, 3, 4]");
}

#[test]
fn a_grid_stays_broken_and_keeps_its_rows() {
    let grid = indoc! {"
        [
            1, 0, 0,
            0, 1, 0,
            0, 0, 1,
        ]"};
    assert_eq!(fmt(grid, 100), grid);
    assert_eq!(
        fmt("(\n1, 0,\n0, 1)", 100),
        indoc! {"
            (
                1, 0,
                0, 1,
            )"}
    );
    // Its parents break around it, and a wrapper hugs it.
    assert_eq!(
        fmt("(m: Some([\n1, 0,\n0, 1]))", 100),
        indoc! {"
            (
                m: Some([
                    1, 0,
                    0, 1,
                ]),
            )"}
    );
    assert_eq!(
        fmt("Some([\n1, 0,\n0, 1])", 100),
        indoc! {"
            Some([
                1, 0,
                0, 1,
            ])"}
    );
}

#[test]
fn a_blank_line_between_elements_starts_a_new_row() {
    assert_eq!(
        fmt("[1, 2,\n\n3, 4]", 100),
        indoc! {"
            [
                1, 2,

                3, 4,
            ]"}
    );
    assert_eq!(
        fmt("Some([1, 2,\n\n3, 4])", 100),
        indoc! {"
            Some([
                1, 2,

                3, 4,
            ])"}
    );
}

#[test]
fn a_packed_element_too_wide_for_any_line_gets_lines_of_its_own() {
    // `(true, A, 23891),` does not fit at width 20 even on a line of its
    // own, so it breaks; the next element starts a new line.
    assert_eq!(
        fmt("Foo((44.9829, Bee), (true, A, 23891), 7)", 20),
        indoc! {"
            Foo(
                (44.9829, Bee),
                (
                    true, A,
                    23891,
                ),
                7,
            )"}
    );
}

#[test]
fn kept_line_breaks_are_dropped_if_no_line_would_hold_two_elements() {
    // Keeping the break before `17.29` would leave every element on a line
    // of its own, and formatting that again would pack them differently.
    assert_eq!(
        fmt("('x', (57660, 56624), 'x',\n17.29)", 20),
        indoc! {"
            (
                'x',
                (57660, 56624),
                'x', 17.29,
            )"}
    );
}

#[test]
fn long_or_nested_elements_are_never_packed() {
    assert_eq!(
        fmt(r#"["a string over sixteen", "b", "c"]"#, 20),
        indoc! {r#"
            [
                "a string over sixteen",
                "b",
                "c",
            ]"#}
    );
    assert_eq!(
        fmt("[[1], [2], [3], [4], [5], [6]]", 20),
        indoc! {"
            [
                [1],
                [2],
                [3],
                [4],
                [5],
                [6],
            ]"}
    );
    assert_eq!(
        fmt("[(a: 1), (a: 2), (a: 3)]", 20),
        indoc! {"
            [
                (a: 1),
                (a: 2),
                (a: 3),
            ]"}
    );
    // A comment anywhere puts every element on its own line.
    assert_eq!(
        fmt("[1, 2, // c\n3]", 100),
        indoc! {"
            [
                1,
                2, // c
                3,
            ]"}
    );
}

#[test]
fn a_wrapper_hugs_its_only_container() {
    assert_eq!(fmt("Some([1, 2, 3])", 100), "Some([1, 2, 3])");
    assert_eq!(
        fmt(r#"Some(Config(name: "primary", replicas: 3))"#, 30),
        indoc! {r#"
            Some(Config(
                name: "primary",
                replicas: 3,
            ))"#}
    );
    // Not an atom, and not across a comment.
    assert_eq!(
        fmt(r#"Some("a string too long for the line")"#, 30),
        indoc! {r#"
            Some(
                "a string too long for the line",
            )"#}
    );
    assert_eq!(
        fmt("Some( // c\n[1])", 100),
        indoc! {"
            Some(
                // c
                [1],
            )"}
    );
}

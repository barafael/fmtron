use fmtron::format_ron;

#[test]
fn format_simple_struct() {
    let input = include_str!("../test_data/simple_struct.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("foo: 1"));
    assert!(output.contains("bar: \"baz\""));
}

#[test]
fn format_list() {
    let input = include_str!("../test_data/list.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("1"));
    assert!(output.contains("2"));
    assert!(output.contains("3"));
    assert!(output.contains("4"));
}

#[test]
fn format_map() {
    let input = include_str!("../test_data/map.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("a: 1"));
    assert!(output.contains("b: 2"));
}

#[test]
fn format_tuple() {
    let input = include_str!("../test_data/tuple.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("foo"));
    assert!(output.contains("42"));
    assert!(output.contains("true"));
}

#[test]
fn format_unit() {
    let input = include_str!("../test_data/unit.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("meta_format_version: \"1.0\""));
    assert!(output.contains("asset: Load("));
    assert!(output.contains("loader: \"adventure_game::characters::data::CharacterDatasLoader\""));
    assert!(output.contains("settings: ()"));
}

#[test]
fn format_with_extension() {
    let input = include_str!("../test_data/with_extension.ron");
    let output = dbg!(format_ron(input).unwrap());
    assert!(output.contains("#![enable(bar, foo)]"));
    assert!(output.contains("hello: 123"));
}

#[test]
fn error_on_invalid() {
    let input = "not valid ron";
    let result = format_ron(input);
    assert!(result.is_err());
}

#[test]
fn format_nested_structures() {
    let input = include_str!("../test_data/nested_structures.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("foo"));
    assert!(output.contains("bar"));
    assert!(output.contains("baz"));
    assert!(output.contains("true"));
    assert!(output.contains("false"));
}

#[test]
fn format_empty_collections() {
    let input = include_str!("../test_data/empty_collections.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("empty_list"));
    assert!(output.contains("[]"));
    assert!(output.contains("empty_map"));
    assert!(output.contains("{}"));
    assert!(output.contains("empty_tuple"));
    assert!(output.contains("()"));
}

#[test]
fn format_with_comments() {
    let input = include_str!("../test_data/with_comments.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("foo: 1"));
    assert!(output.contains("bar: 2"));
}

#[test]
fn format_with_raw_string() {
    let input = include_str!("../test_data/raw_string.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("raw string with"));
}

#[test]
fn format_edge_case_numbers() {
    let input = include_str!("../test_data/edge_case_numbers.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("zero: 0"));
    assert!(output.contains("neg: -42"));
    assert!(output.contains("float: 3.1415"));
    assert!(output.contains("sci: 1e6"));
}

#[test]
fn format_deeply_nested() {
    let input = include_str!("../test_data/deeply_nested.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("a:"));
    assert!(output.contains("b:"));
    assert!(output.contains("c:"));
    assert!(output.contains("d:"));
    assert!(output.contains("3"));
}

#[test]
fn format_mixed_types() {
    let input = include_str!("../test_data/mixed_types.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("int: 42"));
    assert!(output.contains("float: 2.71"));
    assert!(output.contains("bool: false"));
    assert!(output.contains("str: \"hi\""));
    assert!(output.contains("list: ["));
    assert!(output.contains("tuple: (1, \"a\")"));
}

#[test]
fn format_unicode() {
    let input = include_str!("../test_data/unicode.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("😀"));
    assert!(output.contains("привет"));
    assert!(output.contains("你好"));
}

#[test]
fn format_large_numbers() {
    let input = include_str!("../test_data/large_numbers.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("12345678901234567890"));
    assert!(output.contains("-9876543210987654321"));
}

#[test]
fn format_multiple_extensions() {
    let input = include_str!("../test_data/multiple_extensions.ron");
    let output = format_ron(input).unwrap();
    assert!(output.contains("#![enable("));
    assert!(output.contains("foo"));
    assert!(output.contains("bar"));
    assert!(output.contains("baz"));
    assert!(output.contains("data: 1"));
}

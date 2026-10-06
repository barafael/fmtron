//! WebAssembly bindings to the [fmtron](https://github.com/barafael/fmtron)
//! RON formatter, for the [online demo](https://barafael.github.io/fmtron/).

use wasm_bindgen::prelude::*;

/// Formats RON text with fmtron's layout rules.
#[wasm_bindgen]
pub fn format_ron(
    input: &str,
    tab_size: u32,
    max_width: u32,
    keep_blank_lines: bool,
) -> Result<String, JsError> {
    let mut config = fmtron::Config::default()
        .with_tab_size(tab_size.max(1) as usize)
        .with_max_width(max_width.max(1) as usize);
    if !keep_blank_lines {
        config = config.with_blank_lines(fmtron::BlankLines::Remove);
    }
    fmtron::format_ron(input, &config).map_err(|e| JsError::new(&e.to_string()))
}

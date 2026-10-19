//! Parser for winget's fixed-width tables.
//!
//! OWNER: package-managers agent.

/// One table: header cells (localized, informative only) and rows of cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Finds and parses every table in a winget output (a header line followed by
/// a line of dashes, then rows until a blank or non-row line).
///
/// Must handle: progress/spinner garbage before the table (`\r` overwrites),
/// CRLF, localized headers, column positions computed in *display width*
/// (East-Asian wide chars count 2), cells containing spaces, empty trailing
/// cells, and several tables in one output.
pub fn parse_tables(output: &str) -> Vec<Table> {
    let _ = output;
    todo!("table::parse_tables")
}

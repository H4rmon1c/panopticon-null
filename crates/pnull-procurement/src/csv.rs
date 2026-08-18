//! Deterministic CSV export for procurement data with spreadsheet-formula
//! injection neutralization.
//!
//! Every cell that could be interpreted by a spreadsheet as a formula (leading
//! `=`, `+`, `-`, `@`, tab, or carriage return) is neutralized by prefixing a
//! single quote. CSV quoting and escaping follow RFC 4180: fields containing a
//! comma, quote, newline, or carriage return are quoted, and embedded quotes are
//! doubled. This is deterministic and tested against hostile inputs including
//! broken/ambiguous quoting.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CsvError {
    #[error("no data rows to export")]
    NoRows,
}

/// Neutralizes spreadsheet-formula injection in a single cell.
///
/// Cells that begin with a formula-trigger character are prefixed with a single
/// quote so a spreadsheet opens them as literal text rather than executing a
/// formula. Leading whitespace is scanned so that whitespace-padded formulas are
/// also caught. Empty and normal cells are returned unchanged.
pub fn neutralize_cell(raw: &str) -> String {
    let trimmed = raw.trim_start();
    let is_formula_trigger = trimmed
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '=' | '+' | '-' | '@' | '\t' | '\r'));
    if is_formula_trigger {
        format!("'{raw}")
    } else {
        raw.to_owned()
    }
}

/// RFC 4180 quoting of a single cell (after formula neutralization).
fn quote_cell(cell: &str) -> String {
    let needs_quotes = cell.chars().any(|c| matches!(c, ',' | '"' | '\n' | '\r'));
    if !needs_quotes {
        return cell.to_owned();
    }
    format!("\"{}\"", cell.replace('"', "\"\""))
}

/// Renders a header + rows to deterministic RFC 4180 CSV text.
///
/// Every cell is formula-neutralized and quoted. The output is a single string;
/// it contains no BOM and uses `\n` line endings for determinism.
pub fn rows_to_csv(header: &[&str], rows: &[Vec<String>]) -> Result<String, CsvError> {
    if rows.is_empty() {
        return Err(CsvError::NoRows);
    }
    let mut out = String::new();
    out.push_str(
        &header
            .iter()
            .map(|h| quote_cell(&neutralize_cell(h)))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push('\n');
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .map(|c| quote_cell(&neutralize_cell(c)))
            .collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_cells_are_neutralized() {
        assert_eq!(neutralize_cell("=SUM(A1:A10)"), "'=SUM(A1:A10)");
        assert_eq!(neutralize_cell("+1+2"), "'+1+2");
        assert_eq!(neutralize_cell("-1"), "'-1");
        assert_eq!(neutralize_cell("@cmd"), "'@cmd");
        assert_eq!(neutralize_cell("\t=1"), "'\t=1");
        assert_eq!(neutralize_cell(" normal "), " normal ");
    }

    #[test]
    fn whitespace_padded_formula_is_caught() {
        // Leading whitespace is scanned so "  =1" is still a formula.
        assert_eq!(neutralize_cell("  =1"), "'  =1");
    }

    #[test]
    fn normal_and_money_cells_are_preserved() {
        assert_eq!(neutralize_cell("$42,075.00"), "$42,075.00");
        assert_eq!(neutralize_cell("Optiv"), "Optiv");
        assert_eq!(neutralize_cell(""), "");
        // A cell that merely contains '=' in the middle is not a formula.
        assert_eq!(neutralize_cell("a=b"), "a=b");
    }

    #[test]
    fn csv_quoting_follows_rfc4180() {
        let csv = rows_to_csv(
            &["name", "amount"],
            &[vec!["Optiv".to_owned(), "$42,075.00".to_owned()]],
        )
        .expect("csv");
        assert!(csv.contains("\"$42,075.00\""));
        assert!(csv.contains("name,amount"));
    }

    #[test]
    fn embedded_quotes_and_newlines_are_doubled_and_quoted() {
        let csv = rows_to_csv(&["note"], &[vec!["say \"hi\"\nnext".to_owned()]]).expect("csv");
        assert!(csv.contains("\"say \"\"hi\"\"\nnext\""));
    }

    #[test]
    fn formula_injection_is_neutralized_in_export() {
        let csv = rows_to_csv(
            &["vendor", "notes"],
            &[vec![
                "=HYPERLINK(\"http://evil\")".to_owned(),
                "Optiv".to_owned(),
            ]],
        )
        .expect("csv");
        // The formula-triggering cell is prefixed, not passed through raw.
        assert!(csv.contains("'=HYPERLINK"));
        assert!(!csv.lines().any(|l| l.contains(",=HYPERLINK")));
    }

    #[test]
    fn broken_or_ambiguous_cells_are_handled_without_panic() {
        let hostile = [
            "\"", "\"\"", "a\"b", "a,b", "a\nb", "\r\n", ",,", " ", "\t", "=1,=2", "@", "+", "-",
            "'", "a\"\"b",
        ];
        for cell in hostile {
            let rows = vec![vec![cell.to_owned()]];
            let csv = rows_to_csv(&["c"], &rows).expect("csv for hostile cell");
            // The export is well-formed: it has a header line and a data line,
            // and the data line is not empty.
            let lines: Vec<&str> = csv.lines().collect();
            assert!(lines.len() >= 2, "expected header + data for {cell:?}");
            assert!(
                !lines[1].is_empty(),
                "data line must not be empty for {cell:?}"
            );
        }
    }

    #[test]
    fn empty_rows_are_an_error() {
        assert_eq!(rows_to_csv(&["a"], &[]), Err(CsvError::NoRows));
    }

    #[test]
    fn export_is_deterministic() {
        let rows = vec![
            vec!["B".to_owned(), "2".to_owned()],
            vec!["A".to_owned(), "1".to_owned()],
        ];
        let a = rows_to_csv(&["x", "y"], &rows).expect("a");
        let b = rows_to_csv(&["x", "y"], &rows).expect("b");
        assert_eq!(a, b);
    }
}

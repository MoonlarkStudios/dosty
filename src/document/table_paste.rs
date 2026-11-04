//! Atomic rectangular values paste. Existing cells outside the target keep their
//! source bytes; growth inserts only missing cells, separator tokens, and rows.

use super::TableEdit;
use super::controls;
use super::input::literal_cell;
use crate::document::collaboration::{
    MAX_SOURCE_BYTES, MAX_TRANSACTION_EDITS, SourceEdit, validate_edits,
};
use crate::document::projection::Projection;
use crate::document::selection::Selection;

const TOO_LARGE: &str = "This paste exceeds the document or table growth limit.";
const INVALID: &str = "This table's source structure cannot be pasted into safely.";

/// Values are decoded rectangular text, with CR/CRLF normalized to logical cell
/// line breaks. All dimensions and growth are checked before source mutation.
pub fn paste_rectangle(
    projection: &Projection,
    source: &str,
    index: usize,
    start: (usize, usize),
    values: &[Vec<String>],
) -> Result<TableEdit, &'static str> {
    let table = projection.rich.tables.get(index).ok_or(INVALID)?;
    let width = values.first().map_or(0, Vec::len);
    if values.is_empty() || width == 0 || values.iter().any(|row| row.len() != width) {
        return Err("Clipboard cells must form a nonempty rectangle.");
    }
    let old_width = table.alignment.len();
    let old_height = table.rows.len();
    if start.0 >= old_height
        || start.1 >= old_width
        || source.get(table.source_range.clone()).is_none()
    {
        return Err(INVALID);
    }
    let bottom = start.0.checked_add(values.len()).ok_or(TOO_LARGE)?;
    let right = start.1.checked_add(width).ok_or(TOO_LARGE)?;
    let new_width = old_width.max(right);
    let new_height = old_height.max(bottom);
    let cells = values.len().checked_mul(width).ok_or(TOO_LARGE)?;
    if cells > MAX_TRANSACTION_EDITS {
        return Err(TOO_LARGE);
    }
    let rows = controls::source_rows(source, table)?;
    if rows.iter().any(|row| row.cells.len() > old_width) {
        return Err("This table has extra source cells. Resolve them in Source before pasting.");
    }
    let needed_columns = |physical: usize, present: usize| {
        if new_width > old_width {
            return new_width;
        }
        let Some(row) = logical_row(physical).filter(|&row| start.0 <= row && row < bottom) else {
            return present;
        };
        // Existing implicit empty cells need no source text for an empty paste.
        // Materialize only through the last new nonempty value in this row.
        (present.max(start.1)..right)
            .rev()
            .find(|&column| !values[row - start.0][column - start.1].is_empty())
            .map_or(present, |column| column + 1)
    };
    // A narrow clipboard can require a wide expansion across many old rows.
    // Bound every newly materialized slot, not just clipboard dimensions.
    let mut work = cells;
    for (physical, row) in rows.iter().enumerate() {
        let needed = needed_columns(physical, row.cells.len());
        work = work
            .checked_add(needed.saturating_sub(row.cells.len()))
            .ok_or(TOO_LARGE)?;
        if work > MAX_TRANSACTION_EDITS {
            return Err(TOO_LARGE);
        }
    }
    work = work
        .checked_add(
            new_height
                .saturating_sub(old_height)
                .checked_mul(new_width)
                .ok_or(TOO_LARGE)?,
        )
        .ok_or(TOO_LARGE)?;
    if work > MAX_TRANSACTION_EDITS {
        return Err(TOO_LARGE);
    }

    let ending = rows
        .iter()
        .find(|row| !row.ending.is_empty())
        .map(|row| &source[row.ending.clone()])
        .unwrap_or("\n");
    let prefix = super::row_prefix(source, table);
    let new_rows = new_height - old_height;
    let minimum_row = prefix
        .len()
        .checked_add(new_width.checked_mul(3).ok_or(TOO_LARGE)?)
        .and_then(|size| size.checked_add(ending.len() + 1))
        .ok_or(TOO_LARGE)?;
    if new_rows.checked_mul(minimum_row).ok_or(TOO_LARGE)? > MAX_SOURCE_BYTES {
        return Err(TOO_LARGE);
    }

    let mut normalized = Vec::with_capacity(values.len());
    let mut encoded = Vec::with_capacity(values.len());
    let mut encoded_bytes = 0usize;
    for row in values {
        let mut plain_row = Vec::with_capacity(width);
        let mut encoded_row = Vec::with_capacity(width);
        for value in row {
            if value.len() > MAX_SOURCE_BYTES {
                return Err(TOO_LARGE);
            }
            let value = value.replace("\r\n", "\n").replace('\r', "\n");
            encoded_bytes = encoded_bytes
                .checked_add(encoded_length(&value)?)
                .ok_or(TOO_LARGE)?;
            if encoded_bytes > MAX_SOURCE_BYTES {
                return Err(TOO_LARGE);
            }
            encoded_row.push(literal_cell(&value, value.len()).0);
            plain_row.push(value);
        }
        normalized.push(plain_row);
        encoded.push(encoded_row);
    }
    let value_at = |row: usize, column: usize| -> Option<&str> {
        (start.0 <= row && row < bottom && start.1 <= column && column < right)
            .then(|| encoded[row - start.0][column - start.1].as_str())
    };
    let mut edits = Vec::new();
    for (physical, raw) in rows.iter().enumerate() {
        let logical = logical_row(physical);
        if let Some(row) = logical.filter(|&row| start.0 <= row && row < bottom) {
            for column in start.1..right.min(raw.cells.len()) {
                let original = table
                    .source_rows
                    .get(row)
                    .and_then(|row| row.get(column))
                    .ok_or(INVALID)?;
                let display = table
                    .rows
                    .get(row)
                    .and_then(|row| row.get(column))
                    .ok_or(INVALID)?;
                let plain = &normalized[row - start.0][column - start.1];
                if projection.text.get(display.clone()) == Some(plain.as_str()) {
                    continue;
                }
                let original_text = source.get(original.clone()).ok_or(INVALID)?;
                let encoded = value_at(row, column).ok_or(INVALID)?;
                let replacement = preserve_style(
                    original_text,
                    &projection.text[display.clone()],
                    plain,
                    encoded,
                );
                edits.push(SourceEdit {
                    range: original.clone(),
                    text: replacement,
                });
            }
            // Erasing an outer value in an optional-edge-pipe row must not
            // reinterpret the next inner delimiter as the table's outer edge.
            if start.1 == 0
                && normalized[row - start.0][0].is_empty()
                && !source[raw.content.clone()].trim_start().starts_with('|')
            {
                edits.push(SourceEdit {
                    range: raw.content.start..raw.content.start,
                    text: "|".into(),
                });
            }
            if right == raw.cells.len()
                && right <= old_width
                && normalized[row - start.0][width - 1].is_empty()
                && !source[raw.content.clone()].trim_end().ends_with('|')
            {
                edits.push(SourceEdit {
                    range: raw.content.end..raw.content.end,
                    text: "|".into(),
                });
            }
        }
        let needed = needed_columns(physical, raw.cells.len());
        if needed > raw.cells.len() {
            let point = raw.cells.last().ok_or(INVALID)?.end;
            let outer_pipe = point < raw.content.end;
            let mut text = String::from(" ");
            for column in raw.cells.len()..needed {
                text.push_str("| ");
                text.push_str(if physical == 1 {
                    "---"
                } else {
                    logical.and_then(|row| value_at(row, column)).unwrap_or("")
                });
                text.push(' ');
                if text.len() > MAX_SOURCE_BYTES {
                    return Err(TOO_LARGE);
                }
            }
            if !outer_pipe {
                text.push('|');
            }
            edits.push(SourceEdit {
                range: point..point,
                text,
            });
        }
    }
    if new_rows > 0 {
        let terminated = rows.last().is_some_and(|row| !row.ending.is_empty());
        let mut text = String::new();
        if !terminated {
            text.push_str(ending);
        }
        for row in old_height..new_height {
            text.push_str(&prefix);
            text.push('|');
            for column in 0..new_width {
                text.push(' ');
                text.push_str(value_at(row, column).unwrap_or(""));
                text.push_str(" |");
            }
            if terminated || row + 1 < new_height {
                text.push_str(ending);
            }
            if text.len() > MAX_SOURCE_BYTES {
                return Err(TOO_LARGE);
            }
        }
        edits.push(SourceEdit {
            range: table.source_range.end..table.source_range.end,
            text,
        });
    }
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    let mut merged: Vec<SourceEdit> = Vec::with_capacity(edits.len());
    for edit in edits {
        if let Some(last) = merged.last_mut()
            && last.range.end == edit.range.start
            && (last.range.is_empty() || edit.range.is_empty())
        {
            last.range.end = edit.range.end;
            last.text.push_str(&edit.text);
        } else {
            merged.push(edit);
        }
    }
    merged.retain(|edit| source.get(edit.range.clone()) != Some(edit.text.as_str()));
    validate_edits(source, &merged).map_err(|_| TOO_LARGE)?;

    // One batch oracle catches literal/Markdown ambiguities and structural
    // surprises without reparsing the whole table separately for every cell.
    let mut candidate = source.to_owned();
    for edit in merged.iter().rev() {
        candidate.replace_range(edit.range.clone(), &edit.text);
    }
    let parsed = Projection::new(&candidate);
    let result = parsed
        .rich
        .tables
        .get(index)
        .filter(|result| {
            result.source_range.start == table.source_range.start
                && result.rows.len() == new_height
                && result.alignment.len() == new_width
        })
        .ok_or(INVALID)?;
    for (row, values) in normalized.iter().enumerate() {
        for (column, value) in values.iter().enumerate() {
            let range = result
                .rows
                .get(start.0 + row)
                .and_then(|row| row.get(start.1 + column))
                .ok_or(INVALID)?;
            if parsed.text.get(range.clone()) != Some(value.as_str()) {
                return Err("A pasted value cannot be represented safely in this Markdown table.");
            }
        }
    }
    Ok(TableEdit {
        edits: merged,
        selection: Selection::caret(result.source_rows[bottom - 1][right - 1].end),
    })
}

fn logical_row(physical: usize) -> Option<usize> {
    match physical {
        0 => Some(0),
        1 => None,
        _ => Some(physical - 1),
    }
}

fn encoded_length(value: &str) -> Result<usize, &'static str> {
    let start = value.len() - value.trim_start().len();
    let end = value.trim_end().len();
    value
        .char_indices()
        .try_fold(0usize, |size, (index, character)| {
            let bytes = if character == '\n' {
                4
            } else if character.is_whitespace() && (index < start || index >= end) {
                4 + (character as u32).ilog10() as usize
            } else {
                character.len_utf8() + usize::from(character.is_ascii_punctuation())
            };
            size.checked_add(bytes).ok_or(TOO_LARGE)
        })
}

fn preserve_style(original: &str, visible: &str, value: &str, encoded: &str) -> String {
    // Simple outer emphasis can survive a word replacement without risking
    // punctuation flanking or introducing a per-cell Markdown parse.
    if !value.contains('\n')
        && value.starts_with(char::is_alphanumeric)
        && value.ends_with(char::is_alphanumeric)
    {
        for marker in ["***", "___", "**", "__", "~~", "*", "_"] {
            if original
                .strip_prefix(marker)
                .and_then(|body| body.strip_suffix(marker))
                == Some(visible)
            {
                return format!("{marker}{encoded}{marker}");
            }
        }
    }
    encoded.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|row| row.iter().map(|value| (*value).to_owned()).collect())
            .collect()
    }
    fn apply(
        source: &str,
        start: (usize, usize),
        values: &[Vec<String>],
    ) -> (String, Projection, TableEdit) {
        let projection = Projection::new(source);
        let edit = paste_rectangle(&projection, source, 0, start, values).unwrap();
        validate_edits(source, &edit.edits).unwrap();
        let mut result = source.to_owned();
        for change in edit.edits.iter().rev() {
            result.replace_range(change.range.clone(), &change.text);
        }
        let fresh = Projection::new(&result);
        (result, fresh, edit)
    }
    fn matrix(projection: &Projection) -> Vec<Vec<&str>> {
        projection.rich.tables[0]
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| &projection.text[cell.clone()])
                    .collect()
            })
            .collect()
    }

    #[test]
    fn rectangle_grows_columns_and_rows_without_wrapping_or_rewriting_neighbors() {
        let source = "Before\n\n| A | B |\n| :--- | ---: |\n| **left** | *right* |\n| keep | unchanged |\n\nAfter";
        let original_range = Projection::new(source).rich.tables[0].source_range.clone();
        let data = values(&[
            &["one", "two", "three"],
            &["four", "five", "six"],
            &["seven", "eight", "nine"],
        ]);
        let (result, projection, edits) = apply(source, (1, 1), &data);
        assert_eq!(
            matrix(&projection),
            vec![
                vec!["A", "B", "", ""],
                vec!["left", "one", "two", "three"],
                vec!["keep", "four", "five", "six"],
                vec!["", "seven", "eight", "nine"]
            ]
        );
        assert!(result.starts_with("Before\n\n| A | B "));
        assert!(result.contains("| :--- | ---: "));
        assert!(result.contains("| **left** | *one*"));
        assert!(result.contains("| keep |"));
        assert!(result.ends_with("\n\nAfter"));
        assert!(edits.edits.iter().all(|edit| edit.range != original_range));
    }

    #[test]
    fn literal_clipboard_values_keep_unicode_tabs_breaks_quotes_and_markdown_literal() {
        let source = "| A | B | C | D |\n|---|---|---|---|\n| ![old](old.png) | untouched | still | ![keep](keep.png) |\n";
        let data = values(&[&[
            " **literal** | &amp; ",
            "first\tsecond\n\n日本語",
            "\"quoted\" \\ <br> [link](url)",
        ]]);
        let (result, projection, _) = apply(source, (1, 0), &data);
        assert_eq!(&matrix(&projection)[1][..3], data[0]);
        assert_eq!(projection.rich.images.len(), 1);
        assert_eq!(projection.rich.images[0].url, "keep.png");
        assert!(result.contains("![keep](keep.png)"));
        assert!(result.contains("<br><br>"));
        assert!(result.contains("&#32;"));
        assert_eq!(projection.rich.tables[0].rows.len(), 2);
        let (_, projection, _) = apply(source, (1, 0), &values(&[&["first\r\nsecond\rthird"]]));
        assert_eq!(matrix(&projection)[1][0], "first\nsecond\nthird");
    }

    #[test]
    fn implicit_cells_growth_and_containers_preserve_physical_endings() {
        for ending in ["\n", "\r\n", "\r"] {
            for prefix in ["> ", "  > "] {
                let source = [
                    format!("{prefix}| A | B | C |"),
                    format!("{prefix}| :--- | ---: | --- |"),
                    format!("{prefix}| short |"),
                ]
                .join(ending);
                let source = if prefix.starts_with(' ') {
                    format!("- Outer{ending}{source}")
                } else {
                    source
                };
                let data = values(&[&["late", "new"], &["next", "row"]]);
                let (result, projection, _) = apply(&source, (1, 2), &data);
                assert_eq!(matrix(&projection)[1], vec!["short", "", "late", "new"]);
                assert_eq!(matrix(&projection)[2], vec!["", "", "next", "row"]);
                assert!(result.contains(&format!("{ending}{prefix}|")));
                assert!(!result.ends_with(['\r', '\n']));
                let stripped = result.replace(ending, "");
                assert!(!stripped.contains(['\r', '\n']));
            }
        }
    }

    #[test]
    fn in_bounds_paste_changes_only_target_values_and_repairs_optional_empty_edges() {
        let source = "Before\n\nA | B | C\n:--- | ---: | ---\nleft | **middle** | right\nkeep | untouched | still\n\nAfter";
        let (result, projection, edit) = apply(source, (1, 1), &values(&[&["updated"]]));
        assert_eq!(result, source.replace("**middle**", "**updated**"));
        assert_eq!(edit.edits.len(), 1);
        assert_eq!(matrix(&projection)[2], vec!["keep", "untouched", "still"]);
        let (_, projection, _) = apply(source, (0, 0), &values(&[&["", "", ""]]));
        assert_eq!(matrix(&projection)[0], vec!["", "", ""]);
        let source = "A | B | C\n---|---|---\nleft | right\n";
        let (result, projection, edit) = apply(source, (1, 2), &values(&[&[""]]));
        assert_eq!(result, source);
        assert!(
            edit.edits.is_empty(),
            "pasting an unchanged implicit empty cell must stay a no-op"
        );
        assert_eq!(matrix(&projection)[1], vec!["left", "right", ""]);
    }

    #[test]
    fn malformed_excess_and_oversized_growth_are_rejected_without_a_plan() {
        let source = "| A | B |\n|---|---|\n| one | two | excess |\n";
        let projection = Projection::new(source);
        assert!(paste_rectangle(&projection, source, 0, (1, 0), &values(&[&["new"]])).is_err());
        let source = "| A | B |\n|---|---|\n| one | two |\n";
        let projection = Projection::new(source);
        assert!(
            paste_rectangle(&projection, source, 0, (1, 0), &[vec!["x".into()], vec![]]).is_err()
        );
        assert!(
            paste_rectangle(
                &projection,
                source,
                0,
                (1, 0),
                &[vec![" ".repeat(MAX_SOURCE_BYTES / 5 + 1)]]
            )
            .is_err()
        );
        let wide = vec!["x".into(); MAX_TRANSACTION_EDITS / 2];
        assert!(paste_rectangle(&projection, source, 0, (1, 1), &[wide]).is_err());
    }
}

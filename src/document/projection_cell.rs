//! Disclose user-edited edge whitespace without revealing table delimiters or
//! pre-existing alignment padding. All ranges here are UTF-8 source offsets.

use std::ops::Range;

use crate::document::rich_document::RichDocument;

fn horizontal_space(character: char) -> bool {
    character.is_whitespace() && !matches!(character, '\r' | '\n')
}

pub(super) fn visible_cell(
    source: &str,
    rich: &RichDocument,
    caret: usize,
    tracked: &Range<usize>,
) -> Option<(usize, usize, usize, Range<usize>)> {
    if tracked.start > caret
        || caret > tracked.end
        || !source.is_char_boundary(caret)
        || source.get(tracked.clone()).is_none()
    {
        return None;
    }
    let table_index = rich
        .tables
        .partition_point(|table| table.source_range.end < caret);
    let table = rich.tables.get(table_index)?;
    if caret < table.source_range.start {
        return None;
    }
    let line = source[..caret].rfind(['\r', '\n']).map_or(0, |n| n + 1);
    let row_index = table
        .rows
        .partition_point(|row| row.first().is_some_and(|cell| cell.start < line));
    let row = table.rows.get(row_index)?;
    for (column, cell) in row.iter().enumerate() {
        // Trimming only horizontal whitespace cannot cross a pipe or row break.
        // Clipping also keeps a structural insertion at a tracked boundary from
        // accidentally disclosing another cell or a container prefix.
        let start = source[..cell.start]
            .trim_end_matches(horizontal_space)
            .len();
        let end = cell.end
            + source[cell.end..]
                .chars()
                .take_while(|&character| horizontal_space(character))
                .map(char::len_utf8)
                .sum::<usize>();
        if caret < start || caret > end {
            continue;
        }
        let visible = if cell.is_empty() {
            // The parser locates an empty cell after all of its padding. An
            // edited value may instead be empty before the original right pad.
            tracked.start.max(start)..tracked.end.min(end)
        } else {
            tracked.start.max(start).min(cell.start)..tracked.end.min(end).max(cell.end)
        };
        return (visible != *cell).then_some((table_index, row_index, column, visible));
    }
    None
}

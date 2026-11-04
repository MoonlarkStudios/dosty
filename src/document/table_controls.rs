//! Source splices for explicit table commands. Existing cells are never reserialized.

use std::ops::Range;

use pulldown_cmark::Alignment;

use super::cell_at;
use crate::document::collaboration::SourceEdit;
use crate::document::projection::Projection;
use crate::document::rich_document::Table;
use crate::document::selection::Selection;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    RowBefore,
    RowAfter,
    DeleteRow,
    ColumnBefore,
    ColumnAfter,
    DeleteColumn,
    Align(Alignment),
}

pub struct TableEdit {
    pub edits: Vec<SourceEdit>,
    pub selection: Selection,
}

pub(super) struct Row {
    pub(super) line: Range<usize>,
    pub(super) content: Range<usize>,
    pub(super) cells: Vec<Range<usize>>,
    pub(super) ending: Range<usize>,
}

/// Selection must belong to one cell. Multiple source splices form one human
/// transaction, retaining CRDT identities in untouched cells and nearby prose.
pub fn command(
    projection: &Projection,
    source: &str,
    selection: Selection,
    command: Command,
) -> Result<TableEdit, &'static str> {
    let (table, row, column) = cell_at(projection, selection)
        .ok_or("Place the caret or select text inside one table cell.")?;
    command_at(source, table, row, column, selection, command)
}

pub fn command_at(
    source: &str,
    table: &Table,
    row: usize,
    column: usize,
    selection: Selection,
    command: Command,
) -> Result<TableEdit, &'static str> {
    if row >= table.source_rows.len() || column >= table.alignment.len() {
        return Err("The selected cell is no longer available.");
    }
    let columns = table.alignment.len();
    let rows = source_rows(source, table)?;
    let physical_row = if row == 0 { 0 } else { row + 1 };
    let prefix = super::row_prefix(source, table);
    let ending = rows
        .iter()
        .find(|row| !row.ending.is_empty())
        .map_or("\n", |row| &source[row.ending.clone()]);
    let blank = format!("|{}", "  |".repeat(columns));
    let splice = |range, text| SourceEdit { range, text };
    let mut edits = match command {
        Command::RowBefore if row == 0 => {
            // A new header takes the old header's place. Move the separator
            // above the old header, which becomes the first body row.
            let separator = &source[rows[1].content.clone()];
            vec![
                splice(
                    rows[0].content.start..rows[0].content.start,
                    format!("{blank}{ending}{prefix}{separator}{ending}{prefix}"),
                ),
                splice(rows[1].line.clone(), String::new()),
            ]
        }
        Command::RowBefore | Command::RowAfter => {
            let index = if command == Command::RowBefore {
                physical_row
            } else if row == 0 {
                2
            } else {
                physical_row + 1
            };
            if let Some(next) = rows.get(index) {
                vec![splice(
                    next.line.start..next.line.start,
                    format!("{prefix}{blank}{ending}"),
                )]
            } else {
                let last = rows.last().ok_or("Cannot locate the last row.")?;
                let end = table.source_range.end;
                let text = if last.ending.is_empty() {
                    format!("{ending}{prefix}{blank}")
                } else {
                    format!("{prefix}{blank}{ending}")
                };
                vec![splice(end..end, text)]
            }
        }
        Command::DeleteRow if rows.len() == 2 => {
            vec![splice(table.source_range.clone(), String::new())]
        }
        Command::DeleteRow if row == 0 => {
            let next = &rows[2];
            if next.cells.len() > columns {
                return Err(
                    "The next row has extra source cells. Edit those in Source before promoting it to the header.",
                );
            }
            let mut header = source[next.content.clone()].to_owned();
            if next.cells.len() < columns {
                // Materialize implicit empty cells before promoting a short row.
                let at = next
                    .cells
                    .last()
                    .ok_or("Cannot locate the next row's cells.")?
                    .end
                    - next.content.start;
                header.insert_str(
                    at,
                    &format!(" {}", "|  ".repeat(columns - next.cells.len())),
                );
            }
            vec![
                splice(rows[0].content.clone(), header),
                splice(next.line.clone(), String::new()),
            ]
        }
        Command::DeleteRow => vec![splice(rows[physical_row].line.clone(), String::new())],
        Command::DeleteColumn if columns == 1 => {
            vec![splice(table.source_range.clone(), String::new())]
        }
        Command::ColumnBefore | Command::ColumnAfter => {
            let index = column + usize::from(command == Command::ColumnAfter);
            rows.iter()
                .enumerate()
                .map(|(row, raw)| {
                    let value = if row == 1 { " --- " } else { "  " };
                    if let Some(cell) = raw.cells.get(index) {
                        let edge = if index == 0 && cell.start == raw.content.start {
                            "|"
                        } else {
                            ""
                        };
                        splice(cell.start..cell.start, format!("{edge}{value}|"))
                    } else {
                        let end = raw.cells.last().map_or(raw.content.end, |cell| cell.end);
                        let padding = "|  ".repeat(index.saturating_sub(raw.cells.len()));
                        let edge = if end == raw.content.end { "|" } else { "" };
                        // An unterminated row may end in a literal backslash.
                        // Separate it from the new pipe so it cannot escape it.
                        splice(end..end, format!(" {padding}|{value}{edge}"))
                    }
                })
                .collect()
        }
        Command::DeleteColumn => rows
            .iter()
            .enumerate()
            .filter_map(|(row, raw)| {
                let cell = raw.cells.get(column)?;
                let edge = if row < 2
                    && columns == 2
                    && raw.cells[0].start == raw.content.start
                    && raw
                        .cells
                        .last()
                        .is_some_and(|cell| cell.end == raw.content.end)
                {
                    "|"
                } else {
                    ""
                };
                Some(if raw.cells.len() == 1 {
                    // A physically short row still needs a real empty cell.
                    splice(raw.content.clone(), "|  |".into())
                } else if column == 0 {
                    splice(cell.start..raw.cells[1].start, edge.into())
                } else {
                    splice(raw.cells[column - 1].end..cell.end, edge.into())
                })
            })
            .collect(),
        Command::Align(alignment) => {
            let cell = &rows[1].cells[column];
            let raw = &source[cell.clone()];
            let token = raw.trim_matches([' ', '\t']);
            let start = cell.start + raw.len() - raw.trim_start_matches([' ', '\t']).len();
            let dashes = token.trim_matches(':');
            if dashes.is_empty() || !dashes.bytes().all(|byte| byte == b'-') {
                return Err("Cannot locate the column alignment marker.");
            }
            let left = if matches!(alignment, Alignment::Left | Alignment::Center) {
                ":"
            } else {
                ""
            };
            let right = if matches!(alignment, Alignment::Right | Alignment::Center) {
                ":"
            } else {
                ""
            };
            vec![splice(
                start..start + token.len(),
                format!("{left}{dashes}{right}"),
            )]
        }
    };
    edits.retain(|edit| source.get(edit.range.clone()) != Some(edit.text.as_str()));
    edits.sort_by_key(|edit| edit.range.start);
    let selection = Selection {
        anchor: remap(selection.anchor, &edits),
        head: remap(selection.head, &edits),
    };
    Ok(TableEdit { edits, selection })
}

pub(super) fn source_rows(source: &str, table: &Table) -> Result<Vec<Row>, &'static str> {
    let mut rows = Vec::new();
    let mut start = table.source_range.start;
    while start < table.source_range.end {
        let end = source[start..table.source_range.end]
            .find(['\r', '\n'])
            .map_or(table.source_range.end, |n| start + n);
        let mut next = end;
        if next < table.source_range.end {
            next += 1;
            if source.as_bytes()[end] == b'\r' && source.as_bytes().get(next) == Some(&b'\n') {
                next += 1;
            }
        }
        let content = if rows.is_empty() {
            start
        } else if rows.len() == 1 {
            // The separator has no cell events. Only container prefixes and
            // whitespace can precede its first pipe, colon or dash.
            start
                + source[start..end]
                    .find(['|', ':', '-'])
                    .ok_or("Cannot locate the table separator.")?
        } else {
            let first = table
                .source_rows
                .get(rows.len() - 1)
                .and_then(|row| row.first())
                .ok_or("Cannot locate the table row.")?;
            if first.start > end {
                return Err("Cannot locate the table cell.");
            }
            let before = &source[start..first.start];
            // Cell metadata trims Unicode whitespace. Only ASCII indentation
            // and quote markers belong to the container; retain other padding
            // as part of the first cell, including before its opening pipe.
            before
                .find('|')
                .or_else(|| before.find(|c| !matches!(c, ' ' | '\t' | '>')))
                .map_or(first.start, |offset| start + offset)
        };
        rows.push(Row {
            line: start..next,
            content: content..end,
            cells: cells(source, content..end),
            ending: end..next,
        });
        start = next;
    }
    let columns = table.alignment.len();
    if columns == 0
        || rows.len() != table.source_rows.len() + 1
        || rows[0].cells.len() != columns
        || rows[1].cells.len() != columns
    {
        return Err("This table's source structure cannot be edited safely.");
    }
    Ok(rows)
}

/// GFM recognizes an unescaped pipe even inside a code span. Keep the exact
/// whitespace, optional edge pipes and any excess cells omitted by the parser.
pub(super) fn cells(source: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let raw = &source[range.clone()];
    let mut pipes = Vec::new();
    let mut backslashes = 0;
    for (index, byte) in raw.bytes().enumerate() {
        if byte == b'|' && backslashes % 2 == 0 {
            pipes.push(range.start + index);
        }
        backslashes = if byte == b'\\' { backslashes + 1 } else { 0 };
    }
    let mut start = range.start;
    let mut end = range.end;
    if pipes
        .first()
        .is_some_and(|&pipe| source[start..pipe].trim().is_empty())
    {
        start = pipes.remove(0) + 1;
    }
    if pipes
        .last()
        .is_some_and(|&pipe| source[pipe + 1..end].trim().is_empty())
    {
        end = pipes.pop().unwrap_or(end);
    }
    let mut result = Vec::new();
    for pipe in pipes {
        result.push(start..pipe);
        start = pipe + 1;
    }
    result.push(start..end);
    result
}

pub(super) fn remap(point: usize, edits: &[SourceEdit]) -> usize {
    let mut shift = 0isize;
    for edit in edits {
        if point < edit.range.start {
            break;
        }
        if point < edit.range.end {
            return (edit.range.start as isize + shift) as usize;
        }
        shift += edit.text.len() as isize - edit.range.len() as isize;
    }
    (point as isize + shift) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(source: &str, row: usize, column: usize, action: Command) -> String {
        let projection = Projection::new(source);
        let cell = &projection.rich.tables[0].source_rows[row][column];
        let edit = command(&projection, source, Selection::caret(cell.start), action).unwrap();
        let table = &projection.rich.tables[0].source_range;
        let mut result = source.to_owned();
        for edit in edit.edits.iter().rev() {
            assert!(table.start <= edit.range.start && edit.range.end <= table.end);
            result.replace_range(edit.range.clone(), &edit.text);
        }
        assert!(result.starts_with(&source[..table.start]));
        assert!(result.ends_with(&source[table.end..]));
        assert!(result.is_char_boundary(edit.selection.anchor));
        assert!(result.is_char_boundary(edit.selection.head));
        result
    }

    fn matrix(source: &str) -> Vec<Vec<String>> {
        let projection = Projection::new(source);
        assert_eq!(projection.rich.tables.len(), 1, "{source}");
        projection.rich.tables[0]
            .source_rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| source[cell.clone()].to_owned())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn commands_preserve_cells_containers_line_endings_and_surrounding_source() {
        let fixture = include_str!("../../tests/fixtures/table-controls.md");
        for ending in ["\n", "\r\n", "\r"] {
            for prefix in ["", "> ", "  "] {
                let source = fixture
                    .lines()
                    .map(|line| format!("{prefix}{line}{ending}"))
                    .collect::<String>();
                let original = matrix(&source);
                for row in 0..original.len() {
                    for column in 0..original[0].len() {
                        for action in [
                            Command::RowBefore,
                            Command::RowAfter,
                            Command::DeleteRow,
                            Command::ColumnBefore,
                            Command::ColumnAfter,
                            Command::DeleteColumn,
                            Command::Align(Alignment::None),
                            Command::Align(Alignment::Left),
                            Command::Align(Alignment::Center),
                            Command::Align(Alignment::Right),
                        ] {
                            let result = apply(&source, row, column, action);
                            let mut expected = original.clone();
                            match action {
                                Command::RowBefore => expected.insert(row, vec![String::new(); 3]),
                                Command::RowAfter => {
                                    expected.insert(row + 1, vec![String::new(); 3])
                                }
                                Command::DeleteRow => {
                                    expected.remove(row);
                                }
                                Command::ColumnBefore | Command::ColumnAfter => {
                                    let index =
                                        column + usize::from(action == Command::ColumnAfter);
                                    for row in &mut expected {
                                        row.insert(index, String::new());
                                    }
                                }
                                Command::DeleteColumn => {
                                    for row in &mut expected {
                                        row.remove(column);
                                    }
                                }
                                Command::Align(alignment) => {
                                    let projection = Projection::new(&result);
                                    assert_eq!(
                                        projection.rich.tables[0].alignment[column],
                                        alignment
                                    );
                                }
                            }
                            assert!(
                                !Projection::new(&result).rich.tables.is_empty(),
                                "{action:?}, {row}/{column}, {prefix:?}, {ending:?}\n{result}"
                            );
                            assert_eq!(
                                matrix(&result),
                                expected,
                                "{action:?}, {row}/{column}, {prefix:?}, {ending:?}\n{result}"
                            );
                            if ending == "\r\n" {
                                assert!(!result.replace("\r\n", "").contains('\n'));
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn exact_column_edits_retain_whitespace_escaped_pipes_and_extra_source_cells() {
        let source = "Before\n\nA  | B\n:--|---:\nx\\|y | z | hidden\n\nAfter";
        assert_eq!(
            apply(source, 1, 0, Command::ColumnBefore),
            "Before\n\n|  |A  | B\n| --- |:--|---:\n|  |x\\|y | z | hidden\n\nAfter"
        );
        assert_eq!(
            apply(source, 1, 0, Command::DeleteColumn),
            "Before\n\n| B\n|---:\n z | hidden\n\nAfter"
        );
        assert_eq!(
            Projection::new(&apply(source, 1, 0, Command::DeleteColumn))
                .rich
                .tables
                .len(),
            1
        );
    }

    #[test]
    fn short_rows_and_header_only_tables_remain_editable() {
        for source in [
            "| A | B | C |\n|---|---|---|\n| x |\n",
            "A | B | C\n---|---|---\nx",
            "| A | B |\n|---|---|",
        ] {
            let projection = Projection::new(source);
            for column in 0..projection.rich.tables[0].alignment.len() {
                for action in [
                    Command::ColumnBefore,
                    Command::ColumnAfter,
                    Command::DeleteColumn,
                ] {
                    let result = apply(source, 0, column, action);
                    let projection = Projection::new(&result);
                    assert_eq!(
                        projection.rich.tables.len(),
                        1,
                        "{action:?}, {column}: {result}"
                    );
                }
            }
        }
        for action in [Command::DeleteRow, Command::DeleteColumn] {
            assert_eq!(
                apply("Before\n\n| A |\n|---|\n\nAfter", 0, 0, action),
                "Before\n\n\nAfter"
            );
        }
    }

    #[test]
    fn nested_lists_missing_edge_pipes_and_final_line_endings_keep_their_structure() {
        for ending in ["\n", "\r\n", "\r"] {
            for (first, continuation) in [("- ", "  "), ("> 1. ", ">    "), ("> > ", "> > ")] {
                for edges in [false, true] {
                    let header = if edges { "| A | B |" } else { "A | B" };
                    let separator = if edges { "|:--|--:|" } else { ":--|--:" };
                    let data = if edges {
                        "| café | 中文 |"
                    } else {
                        "café | 中文"
                    };
                    let source = format!(
                        "{first}{header}{ending}{continuation}{separator}{ending}{continuation}{data}"
                    );
                    let original = matrix(&source);
                    for action in [
                        Command::RowBefore,
                        Command::RowAfter,
                        Command::DeleteRow,
                        Command::ColumnBefore,
                        Command::ColumnAfter,
                        Command::DeleteColumn,
                    ] {
                        for row in 0..2 {
                            let result = apply(&source, row, 0, action);
                            assert_eq!(
                                Projection::new(&result).rich.tables.len(),
                                1,
                                "{action:?}: {result}"
                            );
                            let mut expected = original.clone();
                            match action {
                                Command::RowBefore => expected.insert(row, vec![String::new(); 2]),
                                Command::RowAfter => {
                                    expected.insert(row + 1, vec![String::new(); 2])
                                }
                                Command::DeleteRow => {
                                    expected.remove(row);
                                }
                                Command::ColumnBefore => {
                                    for row in &mut expected {
                                        row.insert(0, String::new());
                                    }
                                }
                                Command::ColumnAfter => {
                                    for row in &mut expected {
                                        row.insert(1, String::new());
                                    }
                                }
                                Command::DeleteColumn => {
                                    for row in &mut expected {
                                        row.remove(0);
                                    }
                                }
                                Command::Align(_) => unreachable!(),
                            }
                            assert_eq!(matrix(&result), expected, "{action:?}: {result}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn column_commands_handle_unicode_padding_and_literal_backslashes_at_row_end() {
        for (header, body) in [
            ("A | B", "x | y\\"),
            ("A | B", "\u{a0}x | y"),
            ("| A | B |", "| \u{a0} x | y |"),
            ("A|B", "`a\\|b` | **café**"),
        ] {
            let source = format!("{header}\n---|---\n{body}");
            let original = matrix(&source);
            for column in 0..2 {
                for action in [
                    Command::ColumnBefore,
                    Command::ColumnAfter,
                    Command::DeleteColumn,
                ] {
                    let result = apply(&source, 0, column, action);
                    let mut expected = original.clone();
                    for row in &mut expected {
                        if action == Command::DeleteColumn {
                            row.remove(column);
                        } else {
                            row.insert(
                                column + usize::from(action == Command::ColumnAfter),
                                String::new(),
                            );
                        }
                    }
                    assert_eq!(
                        Projection::new(&result).rich.tables.len(),
                        1,
                        "{action:?}: {result}"
                    );
                    assert_eq!(matrix(&result), expected, "{action:?}: {result}");
                }
            }
        }
    }
}

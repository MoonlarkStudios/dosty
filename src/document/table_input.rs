//! Document-style text entry inside a Markdown grid. Splices touch cell values
//! and any required edge pipes, never the separators between selected cells.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::borrow::Cow;
use std::ops::Range;

use super::{TableEdit, cell_at, controls, escape_pipes};
use crate::document::collaboration::SourceEdit;
use crate::document::projection::Projection;
use crate::document::selection::Selection;

/// Only a bare inline break is interpreted as cell content; attributes and
/// arbitrary HTML remain literal. Code spans and escaped tags are not HTML events.
pub fn break_tag(html: &str) -> bool {
    html.strip_prefix('<')
        .and_then(|tag| tag.strip_suffix('>'))
        .is_some_and(|tag| {
            tag.trim()
                .trim_end_matches('/')
                .trim()
                .eq_ignore_ascii_case("br")
        })
}

/// Iced's Markdown viewer ignores inline HTML. A newline character reference
/// decodes inside the cell's text without creating a physical Markdown row.
pub fn read_breaks(source: &str) -> Cow<'_, str> {
    if !source.contains('<') {
        return Cow::Borrowed(source);
    }
    let mut table = false;
    let mut ranges = Vec::new();
    for (event, range) in
        Parser::new_ext(source, crate::document::block_parser::MARKDOWN_OPTIONS).into_offset_iter()
    {
        match event {
            Event::Start(Tag::Table(_)) => table = true,
            Event::End(TagEnd::Table) => table = false,
            Event::InlineHtml(html) if table && break_tag(&html) => ranges.push(range),
            _ => {}
        }
    }
    if ranges.is_empty() {
        return Cow::Borrowed(source);
    }
    let mut result = source.to_owned();
    for range in ranges.into_iter().rev() {
        result.replace_range(range, "&#10;");
    }
    Cow::Owned(result)
}

pub fn materialize_cell(
    source: &str,
    row: usize,
    column: usize,
    table: &crate::document::rich_document::Table,
) -> Option<TableEdit> {
    let first = table.source_rows.get(row)?.first()?;
    let cell = table.source_rows.get(row)?.get(column)?;
    if !cell.is_empty() {
        return None;
    }
    let line = source[..first.start]
        .rfind(['\r', '\n'])
        .map_or(0, |n| n + 1);
    let end = source[first.start..]
        .find(['\r', '\n'])
        .map_or(source.len(), |n| first.start + n);
    let before = &source[line..first.start];
    let start = if row == 0 {
        table.source_range.start
    } else {
        before
            .find('|')
            .or_else(|| before.find(|character| !matches!(character, ' ' | '\t' | '>')))
            .map_or(first.start, |n| line + n)
    };
    let cells = controls::cells(source, start..end);
    if column < cells.len() {
        return None;
    }
    let mut text = if cells.last()?.end < end {
        String::new()
    } else {
        " |".into()
    };
    for _ in cells.len()..=column {
        text.push_str("  |");
    }
    let point = end;
    Some(TableEdit {
        selection: Selection::caret(point + text.len() - 2),
        edits: vec![SourceEdit {
            range: point..point,
            text,
        }],
    })
}

fn encode(text: &str) -> String {
    let text = escape_pipes(&text.replace("\r\n", "\n").replace('\r', "\n"));
    let start = text.len() - text.trim_start().len();
    let end = text.trim_end().len();
    let mut output = String::new();
    for (index, character) in text.char_indices() {
        if character == '\n' {
            output.push_str("<br>");
        } else if character.is_whitespace() && (index < start || index >= end) {
            output.push_str(&format!("&#{};", character as u32));
        } else {
            output.push(character);
        }
    }
    output
}

pub(super) fn literal_cell(text: &str, caret: usize) -> (String, usize) {
    let start = text.len() - text.trim_start().len();
    let end = text.trim_end().len();
    let mut output = String::new();
    let mut point = 0;
    for (index, character) in text.char_indices() {
        if index == caret {
            point = output.len();
        }
        if character == '\n' {
            output.push_str("<br>");
        } else if character.is_whitespace() && (index < start || index >= end) {
            output.push_str(&format!("&#{};", character as u32));
        } else {
            if character.is_ascii_punctuation() {
                output.push('\\');
            }
            output.push(character);
        }
    }
    if caret == text.len() {
        point = output.len();
    }
    (output, point)
}

/// A rectangular selection is not the contiguous Markdown range between its
/// corners. Only explicitly selected cell values enter this transaction.
pub fn clear_rectangle(
    projection: &Projection,
    source: &str,
    index: usize,
    anchor: (usize, usize),
    head: (usize, usize),
) -> Option<TableEdit> {
    let table = projection.rich.tables.get(index)?;
    let mut edits = Vec::new();
    for row in anchor.0.min(head.0)..=anchor.0.max(head.0) {
        for column in anchor.1.min(head.1)..=anchor.1.max(head.1) {
            let cell = table.source_rows.get(row)?.get(column)?;
            if !cell.is_empty() {
                edits.push(SourceEdit {
                    range: cell.clone(),
                    text: String::new(),
                });
            }
        }
    }
    let mut plan = finish(
        projection,
        source,
        table,
        edits,
        table.source_rows[head.0][head.1].start,
    )?;
    plan.selection = super::remap_selection(
        Selection::caret(table.source_rows[head.0][head.1].start),
        &plan.edits,
    );
    Some(plan)
}

/// Commit one local draft. Unchanged styled text retains its exact source;
/// newly typed punctuation remains literal when Markdown cannot express the
/// intended visible text with the existing inline wrappers.
pub fn commit_cell(
    projection: &Projection,
    source: &str,
    index: usize,
    cell: (usize, usize),
    expected: &str,
) -> Option<TableEdit> {
    let table = projection.rich.tables.get(index)?;
    let range = &table.source_rows[cell.0][cell.1];
    let display = &table.rows[cell.0][cell.1];
    let diff = crate::document::projection::difference(&projection.text[display.clone()], expected);
    let selection = Selection {
        anchor: projection.to_source(
            display.start + diff.old.start,
            crate::document::projection::Affinity::After,
            Some(range.start),
        ),
        head: projection.to_source(
            display.start + diff.old.end,
            crate::document::projection::Affinity::Before,
            Some(range.end),
        ),
    };
    let inserted = &expected[diff.new];
    if let Some(mut plan) = materialize_cell(source, cell.0, cell.1, table) {
        let (value, _) = literal_cell(expected, expected.len());
        let edit = plan.edits.first_mut()?;
        let point = edit.text.len() - 2;
        edit.text.insert_str(point, &value);
        plan.selection = Selection::caret(edit.range.start + point + value.len());
        return Some(plan);
    }
    let plan = insert_text(projection, source, selection, inserted)?;
    let mut candidate = source[range.clone()].to_owned();
    for edit in plan.edits.iter().rev() {
        if range.start <= edit.range.start && edit.range.end <= range.end {
            candidate.replace_range(
                edit.range.start - range.start..edit.range.end - range.start,
                &edit.text,
            );
        }
    }
    let wrapper = format!("| {candidate} |\n|---|\n");
    let parsed = Projection::new(&wrapper);
    let actual = parsed
        .rich
        .tables
        .first()
        .and_then(|t| t.rows.first())
        .and_then(|r| r.first())
        .map(|r| &parsed.text[r.clone()]);
    if actual == Some(expected) {
        return Some(plan);
    }
    let (mut value, _) = literal_cell(expected, expected.len());
    let original = &source[range.clone()];
    if !expected.is_empty()
        && let Some(marker) = ["***", "___", "**", "__", "~~", "*", "_"]
            .into_iter()
            .find(|m| {
                original.len() > m.len() * 2 && original.starts_with(m) && original.ends_with(m)
            })
    {
        value = format!("{marker}{value}{marker}");
    }
    let caret = range.start + value.len();
    finish(
        projection,
        source,
        table,
        vec![SourceEdit {
            range: range.clone(),
            text: value,
        }],
        caret,
    )
}

pub fn insert_text(
    projection: &Projection,
    source: &str,
    selection: Selection,
    text: &str,
) -> Option<TableEdit> {
    let (table, _, _) = cell_at(projection, selection)?;
    if !selection.is_caret() && projection.inline_code_at(selection).is_none() {
        return replace_table_range(projection, source, table, selection.range(), text);
    }
    let selected = selection.range();
    let (range, text, caret) = if text.contains(['\r', '\n'])
        && let Some((span, body)) = projection.inline_code_at(selection)
    {
        let fence = &source[span.start..body.start];
        let left = &source[body.start..selected.start];
        let right = &source[selected.end..body.end];
        let mut value = if left.is_empty() {
            String::new()
        } else {
            format!("{fence}{left}{fence}")
        };
        value.push_str(&encode(text));
        let mut caret = span.start + value.len();
        if !right.is_empty() {
            caret += fence.len();
            value.push_str(&format!("{fence}{right}{fence}"));
        }
        (span, value, caret)
    } else {
        let value = if projection.inline_code_at(selection).is_some() {
            escape_pipes(text)
        } else {
            encode(text)
        };
        let caret = selected.start + value.len();
        (selected, value, caret)
    };
    let edits = vec![SourceEdit { range, text }];
    finish(projection, source, table, edits, caret)
}

/// A text selection spanning cells replaces only their selected contents. Full
/// document/table-block deletions still use the ordinary document edit path.
pub fn replace_across_cells(
    projection: &Projection,
    source: &str,
    selection: Selection,
    text: &str,
) -> Option<TableEdit> {
    if selection.is_caret() || cell_at(projection, selection).is_some() {
        return None;
    }
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = normalized.as_str();
    let selected = selection.range();
    let table = projection.rich.tables.iter().find(|table| {
        table.source_range.start <= selected.start && selected.end <= table.source_range.end
    })?;
    replace_table_range(projection, source, table, selected, text)
}

fn replace_table_range(
    projection: &Projection,
    source: &str,
    table: &crate::document::rich_document::Table,
    selected: Range<usize>,
    text: &str,
) -> Option<TableEdit> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = normalized.as_str();
    let mut edits = Vec::new();
    let mut caret = None;
    for (cell, display_cell) in table
        .source_rows
        .iter()
        .flatten()
        .zip(table.rows.iter().flatten())
    {
        if cell.end < selected.start || selected.end < cell.start {
            continue;
        }
        let range = selected.start.max(cell.start)..selected.end.min(cell.end);
        if range.is_empty() && !cell.is_empty() {
            continue;
        }
        let value = if caret.is_none() {
            encode(text)
        } else {
            String::new()
        };
        let mut edit = SourceEdit {
            range: range.clone(),
            text: String::new(),
        };
        let mut point = range.start;
        if range != *cell && !range.is_empty() {
            // Only the two partial boundary cells need inline delimiter repair.
            // Whole selected cells are cleared directly, avoiding one parse per
            // cell in a large selection.
            let before = Selection {
                anchor: range.start,
                head: range.end,
            };
            let shown = projection.display_selection(before).range();
            let before = Selection {
                anchor: projection.to_source(
                    shown.start,
                    crate::document::projection::Affinity::After,
                    None,
                ),
                head: projection.to_source(
                    shown.end,
                    crate::document::projection::Affinity::Before,
                    None,
                ),
            };
            let mut after = projection.text.clone();
            after.replace_range(shown.clone(), "");
            let repaired = projection.edit(source, &after, before, Selection::caret(shown.start));
            if cell.start <= repaired.range.start && repaired.range.end <= cell.end {
                point = repaired.selection.head;
                edit = SourceEdit {
                    range: repaired.range,
                    text: repaired.text,
                };
            }
        }
        edit.text.insert_str(point - edit.range.start, &value);
        let selected_display = projection
            .display_selection(Selection {
                anchor: range.start,
                head: range.end,
            })
            .range();
        let original = &source[cell.clone()];
        if caret.is_none()
            && !text.is_empty()
            && selected_display == *display_cell
            && let Some(marker) =
                ["***", "___", "**", "__", "~~", "*", "_"]
                    .into_iter()
                    .find(|marker| {
                        original.len() > marker.len() * 2
                            && original.starts_with(marker)
                            && original.ends_with(marker)
                    })
        {
            edit = SourceEdit {
                range: cell.clone(),
                text: format!("{marker}{value}{marker}"),
            };
            point = cell.start + marker.len();
        }
        if range != *cell && !range.is_empty() {
            let shown = projection
                .display_selection(Selection {
                    anchor: range.start,
                    head: range.end,
                })
                .range();
            let mut expected = projection.text[display_cell.clone()].to_owned();
            expected.replace_range(
                shown.start - display_cell.start..shown.end - display_cell.start,
                if caret.is_none() { text } else { "" },
            );
            let desired_caret =
                shown.start - display_cell.start + if caret.is_none() { text.len() } else { 0 };
            let mut candidate = source[cell.clone()].to_owned();
            candidate.replace_range(
                edit.range.start - cell.start..edit.range.end - cell.start,
                &edit.text,
            );
            let wrapper = format!("| {candidate} |\n|---|\n");
            let parsed = Projection::new(&wrapper);
            let actual = parsed
                .rich
                .tables
                .first()
                .and_then(|table| table.rows.first())
                .and_then(|row| row.first())
                .map(|range| &parsed.text[range.clone()]);
            if actual != Some(expected.as_str()) {
                let (mut raw, mut raw_caret) = literal_cell(&expected, desired_caret);
                let original = &source[cell.clone()];
                if !expected.is_empty()
                    && let Some(marker) = ["***", "___", "**", "__", "~~", "*", "_"]
                        .into_iter()
                        .find(|marker| {
                            original.len() > marker.len() * 2
                                && original.starts_with(marker)
                                && original.ends_with(marker)
                        })
                {
                    raw = format!("{marker}{raw}{marker}");
                    raw_caret += marker.len();
                }
                edit = SourceEdit {
                    range: cell.clone(),
                    text: raw,
                };
                point = cell.start + raw_caret - value.len();
            }
        }
        caret.get_or_insert(point + value.len());
        edits.push(edit);
    }
    finish(projection, source, table, edits, caret?)
}

/// Partially selected tables remain blocks when a document selection crosses
/// their boundaries. Whole selected table blocks can still be deleted normally.
pub fn replace_document_selection(
    projection: &Projection,
    source: &str,
    selection: Selection,
    text: &str,
) -> Option<TableEdit> {
    if selection.is_caret() || cell_at(projection, selection).is_some() {
        return None;
    }
    if let Some(edit) = replace_across_cells(projection, source, selection, text) {
        return Some(edit);
    }
    let selected = selection.range();
    let partial = projection
        .rich
        .tables
        .iter()
        .filter(|table| {
            selected.start < table.source_range.end
                && table.source_range.start < selected.end
                && !(selected.start <= table.source_range.start
                    && table.source_range.end <= selected.end)
        })
        .collect::<Vec<_>>();
    if partial.is_empty() {
        return None;
    }
    let mut edits = Vec::new();
    let mut caret = None;
    let mut cursor = selected.start;
    for table in &partial {
        let line = source[..table.source_range.start]
            .rfind(['\r', '\n'])
            .map_or(0, |n| n + 1);
        let ending = if source[table.source_range.clone()].contains("\r\n") {
            "\r\n"
        } else if source[table.source_range.clone()].contains('\r') {
            "\r"
        } else {
            "\n"
        };
        if cursor < line {
            let mut edit = outside_edit(
                projection,
                source,
                cursor..line,
                if caret.is_none() { text } else { "" },
            );
            caret.get_or_insert(edit.selection.head);
            let has_text =
                !source[..edit.range.start].trim().is_empty() || !edit.text.trim().is_empty();
            if has_text {
                let count = ending_count(
                    edit.text
                        .chars()
                        .rev()
                        .chain(source[..edit.range.start].chars().rev()),
                );
                edit.text
                    .push_str(&ending.repeat(2usize.saturating_sub(count)));
            }
            edits.push(SourceEdit {
                range: edit.range,
                text: edit.text,
            });
        }
        let range =
            selected.start.max(table.source_range.start)..selected.end.min(table.source_range.end);
        let plan = replace_table_range(
            projection,
            source,
            table,
            range,
            if caret.is_none() { text } else { "" },
        )?;
        caret.get_or_insert(plan.selection.head);
        edits.extend(plan.edits);
        cursor = table.source_range.end;
    }
    if cursor < selected.end {
        let mut edit = outside_edit(projection, source, cursor..selected.end, "");
        if !source[edit.range.end..].trim().is_empty() {
            let table = partial.last()?;
            let ending = if source[table.source_range.clone()].contains("\r\n") {
                "\r\n"
            } else if source[table.source_range.clone()].contains('\r') {
                "\r"
            } else {
                "\n"
            };
            let count = ending_count(source[..cursor].chars().rev());
            let next = if edit.text.is_empty() {
                &source[edit.range.end..]
            } else {
                &edit.text
            };
            let leading = ending_count(next.chars());
            edit.text
                .insert_str(0, &ending.repeat(2usize.saturating_sub(count + leading)));
        }
        edits.push(SourceEdit {
            range: edit.range,
            text: edit.text,
        });
    }
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    Some(TableEdit {
        edits,
        selection: Selection::caret(caret?),
    })
}

fn ending_count(characters: impl Iterator<Item = char>) -> usize {
    let mut count = 0;
    let mut characters = characters.peekable();
    while let Some(character) = characters.next() {
        if !matches!(character, '\r' | '\n') {
            break;
        }
        count += 1;
        if characters
            .peek()
            .is_some_and(|&next| matches!((character, next), ('\r', '\n') | ('\n', '\r')))
        {
            characters.next();
        }
    }
    count
}

fn outside_edit(
    projection: &Projection,
    source: &str,
    range: Range<usize>,
    text: &str,
) -> crate::document::projection::ProjectedEdit {
    let before = Selection {
        anchor: range.start,
        head: range.end,
    };
    let shown = projection.display_selection(before).range();
    let mut after = projection.text.clone();
    after.replace_range(shown.clone(), text);
    let edit = projection.edit(
        source,
        &after,
        before,
        Selection::caret(shown.start + text.len()),
    );
    if edit.range.end <= range.end {
        edit
    } else {
        crate::document::projection::ProjectedEdit {
            selection: Selection::caret(range.start + text.len()),
            range,
            text: text.into(),
        }
    }
}

/// Once typing continues after an encoded trailing space, ordinary interior
/// spaces are representable again. Keep the Markdown readable in that same edit.
pub fn interior_spaces(
    projection: &Projection,
    source: &str,
    edit: &mut crate::document::projection::ProjectedEdit,
) {
    if !edit.range.is_empty()
        || edit
            .text
            .chars()
            .any(|character| character.is_whitespace() || character.is_ascii_punctuation())
    {
        return;
    }
    let Some((table, row, column)) = cell_at(projection, Selection::caret(edit.range.start)) else {
        return;
    };
    let cell = &table.source_rows[row][column];
    let mut start = edit.range.start;
    while start >= cell.start + 5 && source.get(start - 5..start) == Some("&#32;") {
        start -= 5;
    }
    if start == edit.range.start {
        return;
    }
    let shown = projection.to_display(start);
    let count = (edit.range.start - start) / 5;
    if projection.to_display(edit.range.start) - shown != count
        || projection.text[table.rows[row][column].start..shown]
            .trim()
            .is_empty()
    {
        return;
    }
    edit.range.start = start;
    edit.text.insert_str(0, &" ".repeat(count));
    edit.selection.anchor -= count * 4;
    edit.selection.head -= count * 4;
}

fn finish(
    _projection: &Projection,
    source: &str,
    table: &crate::document::rich_document::Table,
    mut edits: Vec<SourceEdit>,
    mut caret: usize,
) -> Option<TableEdit> {
    let original_caret = edits.first()?.range.start;
    for (index, edit) in edits.iter_mut().enumerate() {
        if edit
            .text
            .bytes()
            .rev()
            .take_while(|&byte| byte == b'\\')
            .count()
            % 2
            == 1
            && source[edit.range.end..].starts_with('|')
        {
            edit.text.push('\\');
            if index == 0 {
                caret += 1;
            }
        }
    }
    let rows = controls::source_rows(source, table).ok()?;
    // Empty edge values in a table without outer pipes can otherwise become
    // edge delimiters themselves, shifting data or invalidating the header.
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    let mut boundaries = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let first_edit = edits.partition_point(|edit| edit.range.end < row.content.start);
        let last_edit = edits.partition_point(|edit| edit.range.start <= row.content.end);
        if index == 1 || first_edit == last_edit {
            continue;
        }
        let row_edits = &edits[first_edit..last_edit];
        for (first, cell) in [(true, row.cells.first()?), (false, row.cells.last()?)] {
            let mut value = source[cell.clone()].to_owned();
            for edit in row_edits
                .iter()
                .rev()
                .filter(|edit| cell.start <= edit.range.start && edit.range.end <= cell.end)
            {
                value.replace_range(
                    edit.range.start - cell.start..edit.range.end - cell.start,
                    &edit.text,
                );
            }
            if value.trim().is_empty() {
                let point = if first {
                    row.content.start
                } else {
                    row.content.end
                };
                let has_pipe = if first {
                    source[row.content.clone()].trim_start().starts_with('|')
                } else {
                    source[row.content.clone()].trim_end().ends_with('|')
                };
                if !has_pipe {
                    if point <= original_caret {
                        caret += 1;
                    }
                    boundaries.push(SourceEdit {
                        range: point..point,
                        text: "|".into(),
                    });
                }
            }
        }
    }
    edits.extend(boundaries);
    // Coalesce insertions at a replacement boundary into that replacement, so
    // the transaction has unambiguous, non-overlapping original-source ranges.
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    let mut merged: Vec<SourceEdit> = Vec::new();
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
    Some(TableEdit {
        edits: merged,
        selection: Selection::caret(caret),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::document::collaboration::EditOrigin;

    #[test]
    fn replacing_forward_and_reverse_cell_selections_preserves_the_grid_and_outside_source() {
        for ending in ["\n", "\r\n", "\r"] {
            for edges in [false, true] {
                for reverse in [false, true] {
                    for text in ["", "café | 🧑‍💻", "first\nsecond"] {
                        let row = if edges { "| A | B |" } else { "A | B" };
                        let source = format!(
                            "Before{ending}{ending}{row}{ending}---|---{ending}left | right{ending}{ending}After"
                        );
                        let projection = Projection::new(&source);
                        let cells = &projection.rich.tables[0].source_rows;
                        let (start, end) = (cells[0][0].start, cells[1][1].end);
                        let before = if reverse {
                            Selection {
                                anchor: end,
                                head: start,
                            }
                        } else {
                            Selection {
                                anchor: start,
                                head: end,
                            }
                        };
                        let plan =
                            replace_across_cells(&projection, &source, before, text).unwrap();
                        let mut document = Document::new(source.clone());
                        document
                            .transact(
                                &plan.edits,
                                EditOrigin::Human,
                                Some((before, plan.selection)),
                                true,
                            )
                            .unwrap();
                        let after = Projection::new(document.source());
                        assert_eq!(after.rich.tables.len(), 1, "{}", document.source());
                        assert_eq!(after.rich.tables[0].rows.len(), 2);
                        assert_eq!(after.rich.tables[0].alignment.len(), 2);
                        assert!(
                            document
                                .source()
                                .starts_with(&format!("Before{ending}{ending}"))
                        );
                        assert!(
                            document
                                .source()
                                .ends_with(&format!("{ending}{ending}After"))
                        );
                        let cell = &after.rich.tables[0].rows[0][0];
                        assert_eq!(&after.text[cell.clone()], text);
                        assert!(
                            after.rich.tables[0]
                                .rows
                                .iter()
                                .flatten()
                                .skip(1)
                                .all(|cell| cell.is_empty())
                        );
                        document.undo().unwrap();
                        assert_eq!(document.source(), source);
                    }
                }
            }
        }
    }

    #[test]
    fn partial_formatted_boundary_cells_keep_visible_unselected_text() {
        let source = "| A | B |\n|---|---|\n| **prefix old** | *old suffix* |\n";
        let projection = Projection::new(source);
        let selection = Selection {
            anchor: source.find("old").unwrap(),
            head: source.rfind("old").unwrap() + 3,
        };
        let plan = replace_across_cells(&projection, source, selection, "new").unwrap();
        let mut document = Document::new(source.into());
        document
            .transact(&plan.edits, EditOrigin::Human, None, true)
            .unwrap();
        let projected = Projection::new(document.source());
        let row = &projected.rich.tables[0].rows[1];
        assert_eq!(&projected.text[row[0].clone()], "prefix new");
        assert_eq!(&projected.text[row[1].clone()], " suffix");
    }

    #[test]
    fn only_bare_inline_table_breaks_are_projected_and_prepared_for_read() {
        let source = "Outside <br>\n\n| A | B |\n|---|---|\n| one<br>two<BR />three | `<br>` and \\<br> and <br onclick=bad> |\n";
        let projection = Projection::new(source);
        assert_eq!(
            &projection.text[projection.rich.tables[0].rows[1][0].clone()],
            "one\ntwo\nthree"
        );
        assert!(projection.text.contains("Outside <br>"));
        assert!(
            projection
                .text
                .contains("<br> and <br> and <br onclick=bad>")
        );
        let read = read_breaks(source);
        assert!(read.contains("one&#10;two&#10;three"));
        assert!(read.contains("`<br>` and \\<br> and <br onclick=bad>"));
    }
}

//! Values-only spreadsheet interchange without changing the clipboard UI.
//!
//! A quote at the beginning of a field is structural only when it forms a
//! complete quoted field. Incomplete or ordinary quote-leading text remains
//! literal, with physical tabs and line endings continuing to delimit cells.

use super::collaboration::{MAX_SOURCE_BYTES, MAX_TRANSACTION_EDITS};

const SIZE_ERROR: &str = "The clipboard supports up to 16 MiB of text.";
const CELLS_ERROR: &str = "The clipboard supports up to 65,536 cells.";

/// Decode spreadsheet text, retaining empty values and padding ragged rows.
/// CRLF is one record ending; bare CR and LF are also accepted. One terminal
/// record separator is ignored, as spreadsheet applications commonly supply it.
pub(crate) fn decode(text: &str) -> Result<Vec<Vec<String>>, &'static str> {
    decode_fields(text, true)
}

/// Native Sheets text quotes fields containing physical tabs or line endings,
/// but leaves quote-only values such as `"literal"` verbatim. The caller must
/// establish the native producer separately; ordinary TSV uses `decode`. Sheets
/// omits an optional record terminator, so a final ending means an empty row.
pub(crate) fn decode_sheets(text: &str) -> Result<Vec<Vec<String>>, &'static str> {
    decode_fields(text, false)
}

fn decode_fields(text: &str, quote_only_fields: bool) -> Result<Vec<Vec<String>>, &'static str> {
    if text.len() > MAX_SOURCE_BYTES {
        return Err(SIZE_ERROR);
    }
    let bytes = text.as_bytes();
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut width = 0;
    let mut start = 0;
    loop {
        let quoted = (bytes.get(start) == Some(&b'"'))
            .then(|| quoted_end(bytes, start))
            .flatten()
            .filter(|&end| {
                quote_only_fields || text[start + 1..end - 1].contains(['\t', '\r', '\n'])
            });
        let end = quoted.unwrap_or_else(|| {
            bytes[start..]
                .iter()
                .position(|byte| matches!(byte, b'\t' | b'\r' | b'\n'))
                .map_or(bytes.len(), |offset| start + offset)
        });
        let columns = width.max(row.len() + 1);
        if (rows.len() + 1)
            .checked_mul(columns)
            .is_none_or(|cells| cells > MAX_TRANSACTION_EDITS)
        {
            return Err(CELLS_ERROR);
        }
        // Decoding only removes quote syntax or shrinks CRLF, so aggregate
        // String capacity stays bounded by the already-checked input length.
        let value = if quoted.is_some() {
            unquote(&text[start + 1..end - 1])
        } else {
            text[start..end].to_owned()
        };
        row.push(value);
        match bytes.get(end) {
            None => {
                width = columns;
                rows.push(row);
                break;
            }
            Some(b'\t') => start = end + 1,
            Some(ending) => {
                width = columns;
                rows.push(row);
                row = Vec::new();
                start = end + 1;
                if *ending == b'\r' && bytes.get(start) == Some(&b'\n') {
                    start += 1;
                }
                if start == bytes.len() && quote_only_fields {
                    break;
                }
            }
        }
    }
    // Every intermediate padded rectangle was bounded before allocating cells.
    for row in &mut rows {
        row.resize_with(width, String::new);
    }
    Ok(rows)
}

/// Copy visible values as quoted TSV with conventional CRLF record separators.
/// Plain single-cell copies have no added newline. A final empty one-column
/// record needs an extra terminal separator to distinguish it from a terminator.
pub(crate) fn encode(rows: &[Vec<String>]) -> String {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let mut result = String::new();
    for (row_index, row) in rows.iter().enumerate() {
        if row_index != 0 {
            result.push_str("\r\n");
        }
        for column in 0..width {
            if column != 0 {
                result.push('\t');
            }
            let value = row.get(column).map_or("", String::as_str);
            if value.contains(['\t', '\r', '\n', '"']) {
                result.push('"');
                for character in value.chars() {
                    if character == '"' {
                        result.push('"');
                    }
                    result.push(character);
                }
                result.push('"');
            } else {
                result.push_str(value);
            }
        }
    }
    if rows.len() > 1
        && width == 1
        && rows
            .last()
            .is_some_and(|row| row.first().is_none_or(String::is_empty))
    {
        result.push_str("\r\n");
    }
    result
}

// Sheets reads typed cell metadata through its native origin wrapper. Keep the
// Dosty marker as well: our own quoted TSV still follows standard quote rules.
const HTML_START: &str = "<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\"><head><meta charset=\"utf-8\"></head><body><google-sheets-html-origin><table data-dosty=\"table\" style=\"border-collapse:collapse;white-space:pre\"><tbody>";
const HTML_END: &str = "</tbody></table></google-sheets-html-origin></body></html>";
const CELL_START: &str = "<td x:str data-sheets-value=\"{&quot;1&quot;:2,&quot;2&quot;:";
const CELL_BODY: &str = "}\" style=\"mso-number-format:&quot;\\@&quot;;white-space:pre\">";
const CELL_END: &str = "</td>";
const SPACERUN_START: &str = "<span style=\"mso-spacerun:yes\">";
const SPACERUN_END: &str = "</span>";

/// Supply a values-only rich clipboard alongside TSV. Typed string metadata and
/// Excel's text format request literal handling for identifiers, dates and
/// formula-looking text. No markup from a value can become executable HTML.
/// Physical CR/CRLF cell breaks normalize to LF; empty cells and rows remain.
pub(crate) fn encode_html(rows: &[Vec<String>]) -> Result<String, &'static str> {
    if rows.is_empty() {
        return Err("Clipboard cells must form a nonempty rectangle.");
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    if rows
        .len()
        .checked_mul(width)
        .is_none_or(|cells| cells > MAX_TRANSACTION_EDITS)
    {
        return Err(CELLS_ERROR);
    }
    let mut input_bytes = 0usize;
    let mut size = HTML_START.len() + HTML_END.len();
    // Preflight the entire serialization, including JSON and HTML escaping,
    // before allocating its output or an individual serialized JSON string.
    for row in rows {
        size = bounded_add(size, "<tr></tr>".len())?;
        for column in 0..width {
            let value = row.get(column).map_or("", String::as_str);
            input_bytes = bounded_add(input_bytes, value.len())?;
            size = bounded_add(size, CELL_START.len() + CELL_BODY.len() + CELL_END.len())?;
            size = bounded_add(size, html_value_length(value)?)?;
        }
    }
    let mut output = String::with_capacity(size);
    output.push_str(HTML_START);
    for row in rows {
        output.push_str("<tr>");
        for column in 0..width {
            let value = row.get(column).map_or("", String::as_str);
            let value = if value.contains('\r') {
                std::borrow::Cow::Owned(value.replace("\r\n", "\n").replace('\r', "\n"))
            } else {
                std::borrow::Cow::Borrowed(value)
            };
            let json = serde_json::to_string(value.as_ref())
                .map_err(|_| "Couldn't encode spreadsheet clipboard values.")?;
            output.push_str(CELL_START);
            for character in json.chars() {
                push_html_character(&mut output, character);
            }
            output.push_str(CELL_BODY);
            push_cell_text(&mut output, &value);
            output.push_str(CELL_END);
        }
        output.push_str("</tr>");
    }
    output.push_str(HTML_END);
    debug_assert_eq!(output.len(), size);
    Ok(output)
}

fn bounded_add(size: usize, addition: usize) -> Result<usize, &'static str> {
    size.checked_add(addition)
        .filter(|&size| size <= MAX_SOURCE_BYTES)
        .ok_or(SIZE_ERROR)
}

fn html_value_length(value: &str) -> Result<usize, &'static str> {
    // The JSON string's two surrounding quotes each become `&quot;`.
    let mut size = 12;
    let mut previous_cr = false;
    let mut spacerun = false;
    for character in value.chars() {
        if character == '\n' && previous_cr {
            previous_cr = false;
            continue;
        }
        previous_cr = character == '\r';
        let character = if previous_cr { '\n' } else { character };
        let json = match character {
            '"' => 7, // JSON backslash plus HTML-escaped quotation mark.
            '\\' | '\u{8}' | '\u{c}' | '\n' | '\r' | '\t' => 2,
            '\u{0}'..='\u{1f}' => 6,
            _ => html_character_length(character),
        };
        let space = matches!(character, ' ' | '\t');
        if space && !spacerun {
            size = bounded_add(size, SPACERUN_START.len() + SPACERUN_END.len())?;
        }
        spacerun = space;
        let text = match character {
            ' ' => "&nbsp;".len(),
            '\t' => "&#9;".len(),
            '\n' => "<br>".len(),
            _ => html_character_length(character),
        };
        size = bounded_add(size, json + text)?;
    }
    Ok(size)
}

fn push_cell_text(output: &mut String, value: &str) {
    let mut spacerun = false;
    for character in value.chars() {
        let space = matches!(character, ' ' | '\t');
        if space && !spacerun {
            output.push_str(SPACERUN_START);
        } else if !space && spacerun {
            output.push_str(SPACERUN_END);
        }
        spacerun = space;
        match character {
            ' ' => output.push_str("&nbsp;"),
            '\t' => output.push_str("&#9;"),
            '\n' => output.push_str("<br>"),
            _ => push_html_character(output, character),
        }
    }
    if spacerun {
        output.push_str(SPACERUN_END);
    }
}

fn html_character_length(character: char) -> usize {
    match character {
        '&' => 5,
        '<' | '>' => 4,
        '"' => 6,
        '\'' => 5,
        _ => character.len_utf8(),
    }
}

fn push_html_character(output: &mut String, character: char) {
    match character {
        '&' => output.push_str("&amp;"),
        '<' => output.push_str("&lt;"),
        '>' => output.push_str("&gt;"),
        '"' => output.push_str("&quot;"),
        '\'' => output.push_str("&#39;"),
        _ => output.push(character),
    }
}

/// Return the first unescaped closing quote only if it completes this field.
/// All byte slicing boundaries are ASCII syntax, hence UTF-8 boundaries.
fn quoted_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 1;
    while let Some(byte) = bytes.get(index) {
        if *byte == b'"' {
            if bytes.get(index + 1) == Some(&b'"') {
                index += 2;
                continue;
            }
            return match bytes.get(index + 1) {
                None | Some(b'\t' | b'\r' | b'\n') => Some(index + 1),
                _ => None,
            };
        }
        index += 1;
    }
    None
}

fn unquote(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' => {
                // The quoted-field preflight certified every interior pair.
                characters.next();
                result.push('"');
            }
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                result.push('\n');
            }
            _ => result.push(character),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(values: &[&[&str]]) -> Vec<Vec<String>> {
        values
            .iter()
            .map(|row| row.iter().map(|value| (*value).to_owned()).collect())
            .collect()
    }

    #[test]
    fn spreadsheet_quotes_retain_cell_boundaries_unicode_and_embedded_endings() {
        let text = "Name\tNotes\tValue\r\n\"Maya\tDesign\"\t\"first\r\nsecond\rthird\nfourth\"\t\"say \"\"café 🧑‍💻\"\"\"\r\n";
        assert_eq!(
            decode(text).unwrap(),
            rows(&[
                &["Name", "Notes", "Value"],
                &[
                    "Maya\tDesign",
                    "first\nsecond\nthird\nfourth",
                    "say \"café 🧑‍💻\"",
                ],
            ])
        );
    }

    #[test]
    fn native_sheets_text_keeps_literal_quotes_and_structural_delimiters() {
        // Captured from the native Sheets clipboard, including its final empty
        // row and its unescaped quote-only C1 value (confirmed in formula bar).
        let text = include_str!("../../tests/fixtures/google-sheets-clipboard.tsv");
        assert_eq!(
            decode_sheets(text).unwrap(),
            rows(&[
                &["plain", "say \"hello\"", "\"literal\"", "end"],
                &["a\tb", "line1\nline2", "top\n\nbottom", ""],
                &[
                    "日本語 café",
                    "e\u{301} 🧑‍💻",
                    "**literal** | &amp; \\",
                    "spaced"
                ],
                &["123", "12345678901234567890", "2", "2026-11-03"],
                &["", "", "", ""],
            ])
        );
        assert_eq!(decode("\"literal\"").unwrap(), rows(&[&["literal"]]));
        assert_eq!(
            decode_sheets("\"literal\"").unwrap(),
            rows(&[&["\"literal\""]])
        );
        assert_eq!(
            decode_sheets("\"say \"\"hi\"\"\tthere\"\t\"first\r\nsecond\"\t\"not closed").unwrap(),
            rows(&[&["say \"hi\"\tthere", "first\nsecond", "\"not closed"]])
        );
        assert_eq!(
            decode_sheets("\"\"\t\"a\"suffix\t\"\"\"").unwrap(),
            rows(&[&["\"\"", "\"a\"suffix", "\"\"\""]])
        );
    }

    #[test]
    fn native_sheets_keeps_terminal_empty_rows_without_changing_standard_tsv() {
        // Native D1:D2 was ["end", ""]; retain the exact clipboard capture.
        let captured = include_str!("../../tests/fixtures/google-sheets-trailing-empty.tsv");
        assert_eq!(captured, "end\n");
        assert_eq!(decode_sheets(captured).unwrap(), rows(&[&["end"], &[""]]));
        assert_eq!(decode(captured).unwrap(), rows(&[&["end"]]));
        for ending in ["\n", "\r\n", "\r"] {
            assert_eq!(
                decode_sheets(&format!("end{ending}")).unwrap(),
                rows(&[&["end"], &[""]])
            );
            assert_eq!(decode_sheets(ending).unwrap(), rows(&[&[""], &[""]]));
            assert_eq!(
                decode_sheets(&ending.repeat(2)).unwrap(),
                rows(&[&[""], &[""], &[""]])
            );
            assert_eq!(
                decode_sheets(&format!("end\t{ending}")).unwrap(),
                rows(&[&["end", ""], &["", ""]])
            );
            assert_eq!(
                decode_sheets(&format!("\"line{ending}\"")).unwrap(),
                rows(&[&["line\n"]])
            );
            assert_eq!(decode(ending).unwrap(), rows(&[&[""]]));
        }
        let maximum = "\n".repeat(MAX_TRANSACTION_EDITS - 1);
        assert_eq!(
            decode_sheets(&maximum).unwrap().len(),
            MAX_TRANSACTION_EDITS
        );
        assert_eq!(decode_sheets(&format!("{maximum}\n")), Err(CELLS_ERROR));
        let row = format!("{}x\n", "x\t".repeat(255));
        let maximum = row.repeat(256);
        assert_eq!(
            decode_sheets(maximum.strip_suffix('\n').unwrap())
                .unwrap()
                .len(),
            256
        );
        assert_eq!(decode_sheets(&maximum), Err(CELLS_ERROR));
    }

    fn html_values(html: &str) -> Vec<String> {
        html.split("data-sheets-value=\"")
            .skip(1)
            .map(|cell| {
                let encoded = cell.split_once('"').unwrap().0;
                let json = encoded
                    .replace("&quot;", "\"")
                    .replace("&#39;", "'")
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&amp;", "&");
                let value: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(value["1"], 2, "Every clipboard cell is a string value.");
                value["2"].as_str().unwrap().to_owned()
            })
            .collect()
    }

    #[test]
    fn rich_clipboard_escapes_values_and_preserves_text_types_and_blank_cells() {
        let data = rows(&[
            &["00123", "12345678901234567890", "=1+1", "2026-11-03"],
            &[
                "\"literal\" &quot; '",
                "</td><script>alert(\"x\")</script> &amp;",
                "日本語 e\u{301} 🧑‍💻\tend",
                "  first\r\n\rsecond\nthird  ",
            ],
            &[""],
        ]);
        let html = encode_html(&data).unwrap();
        let mut expected = data.clone();
        expected[1][3] = "  first\n\nsecond\nthird  ".into();
        expected[2].resize(4, String::new());
        assert_eq!(
            html_values(&html),
            expected.into_iter().flatten().collect::<Vec<_>>()
        );
        assert_eq!(html.matches("<tr>").count(), 3);
        assert_eq!(html.matches("<td ").count(), 12);
        assert!(html.contains("data-dosty=\"table\""));
        assert!(html.contains("xmlns:x=\"urn:schemas-microsoft-com:office:excel\""));
        assert!(html.contains("x:str"));
        assert!(html.contains("mso-number-format:&quot;\\@&quot;"));
        assert!(html.contains("xmlns=\"http://www.w3.org/1999/xhtml\""));
        assert!(html.contains("<google-sheets-html-origin><table data-dosty=\"table\""));
        assert!(html.contains("</tbody></table></google-sheets-html-origin>"));
        assert_eq!(html.matches("white-space:pre").count(), 13);
        assert!(html.contains(&format!(
            "{SPACERUN_START}&nbsp;&nbsp;{SPACERUN_END}first<br><br>second<br>third{SPACERUN_START}&nbsp;&nbsp;{SPACERUN_END}"
        )));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains('\r'));
    }

    #[test]
    fn exported_whitespace_round_trips_through_sheets_and_excel_profiles() {
        let data = rows(&[
            &["  leading", "trailing  ", "a  b", "a\t b\t\tc"],
            &[
                " \t ",
                "\u{a0} a\u{a0} \tb\u{a0}",
                "first \r\n\t second\n ",
                "日本語 & <literal>",
            ],
            &["", "", "", ""],
        ]);
        let html = encode_html(&data).unwrap();
        let mut expected = data.clone();
        expected[1][2] = "first \n\t second\n ".into();
        assert_eq!(
            crate::document::table_clipboard_html::decode(&html).unwrap(),
            expected
        );
        assert_eq!(
            crate::document::table_clipboard_html::decode_excel(&html).unwrap(),
            expected
        );
        assert_eq!(decode(&encode(&data)).unwrap(), expected);
        assert_eq!(
            html_values(&html),
            expected.into_iter().flatten().collect::<Vec<_>>()
        );
        assert_eq!(encode(&rows(&[&["  literal  "]])), "  literal  ");

        // Only generated ASCII-space/tab runs carry Excel's spacerun marker;
        // an actual NBSP remains outside, so the Excel reader retains it.
        let html = encode_html(&rows(&[&[" \t\u{a0} \t"]])).unwrap();
        let text = html
            .split_once(CELL_BODY)
            .unwrap()
            .1
            .split_once(CELL_END)
            .unwrap()
            .0;
        assert_eq!(
            text,
            format!(
                "{SPACERUN_START}&nbsp;&#9;{SPACERUN_END}\u{a0}{SPACERUN_START}&nbsp;&#9;{SPACERUN_END}"
            )
        );
    }

    #[test]
    fn whitespace_preflight_counts_every_run_wrapper_at_the_output_limit() {
        let empty = encode_html(&rows(&[&[""]])).unwrap().len();
        let run = html_value_length(" ").unwrap() - html_value_length("").unwrap();
        let padding = (MAX_SOURCE_BYTES - empty - run) / 2;
        let mut value = format!("{} ", "x".repeat(padding));
        let html = encode_html(&[vec![value.clone()]]).unwrap();
        assert!((MAX_SOURCE_BYTES - 1..=MAX_SOURCE_BYTES).contains(&html.len()));
        value.push('x');
        assert_eq!(encode_html(&[vec![value]]), Err(SIZE_ERROR));

        // Alternating text and spaces produces a new wrapper for every space.
        let per_run = SPACERUN_START.len() + SPACERUN_END.len() + 9;
        let many_runs = "x ".repeat(MAX_SOURCE_BYTES / per_run + 1);
        assert_eq!(encode_html(&[vec![many_runs]]), Err(SIZE_ERROR));
    }

    #[test]
    fn rich_clipboard_limits_include_escaping_and_the_padded_rectangle() {
        assert!(encode_html(&[]).is_err());
        assert_eq!(
            html_values(&encode_html(&[vec![], vec![]]).unwrap()),
            vec![String::new(), String::new()]
        );
        assert_eq!(
            encode_html(&[vec!["x".repeat(MAX_SOURCE_BYTES + 1)]]),
            Err(SIZE_ERROR)
        );
        // Each '<' needs four bytes in both metadata and visible HTML text.
        assert_eq!(
            encode_html(&[vec!["<".repeat(MAX_SOURCE_BYTES / 8 + 1)]]),
            Err(SIZE_ERROR)
        );
        let mut sparse = vec![vec![String::new(); 1024]];
        sparse.resize_with(65, Vec::new);
        assert_eq!(encode_html(&sparse), Err(CELLS_ERROR));
        let controls: String = (0..32).map(char::from).collect();
        let html = encode_html(&[vec![controls.clone()]]).unwrap();
        assert_eq!(
            html_values(&html),
            vec![controls.replace("\r\n", "\n").replace('\r', "\n")]
        );
    }

    #[test]
    fn rectangular_values_and_final_empty_rows_round_trip() {
        for values in [
            rows(&[&[""]]),
            rows(&[&["plain value"]]),
            rows(&[&["first"], &[""]]),
            rows(&[&[""], &[""]]),
            rows(&[&[""], &[""], &[""]]),
            rows(&[&["a", "b"], &["", ""]]),
            rows(&[&["\"literal\"", "one\ttwo"], &["日本語\nnext", "a|b\\c"]]),
        ] {
            assert_eq!(decode(&encode(&values)).unwrap(), values);
        }
        assert_eq!(encode(&rows(&[&["plain value"]])), "plain value");
        assert_eq!(encode(&rows(&[&["a", "b"], &["c", "d"]])), "a\tb\r\nc\td");
        assert_eq!(encode(&rows(&[&["a"], &[""]])), "a\r\n\r\n");
    }

    #[test]
    fn ordinary_quotes_and_incomplete_quoted_fields_remain_literal() {
        for text in [
            "say \"hello\"",
            "\"not closed",
            "\"a\"suffix",
            "a\"b",
            "\"\"\"",
        ] {
            assert_eq!(decode(text).unwrap(), rows(&[&[text]]));
        }
        assert_eq!(
            decode("\"first\"\t\"not closed").unwrap(),
            rows(&[&["first", "\"not closed"]])
        );
        assert_eq!(
            decode("\"not\nclosed").unwrap(),
            rows(&[&["\"not"], &["closed"]])
        );
        assert_eq!(
            decode("\"a\"suffix\tb").unwrap(),
            rows(&[&["\"a\"suffix", "b"]])
        );
    }

    #[test]
    fn one_terminal_separator_is_ignored_while_empty_records_and_columns_survive() {
        for ending in ["\n", "\r\n", "\r"] {
            assert_eq!(decode(ending).unwrap(), rows(&[&[""]]));
            assert_eq!(decode(&format!("a{ending}")).unwrap(), rows(&[&["a"]]));
            assert_eq!(
                decode(&format!("a{ending}{ending}")).unwrap(),
                rows(&[&["a"], &[""]])
            );
            assert_eq!(
                decode(&format!("a\t{ending}")).unwrap(),
                rows(&[&["a", ""]])
            );
        }
        assert_eq!(decode("").unwrap(), rows(&[&[""]]));
        assert_eq!(decode("\t").unwrap(), rows(&[&["", ""]]));
        assert_eq!(decode("\"a\n\"\n").unwrap(), rows(&[&["a\n"]]));
    }

    #[test]
    fn ragged_rows_are_rectangular_without_trimming_values() {
        assert_eq!(
            decode("  a \tb\tc\nx\n\tz\t").unwrap(),
            rows(&[&["  a ", "b", "c"], &["x", "", ""], &["", "z", ""]])
        );
    }

    #[test]
    fn resource_limits_bound_text_and_the_padded_rectangle() {
        assert_eq!(decode(&"x".repeat(MAX_SOURCE_BYTES + 1)), Err(SIZE_ERROR));
        let row = format!("{}x\n", "x\t".repeat(255));
        let maximum = row.repeat(256);
        let cells = decode(&maximum).unwrap();
        assert_eq!(cells.len(), 256);
        assert!(cells.iter().all(|row| row.len() == 256));
        assert_eq!(decode(&format!("{maximum}\n")), Err(CELLS_ERROR));
        // A small ragged payload must not request a huge padded allocation.
        let sparse = format!("{}\n{}", "\t".repeat(1023), "x\n".repeat(64));
        assert_eq!(decode(&sparse), Err(CELLS_ERROR));
    }
}

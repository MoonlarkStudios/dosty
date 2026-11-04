//! Optional rich clipboard metadata. Callers run these blocking methods on a
//! worker; plain-text clipboard handling remains with the focused editor.

use std::sync::Mutex;

use crate::document::collaboration::MAX_SOURCE_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Producer {
    Standard,
    Sheets,
    Excel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub producer: Producer,
    pub values: Option<Vec<Vec<String>>>,
}

impl Snapshot {
    fn standard() -> Self {
        Self {
            producer: Producer::Standard,
            values: None,
        }
    }
}

/// Keep the native clipboard owner alive after a worker returns. In particular,
/// Linux serves copied data from the process that owns the clipboard.
#[derive(Default)]
pub struct Clipboard {
    owner: Mutex<Option<arboard::Clipboard>>,
}

impl Clipboard {
    /// Inspect metadata only when the native plain text still matches the text
    /// captured by the editor, before/after the HTML read and after parsing its
    /// cells. Unavailable metadata leaves ordinary TSV handling to the caller.
    pub fn read(&self, expected: &str) -> Result<Snapshot, String> {
        if expected.len() > MAX_SOURCE_BYTES {
            return Err("The clipboard supports up to 16 MiB of text.".into());
        }
        // Distinguish failure to open native metadata (ordinary TSV fallback)
        // from an unsafe/changed spreadsheet snapshot (reject without source edits).
        with_owner(&self.owner, native, |clipboard| {
            Ok(read_native(clipboard, expected))
        })
        .unwrap_or_else(|_| Ok(Snapshot::standard()))
    }

    /// A single blank spreadsheet cell may have no native text flavor. Accept
    /// it only through a stable, recognized, entirely empty HTML rectangle.
    /// An unavailable ordinary clipboard never implies a blank cell.
    pub fn read_empty(&self) -> Result<Option<Snapshot>, String> {
        with_owner(&self.owner, native, |clipboard| {
            read_empty_with(|| {
                let text = optional_content(clipboard.get_text())?;
                if text.as_ref().is_some_and(|text| !text.is_empty()) {
                    return Ok((text, None));
                }
                let html = optional_content(clipboard.get().html())?;
                Ok((text, html))
            })
        })
    }

    /// Enrich the editor's already-verified copy only while it is current.
    /// Native clipboard APIs provide no atomic compare-and-set; the text check
    /// is deliberately adjacent to the write, under this owner's lock.
    pub fn write(&self, expected: &str, html: &str) -> Result<(), String> {
        if expected.len() > MAX_SOURCE_BYTES || html.len() > MAX_SOURCE_BYTES {
            return Err("The clipboard supports up to 16 MiB per format.".into());
        }
        with_owner(&self.owner, native, |clipboard| {
            let current = clipboard
                .get_text()
                .map_err(|error| format!("Cannot read the clipboard: {error}"))?;
            if !same_text(&current, expected) {
                return Err("The clipboard changed before its table format was added.".into());
            }
            clipboard
                .set_html(html, Some(expected))
                .map_err(|error| format!("Cannot add the clipboard's table format: {error}"))?;
            // arboard considers either flavor succeeding a successful write.
            // Do not report success if the plain-text alternative was lost.
            let copied = clipboard
                .get_text()
                .map_err(|error| format!("Cannot verify the clipboard's text: {error}"))?;
            if !same_text(&copied, expected) {
                return Err("The clipboard's table text could not be verified.".into());
            }
            Ok(())
        })
    }
}

fn optional_content(result: Result<String, arboard::Error>) -> Result<Option<String>, String> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(format!("Cannot read the clipboard: {error}")),
    }
}

/// The reads surround cell decoding. This seam verifies changed/empty-format
/// behavior without reading or claiming the user's clipboard in unit tests.
fn read_empty_with(
    mut read: impl FnMut() -> Result<(Option<String>, Option<String>), String>,
) -> Result<Option<Snapshot>, String> {
    let (text, html) = read()?;
    if text.as_ref().is_some_and(|text| !text.is_empty()) {
        return Ok(None);
    }
    let Some(html) = html else {
        return Ok(None);
    };
    let Ok(cells) = snapshot("", "", &html, "") else {
        return Ok(None);
    };
    if !cells
        .values
        .as_ref()
        .is_some_and(|rows| rows.iter().flatten().all(String::is_empty))
    {
        return Ok(None);
    }
    let (confirmed_text, confirmed_html) = read()?;
    if confirmed_text.as_ref().is_some_and(|text| !text.is_empty())
        || confirmed_html.as_deref() != Some(html.as_str())
    {
        return Err("The clipboard changed. Paste again at the intended cell.".into());
    }
    Ok(Some(cells))
}

fn read_native(clipboard: &mut arboard::Clipboard, expected: &str) -> Result<Snapshot, String> {
    let Ok(before) = clipboard.get_text() else {
        return Ok(Snapshot::standard());
    };
    if !same_text(&before, expected) {
        return Err("The clipboard changed. Paste again at the intended cell.".into());
    }
    let Ok(html) = clipboard.get().html() else {
        return Ok(Snapshot::standard());
    };
    let after = clipboard
        .get_text()
        .map_err(|error| format!("Cannot verify the clipboard's text: {error}"))?;
    let snapshot = snapshot(expected, &before, &html, &after)?;
    if snapshot.values.is_some() {
        let confirmed = clipboard
            .get_text()
            .map_err(|error| format!("Cannot verify the clipboard's table text: {error}"))?;
        if !same_text(&confirmed, expected) {
            return Err("The clipboard changed. Paste again at the intended cell.".into());
        }
    }
    Ok(snapshot)
}

fn snapshot(expected: &str, before: &str, html: &str, after: &str) -> Result<Snapshot, String> {
    if !same_text(before, expected) || !same_text(after, expected) {
        return Err("The clipboard changed. Paste again at the intended cell.".into());
    }
    if html.len() > MAX_SOURCE_BYTES {
        return Err("The clipboard's table format exceeds 16 MiB.".into());
    }
    let producer = producer(expected, before, html, after);
    let values = match producer {
        Producer::Sheets => {
            Some(crate::document::table_clipboard_html::decode(html).map_err(str::to_owned)?)
        }
        Producer::Excel => {
            Some(crate::document::table_clipboard_html::decode_excel(html).map_err(str::to_owned)?)
        }
        Producer::Standard => None,
    };
    Ok(Snapshot { producer, values })
}

fn native() -> Result<arboard::Clipboard, String> {
    arboard::Clipboard::new().map_err(|error| format!("Cannot access the clipboard: {error}"))
}

/// A small ownership seam allows lifetime/retry tests without claiming or
/// replacing the user's system clipboard.
fn with_owner<T, R>(
    owner: &Mutex<Option<T>>,
    create: impl FnOnce() -> Result<T, String>,
    operation: impl FnOnce(&mut T) -> Result<R, String>,
) -> Result<R, String> {
    let mut owner = owner
        .lock()
        .map_err(|_| "Cannot access the clipboard owner.".to_owned())?;
    if owner.is_none() {
        *owner = Some(create()?);
    }
    // Initialization either returned an owner or exited above with an error.
    operation(owner.as_mut().expect("initialized clipboard owner"))
}

fn producer(expected: &str, before: &str, html: &str, after: &str) -> Producer {
    if html.len() > MAX_SOURCE_BYTES || !same_text(before, expected) || !same_text(after, expected)
    {
        return Producer::Standard;
    }
    let markers = markers(html);
    if !markers.dosty && markers.sheets {
        Producer::Sheets
    } else if !markers.dosty && markers.excel_progid && markers.excel_generator {
        Producer::Excel
    } else {
        Producer::Standard
    }
}

#[derive(Default)]
struct Markers {
    dosty: bool,
    sheets: bool,
    excel_progid: bool,
    excel_generator: bool,
}

/// Inspect only opening tag names and their attributes. This does not import
/// values: cell text, JSON attributes, comments and raw text cannot add markers.
fn markers(html: &str) -> Markers {
    let bytes = html.as_bytes();
    let mut markers = Markers::default();
    let mut index = 0;
    while let Some(offset) = bytes[index..].iter().position(|&byte| byte == b'<') {
        index += offset;
        if bytes[index..].starts_with(b"<!--") {
            let Some(end) = bytes[index + 4..]
                .windows(3)
                .position(|part| part == b"-->")
            else {
                break;
            };
            index += 4 + end + 3;
            continue;
        }
        let start = index + 1;
        if !bytes.get(start).is_some_and(u8::is_ascii_alphabetic) {
            // Skip closing tags and declarations as a unit, including quotes.
            if bytes
                .get(start)
                .is_some_and(|byte| matches!(byte, b'/' | b'!' | b'?'))
            {
                let Some(end) = tag_end(bytes, start) else {
                    break;
                };
                index = end + 1;
            } else {
                index = start;
            }
            continue;
        }
        let mut name_end = start;
        while bytes
            .get(name_end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b':' | b'_'))
        {
            name_end += 1;
        }
        let Some(end) = tag_end(bytes, name_end) else {
            break;
        };
        if !bytes[name_end].is_ascii_whitespace() && !matches!(bytes[name_end], b'/' | b'>') {
            index = end + 1;
            continue;
        }
        let name = &bytes[start..name_end];
        let attributes = &bytes[name_end..end];
        if name.eq_ignore_ascii_case(b"google-sheets-html-origin") {
            markers.sheets = true;
        } else if name.eq_ignore_ascii_case(b"table") {
            markers.dosty |= attribute_value(attributes, b"data-dosty")
                .is_some_and(|value| value.eq_ignore_ascii_case(b"table"));
        } else if name.eq_ignore_ascii_case(b"td") || name.eq_ignore_ascii_case(b"th") {
            markers.sheets |= attribute_value(attributes, b"data-sheets-value").is_some();
        } else if name.eq_ignore_ascii_case(b"meta") {
            let kind = attribute_value(attributes, b"name");
            let content = attribute_value(attributes, b"content");
            if kind.is_some_and(|kind| kind.eq_ignore_ascii_case(b"ProgId")) {
                markers.excel_progid |=
                    content.is_some_and(|content| content.eq_ignore_ascii_case(b"Excel.Sheet"));
            } else if kind.is_some_and(|kind| kind.eq_ignore_ascii_case(b"Generator")) {
                markers.excel_generator |= content.is_some_and(|content| {
                    content
                        .get(..16)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"Microsoft Excel "))
                        && content.get(16).is_some_and(u8::is_ascii_digit)
                });
            }
        }
        index = end + 1;
        if [
            b"script".as_slice(),
            b"style",
            b"textarea",
            b"title",
            b"xmp",
            b"iframe",
            b"noembed",
            b"noframes",
        ]
        .iter()
        .any(|raw| name.eq_ignore_ascii_case(raw))
        {
            // These elements treat apparent tags in their body as text.
            index = raw_text_end(bytes, index, name);
        } else if name.eq_ignore_ascii_case(b"plaintext") {
            break;
        }
    }
    markers
}

fn tag_end(bytes: &[u8], mut index: usize) -> Option<usize> {
    let mut quote = None;
    while let Some(&byte) = bytes.get(index) {
        if quote == Some(byte) {
            quote = None;
        } else if quote.is_none() {
            if matches!(byte, b'\'' | b'"') {
                quote = Some(byte);
            } else if byte == b'>' {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}

fn attribute_value<'a>(attributes: &'a [u8], wanted: &[u8]) -> Option<&'a [u8]> {
    let mut index = 0;
    while index < attributes.len() {
        while attributes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'/')
        {
            index += 1;
        }
        let start = index;
        while attributes
            .get(index)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && !matches!(byte, b'=' | b'/'))
        {
            index += 1;
        }
        let name = &attributes[start..index];
        while attributes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if attributes.get(index) != Some(&b'=') {
            continue;
        }
        index += 1;
        while attributes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        let quote = attributes
            .get(index)
            .copied()
            .filter(|byte| matches!(byte, b'\'' | b'"'));
        index += usize::from(quote.is_some());
        let value_start = index;
        while attributes.get(index).is_some_and(|byte| {
            quote.map_or_else(|| !byte.is_ascii_whitespace(), |quote| *byte != quote)
        }) {
            index += 1;
        }
        let value = &attributes[value_start..index];
        index += usize::from(quote.is_some() && index < attributes.len());
        if name.eq_ignore_ascii_case(wanted) {
            return Some(value);
        }
    }
    None
}

fn raw_text_end(bytes: &[u8], mut index: usize, name: &[u8]) -> usize {
    while let Some(offset) = bytes[index..].iter().position(|&byte| byte == b'<') {
        index += offset;
        let end = index + 2 + name.len();
        if bytes[index..].starts_with(b"</")
            && bytes
                .get(index + 2..end)
                .is_some_and(|closing| closing.eq_ignore_ascii_case(name))
            && bytes
                .get(end)
                .is_some_and(|byte| byte.is_ascii_whitespace() || matches!(byte, b'>' | b'/'))
        {
            return tag_end(bytes, end).map_or(bytes.len(), |end| end + 1);
        }
        index += 1;
    }
    bytes.len()
}

/// macOS/browser clipboard flavors can disagree on CRLF versus LF. Only that
/// spelling is equivalent: spaces, tabs, bare CR and record counts still match.
fn same_text(left: &str, right: &str) -> bool {
    fn lf_bytes(text: &str) -> impl Iterator<Item = u8> + '_ {
        let mut bytes = text.bytes().peekable();
        std::iter::from_fn(move || {
            let byte = bytes.next()?;
            if byte == b'\r' && bytes.peek() == Some(&b'\n') {
                bytes.next();
                Some(b'\n')
            } else {
                Some(byte)
            }
        })
    }
    left == right || lf_bytes(left).eq(lf_bytes(right))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn metadata_requires_matching_text_around_the_html_snapshot() {
        let expected = "café\t🧑‍💻\r\nsecond\trow";
        let lf = expected.replace("\r\n", "\n");
        for html in [
            "<google-sheets-html-origin><table></table></google-sheets-html-origin>",
            "<table><td data-sheets-value='{\"1\":2}'></td></table>",
            "<table><TH DATA-SHEETS-VALUE='{}'></TH></table>",
        ] {
            assert_eq!(producer(expected, expected, html, &lf), Producer::Sheets);
            assert_eq!(producer(expected, "old", html, &lf), Producer::Standard);
            assert_eq!(producer(expected, &lf, html, "new"), Producer::Standard);
        }
        for html in [
            "<table><td>ordinary HTML</td></table>",
            "<table data-dosty='table' data-sheets-value='{}'></table>",
            "<table DATA-DOSTY='table'><google-sheets-html-origin></table>",
            "",
        ] {
            assert_eq!(producer(expected, &lf, html, expected), Producer::Standard);
        }
        let oversized = format!("data-sheets-value{}", " ".repeat(MAX_SOURCE_BYTES));
        assert_eq!(
            producer(expected, expected, &oversized, expected),
            Producer::Standard
        );
    }

    #[test]
    fn text_equivalence_preserves_records_whitespace_and_literal_carriage_returns() {
        assert!(same_text("one\r\ntwo\r\n", "one\ntwo\n"));
        assert!(same_text("café 🧑‍💻\t", "café 🧑‍💻\t"));
        for (left, right) in [
            ("one\r", "one\n"),
            ("one\n", "one"),
            ("one\n\n", "one\n"),
            ("one\t", "one "),
            (" one", "one"),
            ("one\r\n", "one\r"),
        ] {
            assert!(!same_text(left, right), "{left:?} versus {right:?}");
        }
    }

    #[test]
    fn producer_markers_require_opening_tags_and_attributes_not_cell_contents() {
        let text = "\"literal\"\tdata-dosty";
        for html in [
            r#"<google-sheets-html-origin><table><td data-sheets-value='{"1":2,"2":"data-dosty"}'>data-dosty</td></table></google-sheets-html-origin>"#,
            r#"<table title='data-dosty="table"'><td data-sheets-value='{}'>data-dosty</td></table>"#,
            r#"<TABLE data-dosty='another-producer'><TH DATA-SHEETS-VALUE = '{}'>data-dosty</TH></TABLE>"#,
            r#"<table><td data-dosty='table' data-sheets-value={}>value</td></table>"#,
            r#"<!-- <table data-dosty="table"> --><GOOGLE-SHEETS-HTML-ORIGIN >value</GOOGLE-SHEETS-HTML-ORIGIN>"#,
        ] {
            assert_eq!(producer(text, text, html, text), Producer::Sheets, "{html}");
        }
        for html in [
            r#"<p>data-sheets-value google-sheets-html-origin</p>"#,
            r#"<p title="<td data-sheets-value='{}'>">ordinary</p>"#,
            r#"<td title='data-sheets-value="{}"'>ordinary</td>"#,
            r#"<!-- <google-sheets-html-origin><td data-sheets-value='{}'> -->"#,
            r#"<script>const value = '<td data-sheets-value="{}">';</script>"#,
            r#"<style>/* <google-sheets-html-origin> */</style>"#,
            r#"<textarea><td data-sheets-value='{}'></textarea>"#,
            r#"<div data-sheets-value='{}'>ordinary</div>"#,
            r#"</google-sheets-html-origin><google-sheets-html-origin-extra>"#,
            r#"<google-sheets-html-origin! data-sheets-value='{}'>"#,
            r#"<table data-dosty='table'><td data-sheets-value='{}'></td></table>"#,
            r#"<google-sheets-html-origin><TABLE DATA-DOSTY = "TABLE" >value</TABLE></google-sheets-html-origin>"#,
            r#"<google-sheets-html-origin><table data-dosty = table >value</table></google-sheets-html-origin>"#,
        ] {
            assert_eq!(
                producer(text, text, html, text),
                Producer::Standard,
                "{html}"
            );
        }
    }

    #[test]
    fn sheets_snapshots_use_html_cell_boundaries_and_reject_changed_or_unsafe_metadata() {
        // Captured from adjacent Sheets cells `"open` and `close"`. A single
        // tab-containing cell has identical native plain text.
        let text = include_str!("../../tests/fixtures/google-sheets-ambiguous.tsv");
        assert_eq!(text, "\"open\tclose\"");
        let split = r#"<google-sheets-html-origin><table><tbody><tr><td data-sheets-value='{"1":2,"2":"\"open"}'>"open</td><td data-sheets-value='{"1":2,"2":"close\""}'>close"</td></tr></tbody></table></google-sheets-html-origin>"#;
        let joined = r#"<google-sheets-html-origin><table><tbody><tr><td data-sheets-value='{"1":2,"2":"open\tclose"}'>open&#9;close</td></tr></tbody></table></google-sheets-html-origin>"#;
        for (html, values) in [
            (split, vec![vec!["\"open".into(), "close\"".into()]]),
            (joined, vec![vec!["open\tclose".into()]]),
        ] {
            assert_eq!(
                snapshot(text, text, html, text).unwrap(),
                Snapshot {
                    producer: Producer::Sheets,
                    values: Some(values),
                }
            );
            assert!(snapshot(text, "before changed", html, text).is_err());
            assert!(snapshot(text, text, html, "after changed").is_err());
        }
        for html in [
            "<google-sheets-html-origin></google-sheets-html-origin>",
            "<google-sheets-html-origin><table><tr><td colspan='2'>merged</td></tr></table></google-sheets-html-origin>",
        ] {
            assert!(snapshot(text, text, html, text).is_err());
        }
        let ordinary = "<p>data-sheets-value google-sheets-html-origin data-dosty</p>";
        assert_eq!(
            snapshot(text, text, ordinary, text).unwrap(),
            Snapshot::standard()
        );
        let own = crate::document::table_clipboard::encode_html(&[vec!["value".into()]]).unwrap();
        assert_eq!(
            snapshot("value", "value", &own, "value").unwrap(),
            Snapshot::standard()
        );
    }

    #[test]
    fn excel_markers_require_both_actual_native_meta_tags() {
        let text = "\"open\tclose\"";
        let native = "<meta name='ProgId' content='Excel.Sheet'><meta name='Generator' content='Microsoft Excel 15'>";
        assert_eq!(producer(text, text, native, text), Producer::Excel);
        let uppercase = "<META CONTENT='EXCEL.SHEET' NAME='PROGID'><META CONTENT='MICROSOFT EXCEL 15' NAME='GENERATOR'>";
        assert_eq!(producer(text, text, uppercase, text), Producer::Excel);
        for html in [
            "<meta name='ProgId' content='Excel.Sheet'>",
            "<meta name='Generator' content='Microsoft Excel 15'>",
            "<meta name='ProgId' content='Excel.Sheet'><meta name='Generator' content='Microsoft Excelish 15'>",
            "<p>ProgId Excel.Sheet Generator Microsoft Excel 15</p>",
            "<p title=\"<meta name='ProgId' content='Excel.Sheet'><meta name='Generator' content='Microsoft Excel 15'>\">ordinary</p>",
        ] {
            assert_eq!(
                producer(text, text, html, text),
                Producer::Standard,
                "{html}"
            );
        }
        for html in [
            format!("<!-- {native} -->"),
            format!("<script>{native}</script>"),
            format!("<textarea>{native}</textarea>"),
            format!("{native}<table data-dosty='table'></table>"),
        ] {
            assert_eq!(
                producer(text, text, &html, text),
                Producer::Standard,
                "{html}"
            );
        }
        assert_eq!(producer(text, "old", native, text), Producer::Standard);
        assert_eq!(producer(text, text, native, "new"), Producer::Standard);
    }

    #[test]
    fn excel_snapshots_use_visible_cells_and_reject_changed_or_unsafe_html() {
        let text =
            "plain\t00123\t12345678901234567890\t2026-11-03\nwhite\t  spaced  \tliteral\tend";
        let native = include_str!("../../tests/fixtures/excel-web-clipboard.html");
        let read = snapshot(text, text, native, text).unwrap();
        assert_eq!(read.producer, Producer::Excel);
        assert_eq!(
            read.values.unwrap(),
            vec![
                vec!["plain", "00123", "12345678901234567890", "2026-11-03"],
                vec!["white", "  spaced  ", "literal", "end"],
            ]
        );
        assert!(snapshot(text, "old", native, text).is_err());
        assert!(snapshot(text, text, native, "new").is_err());
        let meta = "<meta name=ProgId content=Excel.Sheet><meta name=Generator content='Microsoft Excel 15'>";
        for table in [
            "<table><tr><td colspan=2>merged</td></tr></table>",
            "<table><tr><td>unfinished",
            "<table><tr><td hidden>hidden</td></tr></table>",
            "",
        ] {
            assert!(snapshot(text, text, &format!("{meta}{table}"), text).is_err());
        }
        let split = format!("{meta}<table><tr><td>\"open</td><td>close\"</td></tr></table>");
        assert_eq!(
            snapshot(
                "\"open\tclose\"",
                "\"open\tclose\"",
                &split,
                "\"open\tclose\""
            )
            .unwrap()
            .values,
            Some(vec![vec!["\"open".into(), "close\"".into()]])
        );
    }

    #[test]
    fn textless_native_blank_cells_require_a_stable_recognized_empty_rectangle() {
        let excel = "<meta name=ProgId content=Excel.Sheet><meta name=Generator content='Microsoft Excel 15'>";
        for (html, producer, values) in [
            (
                format!("{excel}<table><tr><td></td></tr></table>"),
                Producer::Excel,
                vec![vec![String::new()]],
            ),
            (
                format!("{excel}<table><tr><td></td></tr><tr><td></td></tr></table>"),
                Producer::Excel,
                vec![vec![String::new()], vec![String::new()]],
            ),
            (
                "<google-sheets-html-origin><table><tr><td data-sheets-value='{\"1\":2,\"2\":\"\"}'></td></tr></table></google-sheets-html-origin>".into(),
                Producer::Sheets,
                vec![vec![String::new()]],
            ),
        ] {
            let mut reads = [
                Ok((None, Some(html.clone()))),
                Ok((Some(String::new()), Some(html))),
            ]
            .into_iter();
            assert_eq!(
                read_empty_with(|| reads.next().unwrap()).unwrap(),
                Some(Snapshot {
                    producer,
                    values: Some(values),
                })
            );
            assert!(reads.next().is_none());
        }
    }

    #[test]
    fn unavailable_or_nonempty_clipboards_do_not_infer_blank_cells() {
        let excel = "<meta name=ProgId content=Excel.Sheet><meta name=Generator content='Microsoft Excel 15'>";
        for (text, html) in [
            (None, None),
            (Some(String::new()), None),
            (Some("value".into()), None),
            (
                Some(" ".into()),
                Some(format!("{excel}<table><tr><td></td></tr></table>")),
            ),
            (None, Some("<table><tr><td></td></tr></table>".into())),
            (
                None,
                Some(format!("{excel}<table><tr><td>value</td></tr></table>")),
            ),
            (
                None,
                Some(format!("{excel}<table><tr><td> </td></tr></table>")),
            ),
            (
                None,
                Some(format!(
                    "{excel}<table><tr><td colspan=2></td></tr></table>"
                )),
            ),
            (None, Some(format!("{excel}<table><tr><td>"))),
            (None, Some(" ".repeat(MAX_SOURCE_BYTES + 1))),
        ] {
            let mut reads = [Ok((text, html))].into_iter();
            assert_eq!(read_empty_with(|| reads.next().unwrap()).unwrap(), None);
            assert!(reads.next().is_none());
        }
    }

    #[test]
    fn empty_clipboard_confirmation_rejects_changes_and_preserves_real_read_errors() {
        let blank = "<meta name=ProgId content=Excel.Sheet><meta name=Generator content='Microsoft Excel 15'><table><tr><td></td></tr></table>";
        for after in [
            Ok((Some("changed".into()), Some(blank.into()))),
            Ok((None, Some(format!("{blank} ")))),
            Ok((None, None)),
            Err("read failed".into()),
        ] {
            let mut reads = [Ok((None, Some(blank.into()))), after].into_iter();
            assert!(read_empty_with(|| reads.next().unwrap()).is_err());
            assert!(reads.next().is_none());
        }
        assert_eq!(
            read_empty_with(|| Err("read failed".into())),
            Err("read failed".into())
        );
        assert_eq!(
            optional_content(Err(arboard::Error::ContentNotAvailable)),
            Ok(None)
        );
        assert_eq!(optional_content(Ok(String::new())), Ok(Some(String::new())));
        assert!(optional_content(Err(arboard::Error::ClipboardNotSupported)).is_err());
    }

    #[test]
    fn lazy_owner_survives_operations_and_failed_initialization_can_retry() {
        struct Owner(Arc<AtomicUsize>);
        impl Drop for Owner {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let dropped = Arc::new(AtomicUsize::new(0));
        let owner = Mutex::new(None::<Owner>);
        let failed = with_owner(&owner, || Err("unavailable".into()), |_| Ok(()));
        assert_eq!(failed, Err("unavailable".into()));
        assert!(owner.lock().unwrap().is_none());
        with_owner(&owner, || Ok(Owner(Arc::clone(&dropped))), |_| Ok(())).unwrap();
        let operation: Result<(), String> = with_owner(
            &owner,
            || panic!("an existing native owner must be reused"),
            |_| Err("read failed".into()),
        );
        assert_eq!(operation, Err("read failed".into()));
        with_owner(
            &owner,
            || panic!("a failed operation must retain clipboard ownership"),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(dropped.load(Ordering::SeqCst), 0);
        drop(owner);
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }
}

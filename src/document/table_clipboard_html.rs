//! Bounded values-only reading of native spreadsheet clipboard tables. HTML is
//! scanned as inert text: no rendering, styles, links, images or scripts load.
//! Explicit cell boundaries avoid ambiguities in Sheets' plain-text quoting.

use pulldown_cmark::{Event, Parser};
use serde::Deserialize;

use super::collaboration::{MAX_SOURCE_BYTES, MAX_TRANSACTION_EDITS};

const INVALID: &str = "This spreadsheet's HTML table cannot be pasted safely.";
const TOO_LARGE: &str = "The spreadsheet clipboard exceeds the text or cell limit.";
const MAX_DEPTH: usize = 128;
const MAX_ATTRIBUTES: usize = 128;

#[derive(Deserialize)]
struct SheetsValue {
    #[serde(rename = "1")]
    kind: u8,
    #[serde(rename = "2", default)]
    text: Option<String>,
}

struct Cell {
    string: Option<String>,
    visible: String,
    skip_line_ending: bool,
    spacerun_depth: Option<usize>,
}

/// Read one explicit unmerged table. Type-2 metadata is the authoritative
/// literal string; other cell types retain their displayed text, not formulas
/// or underlying number serials. Ragged rows are padded only within the bound.
pub(crate) fn decode(html: &str) -> Result<Vec<Vec<String>>, &'static str> {
    decode_table(html, false)
}

/// Excel emits a physical line ending after each semantic `<br>`. That source
/// formatting is not another cell break. Other cell whitespace remains literal.
/// Excel cells use displayed text rather than formulas or underlying serials.
pub(crate) fn decode_excel(html: &str) -> Result<Vec<Vec<String>>, &'static str> {
    decode_table(html, true)
}

fn decode_table(html: &str, excel: bool) -> Result<Vec<Vec<String>>, &'static str> {
    if html.len() > MAX_SOURCE_BYTES {
        return Err(TOO_LARGE);
    }
    let mut scanner = Scanner { html, offset: 0 };
    let mut stack: Vec<&str> = Vec::new();
    let mut outside_depth = 0usize;
    let mut found = false;
    let mut finished = false;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Option<Vec<String>> = None;
    let mut cell: Option<Cell> = None;
    let mut width = 0usize;
    let mut bytes = 0usize;
    while let Some(token) = scanner.next()? {
        let tag = match token {
            Token::Text(text) => {
                if let Some(cell) = &mut cell {
                    if cell.string.is_none() {
                        let text = if cell.skip_line_ending {
                            cell.skip_line_ending = false;
                            text.strip_prefix("\r\n")
                                .or_else(|| text.strip_prefix('\n'))
                                .or_else(|| text.strip_prefix('\r'))
                                .unwrap_or(text)
                        } else {
                            text
                        };
                        let mut text = entities(text)?;
                        if cell.spacerun_depth.is_some() {
                            // Excel encodes ordinary spaces in its explicit
                            // spacerun spans with nonbreaking-space entities.
                            text = text.replace('\u{a0}', " ");
                        }
                        check_bytes(bytes, cell.visible.len(), text.len())?;
                        cell.visible.push_str(&text);
                    }
                } else if !stack.is_empty() && !text.trim().is_empty() {
                    return Err(INVALID);
                }
                continue;
            }
            Token::Tag(tag) => tag,
        };
        let name = tag.name;
        if stack.is_empty() {
            if ["plaintext", "template", "noscript"]
                .iter()
                .any(|unsupported| name.eq_ignore_ascii_case(unsupported))
            {
                return Err(INVALID);
            }
            if name.eq_ignore_ascii_case("table") {
                if tag.closing || tag.empty || found {
                    return Err(INVALID);
                }
                reject_hidden(&attributes(tag.attributes)?)?;
                found = true;
                stack.push(name);
            } else if !tag.closing && raw_element(name) {
                scanner.skip_raw(name)?;
            } else if tag.closing {
                outside_depth = outside_depth.saturating_sub(1);
            } else if !tag.empty && !void_element(name) {
                outside_depth += 1;
                if outside_depth >= MAX_DEPTH {
                    return Err(TOO_LARGE);
                }
            }
            continue;
        }
        if tag.closing {
            if !stack
                .last()
                .is_some_and(|open| open.eq_ignore_ascii_case(name))
            {
                return Err(INVALID);
            }
            stack.pop();
            if let Some(cell) = &mut cell
                && cell.spacerun_depth.is_some_and(|depth| stack.len() < depth)
            {
                cell.spacerun_depth = None;
            }
            if name.eq_ignore_ascii_case("td") || name.eq_ignore_ascii_case("th") {
                let cell = cell.take().ok_or(INVALID)?;
                let value = cell.string.unwrap_or(cell.visible);
                bytes = check_bytes(bytes, value.len(), 0)?;
                let row = row.as_mut().ok_or(INVALID)?;
                check_cells(rows.len() + 1, width.max(row.len() + 1))?;
                row.push(value);
            } else if name.eq_ignore_ascii_case("tr") {
                let row = row.take().ok_or(INVALID)?;
                if row.is_empty() || cell.is_some() {
                    return Err(INVALID);
                }
                width = width.max(row.len());
                check_cells(rows.len() + 1, width)?;
                rows.push(row);
            } else if name.eq_ignore_ascii_case("table") {
                if !stack.is_empty() || row.is_some() || cell.is_some() || rows.is_empty() {
                    return Err(INVALID);
                }
                finished = true;
            }
            continue;
        }
        let attrs = attributes(tag.attributes)?;
        reject_hidden(&attrs)?;
        if tag.empty && !void_element(name) {
            return Err(INVALID);
        }
        if name.eq_ignore_ascii_case("table") {
            return Err("Nested spreadsheet tables cannot be pasted safely.");
        }
        let parent = *stack.last().ok_or(INVALID)?;
        let push = if name.eq_ignore_ascii_case("tr") {
            if row.is_some() || cell.is_some() || !row_parent(parent) {
                return Err(INVALID);
            }
            row = Some(Vec::new());
            true
        } else if name.eq_ignore_ascii_case("td") || name.eq_ignore_ascii_case("th") {
            if !parent.eq_ignore_ascii_case("tr") || cell.is_some() {
                return Err(INVALID);
            }
            let string = cell_string(&attrs, !excel)?;
            if let Some(value) = &string {
                check_bytes(bytes, value.len(), 0)?;
            }
            cell = Some(Cell {
                string,
                visible: String::new(),
                skip_line_ending: false,
                spacerun_depth: None,
            });
            true
        } else if ["thead", "tbody", "tfoot", "colgroup"]
            .iter()
            .any(|section| name.eq_ignore_ascii_case(section))
        {
            if !parent.eq_ignore_ascii_case("table") {
                return Err(INVALID);
            }
            true
        } else if name.eq_ignore_ascii_case("col") {
            if !parent.eq_ignore_ascii_case("colgroup") && !parent.eq_ignore_ascii_case("table") {
                return Err(INVALID);
            }
            false
        } else if name.eq_ignore_ascii_case("br") {
            let cell = cell.as_mut().ok_or(INVALID)?;
            if cell.string.is_none() {
                check_bytes(bytes, cell.visible.len(), 1)?;
                cell.visible.push('\n');
                cell.skip_line_ending = excel;
            }
            false
        } else if name.eq_ignore_ascii_case("wbr") {
            if cell.is_none() {
                return Err(INVALID);
            }
            false
        } else if inline_element(name) {
            let cell = cell.as_mut().ok_or(INVALID)?;
            if excel
                && name.eq_ignore_ascii_case("span")
                && cell.spacerun_depth.is_none()
                && excel_spacerun(&attrs)?
            {
                cell.spacerun_depth = Some(stack.len() + 1);
            }
            true
        } else {
            return Err("This spreadsheet contains unsupported cell markup.");
        };
        if push {
            if outside_depth + stack.len() >= MAX_DEPTH {
                return Err(TOO_LARGE);
            }
            stack.push(name);
        }
    }
    if !found || !finished || !stack.is_empty() || row.is_some() || cell.is_some() {
        return Err(INVALID);
    }
    check_cells(rows.len(), width)?;
    for row in &mut rows {
        row.resize_with(width, String::new);
    }
    Ok(rows)
}

fn check_bytes(left: usize, middle: usize, right: usize) -> Result<usize, &'static str> {
    left.checked_add(middle)
        .and_then(|sum| sum.checked_add(right))
        .filter(|&sum| sum <= MAX_SOURCE_BYTES)
        .ok_or(TOO_LARGE)
}

fn check_cells(rows: usize, columns: usize) -> Result<(), &'static str> {
    rows.checked_mul(columns)
        .filter(|&cells| cells <= MAX_TRANSACTION_EDITS)
        .map(|_| ())
        .ok_or(TOO_LARGE)
}

type Attributes<'a> = Vec<(&'a str, Option<&'a str>)>;

fn cell_string(attrs: &Attributes<'_>, sheets: bool) -> Result<Option<String>, &'static str> {
    let mut string = None;
    for &(name, value) in attrs {
        if name.eq_ignore_ascii_case("rowspan") || name.eq_ignore_ascii_case("colspan") {
            if entities(value.ok_or(INVALID)?)?.trim().parse::<usize>() != Ok(1) {
                return Err("Merged spreadsheet cells cannot be pasted safely.");
            }
        } else if sheets && name.eq_ignore_ascii_case("data-sheets-value") {
            let decoded = entities(value.ok_or(INVALID)?)?;
            let value: SheetsValue = serde_json::from_str(&decoded).map_err(|_| INVALID)?;
            if value.kind == 2 {
                string = Some(value.text.ok_or(INVALID)?);
            }
        }
    }
    Ok(string)
}

fn reject_hidden(attrs: &Attributes<'_>) -> Result<(), &'static str> {
    for &(name, value) in attrs {
        if name.eq_ignore_ascii_case("hidden") {
            return Err(INVALID);
        }
        if name.eq_ignore_ascii_case("style")
            && let Some(style) = value
        {
            let compact: String = entities(style)?
                .chars()
                .filter(|character| !character.is_ascii_whitespace())
                .flat_map(char::to_lowercase)
                .collect();
            if compact.contains("display:none") || compact.contains("visibility:hidden") {
                return Err(INVALID);
            }
        }
    }
    Ok(())
}

fn excel_spacerun(attrs: &Attributes<'_>) -> Result<bool, &'static str> {
    for &(name, value) in attrs {
        if name.eq_ignore_ascii_case("style") {
            let style = entities(value.ok_or(INVALID)?)?;
            return Ok(style
                .split(';')
                .rev()
                .filter_map(|property| property.split_once(':'))
                .find(|(name, _)| name.trim().eq_ignore_ascii_case("mso-spacerun"))
                .is_some_and(|(_, value)| value.trim().eq_ignore_ascii_case("yes")));
        }
    }
    Ok(false)
}

fn row_parent(name: &str) -> bool {
    ["table", "thead", "tbody", "tfoot"]
        .iter()
        .any(|allowed| name.eq_ignore_ascii_case(allowed))
}

fn inline_element(name: &str) -> bool {
    [
        "span", "a", "b", "strong", "i", "em", "u", "s", "strike", "sub", "sup", "font", "code",
    ]
    .iter()
    .any(|allowed| name.eq_ignore_ascii_case(allowed))
}

fn void_element(name: &str) -> bool {
    [
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr",
    ]
    .iter()
    .any(|allowed| name.eq_ignore_ascii_case(allowed))
}

fn raw_element(name: &str) -> bool {
    [
        "script", "style", "textarea", "title", "xmp", "iframe", "noembed", "noframes",
    ]
    .iter()
    .any(|allowed| name.eq_ignore_ascii_case(allowed))
}

struct Tag<'a> {
    name: &'a str,
    attributes: &'a str,
    closing: bool,
    empty: bool,
}

enum Token<'a> {
    Text(&'a str),
    Tag(Tag<'a>),
}

struct Scanner<'a> {
    html: &'a str,
    offset: usize,
}

impl<'a> Scanner<'a> {
    fn next(&mut self) -> Result<Option<Token<'a>>, &'static str> {
        let bytes = self.html.as_bytes();
        while self.offset < bytes.len() {
            let start = self.offset;
            if bytes[start] != b'<' {
                let end = self.html[start..]
                    .find('<')
                    .map_or(bytes.len(), |n| start + n);
                self.offset = end;
                return Ok(Some(Token::Text(&self.html[start..end])));
            }
            if bytes[start..].starts_with(b"<!--") {
                let end = self.html[start + 4..].find("-->").ok_or(INVALID)?;
                self.offset = start + 4 + end + 3;
                continue;
            }
            let end = tag_end(bytes, start + 1).ok_or(INVALID)?;
            self.offset = end + 1;
            if matches!(bytes.get(start + 1), Some(b'!' | b'?')) {
                continue;
            }
            let closing = bytes.get(start + 1) == Some(&b'/');
            let name_start = start + 1 + usize::from(closing);
            let mut name_end = name_start;
            if !bytes.get(name_start).is_some_and(u8::is_ascii_alphabetic) {
                return Err(INVALID);
            }
            while bytes.get(name_end).is_some_and(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b':' | b'_')
            }) {
                name_end += 1;
            }
            if name_end > end
                || !bytes[name_end].is_ascii_whitespace() && !matches!(bytes[name_end], b'/' | b'>')
            {
                return Err(INVALID);
            }
            let mut attrs = self.html[name_end..end].trim();
            let empty = attrs.ends_with('/');
            if empty {
                attrs = attrs[..attrs.len() - 1].trim_end();
            }
            if closing && (!attrs.is_empty() || empty) {
                return Err(INVALID);
            }
            return Ok(Some(Token::Tag(Tag {
                name: &self.html[name_start..name_end],
                attributes: attrs,
                closing,
                empty,
            })));
        }
        Ok(None)
    }

    fn skip_raw(&mut self, name: &str) -> Result<(), &'static str> {
        let bytes = self.html.as_bytes();
        while let Some(next) = self.html[self.offset..].find('<') {
            let start = self.offset + next;
            let name_end = start + 2 + name.len();
            if bytes[start..].starts_with(b"</")
                && bytes
                    .get(start + 2..name_end)
                    .is_some_and(|close| close.eq_ignore_ascii_case(name.as_bytes()))
                && bytes
                    .get(name_end)
                    .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'>')
            {
                self.offset = tag_end(bytes, name_end).ok_or(INVALID)? + 1;
                return Ok(());
            }
            self.offset = start + 1;
        }
        Err(INVALID)
    }
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
            } else if byte == b'<' {
                return None;
            }
        }
        index += 1;
    }
    None
}

fn attributes(raw: &str) -> Result<Attributes<'_>, &'static str> {
    let bytes = raw.as_bytes();
    let mut index = 0;
    let mut attrs: Attributes<'_> = Vec::new();
    while index < bytes.len() {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if index == bytes.len() {
            break;
        }
        let start = index;
        while bytes.get(index).is_some_and(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b':' | b'_' | b'.')
        }) {
            index += 1;
        }
        if start == index || attrs.len() >= MAX_ATTRIBUTES {
            return Err(INVALID);
        }
        let name = &raw[start..index];
        if attrs
            .iter()
            .any(|(seen, _)| seen.eq_ignore_ascii_case(name))
        {
            return Err(INVALID);
        }
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        let value = if bytes.get(index) == Some(&b'=') {
            index += 1;
            while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
                index += 1;
            }
            let quote = bytes
                .get(index)
                .copied()
                .filter(|byte| matches!(byte, b'\'' | b'"'));
            index += usize::from(quote.is_some());
            let start = index;
            while bytes.get(index).is_some_and(|byte| {
                quote.map_or_else(|| !byte.is_ascii_whitespace(), |quote| *byte != quote)
            }) {
                if quote.is_none()
                    && matches!(bytes[index], b'<' | b'>' | b'\'' | b'"' | b'=' | 0x60)
                {
                    return Err(INVALID);
                }
                index += 1;
            }
            if quote.is_some() && bytes.get(index) != quote.as_ref() {
                return Err(INVALID);
            }
            let value = &raw[start..index];
            if quote.is_none() && value.is_empty() {
                return Err(INVALID);
            }
            index += usize::from(quote.is_some());
            if bytes
                .get(index)
                .is_some_and(|byte| !byte.is_ascii_whitespace())
            {
                return Err(INVALID);
            }
            Some(value)
        } else {
            None
        };
        attrs.push((name, value));
    }
    Ok(attrs)
}

/// Decode only entity-shaped tokens through the editor's Markdown parser.
/// Sentinels prevent block interpretation; surrounding literal punctuation and
/// whitespace never enter that parser and therefore remain byte-for-byte text.
fn entities(text: &str) -> Result<String, &'static str> {
    let mut output = String::new();
    let mut index = 0;
    while let Some(next) = text[index..].find('&') {
        let amp = index + next;
        check_bytes(output.len(), amp - index, 0)?;
        output.push_str(&text[index..amp]);
        let limit = (amp + 65).min(text.len());
        let candidate = text.as_bytes()[amp + 1..limit]
            .iter()
            .position(|&byte| byte == b';')
            .map(|end| amp + 1 + end + 1)
            .filter(|&end| {
                end > amp + 2
                    && text.as_bytes()[amp + 1..end - 1]
                        .iter()
                        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'#')
            });
        if let Some(end) = candidate {
            let wrapped = format!("x{}x", &text[amp..end]);
            let parsed: String = Parser::new(&wrapped)
                .filter_map(|event| match event {
                    Event::Text(text) => Some(text.into_string()),
                    _ => None,
                })
                .collect();
            let decoded = parsed
                .strip_prefix('x')
                .and_then(|text| text.strip_suffix('x'))
                .ok_or(INVALID)?;
            check_bytes(output.len(), decoded.len(), 0)?;
            output.push_str(decoded);
            index = end;
        } else {
            check_bytes(output.len(), 1, 0)?;
            output.push('&');
            index = amp + 1;
        }
    }
    check_bytes(output.len(), text.len() - index, 0)?;
    output.push_str(&text[index..]);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(body: &str) -> String {
        format!(
            "<google-sheets-html-origin><table><tbody>{body}</tbody></table></google-sheets-html-origin>"
        )
    }

    #[test]
    fn explicit_cells_resolve_the_native_quote_fragment_ambiguity() {
        let plain = include_str!("../../tests/fixtures/google-sheets-ambiguous.tsv");
        assert_eq!(plain, "\"open\tclose\"");
        let split = table(
            r#"<tr><td data-sheets-value='{"1":2,"2":"\"open"}'>"open</td><td data-sheets-value='{"1":2,"2":"close\""}'>close"</td></tr>"#,
        );
        let joined =
            table(r#"<tr><td data-sheets-value='{"1":2,"2":"open\tclose"}'>open close</td></tr>"#);
        assert_eq!(decode(&split).unwrap(), vec![vec!["\"open", "close\""]]);
        assert_eq!(decode(&joined).unwrap(), vec![vec!["open\tclose"]]);
    }

    #[test]
    fn numeric_and_formula_metadata_preserves_displayed_values() {
        let html = table(
            r#"<tr><td data-sheets-value='{"1":3,"3":1234.5}'><span>$1,234</span><a href="https://example.invalid/">.50</a></td><td data-sheets-value='{"1":3,"3":0.125}'>12.50%</td><td data-sheets-value='{"1":3,"3":2}' data-sheets-formula="=1+1"><b>2</b></td><td data-sheets-value='{"1":2,"2":"=1+1"}'>2</td></tr>"#,
        );
        let values = decode(&html).unwrap();
        assert_eq!(values, vec![vec!["$1,234.50", "12.50%", "2", "=1+1"]]);
        let capture =
            include_str!("../../docs/validation/table-clipboard-native/formatted-values.tsv");
        assert_eq!(values[0][..2].join("\t"), capture);
    }

    #[test]
    fn entities_unicode_literal_punctuation_and_cell_breaks_remain_values() {
        let html = table(
            r#"<tr><th data-sheets-value="{&quot;1&quot;:2,&quot;2&quot;:&quot;00123 &amp; \&quot;quote\&quot;\t日本語&quot;}">ignored</th><td><span>  *[x] # \ &lt;tag&gt;&amp;&quot;&#39;&nbsp;</span><a href="javascript:ignored()">e&#x301; 🧑‍💻</a><br>next<br/>last &NotEqualTilde; &unknown;</td></tr><tr><td></td></tr>"#,
        );
        assert_eq!(
            decode(&html).unwrap(),
            vec![
                vec![
                    "00123 & \"quote\"\t日本語",
                    "  *[x] # \\ <tag>&\"'\u{a0}e\u{301} 🧑‍💻\nnext\nlast \u{2242}\u{338} &unknown;",
                ],
                vec!["", ""],
            ]
        );
        assert_eq!(
            entities(" \t\r\n&#32;&#9;&#10;** [x](url) \\ &amp;lt;").unwrap(),
            " \t\r\n \t\n** [x](url) \\ &lt;"
        );
    }

    #[test]
    fn wrappers_comments_raw_style_and_case_do_not_change_table_boundaries() {
        let html = r#"<!DOCTYPE html><html><head><style>td:before { content: "<table><tr><td>fake</td></tr></table>"; }</style></head><body><!-- <table><tr><td>fake</td></tr></table> --><google-sheets-html-origin><TABLE><COLGROUP><COL width=100></COLGROUP><THEAD><TR><TH data-sheets-value='{"1":2,"2":"head"}'>head</TH></TR></THEAD><TBODY><TR><TD rowspan="&#49;" colspan=1 title="quoted > delimiter"><SPAN>value</SPAN></TD></TR></TBODY></TABLE></google-sheets-html-origin></body></html>"#;
        assert_eq!(decode(html).unwrap(), vec![vec!["head"], vec!["value"]]);
    }

    #[test]
    fn native_excel_cells_preserve_strings_and_explicit_space_runs() {
        // Native Excel for the web capture, with only its meta/style/table
        // content retained. Clipboard IDs and account metadata are excluded.
        let html = include_str!("../../tests/fixtures/excel-web-clipboard.html");
        assert_eq!(
            decode_excel(html).unwrap(),
            vec![
                vec!["plain", "00123", "12345678901234567890", "2026-11-03"],
                vec!["white", "  spaced  ", "literal", "end"],
            ]
        );
    }

    #[test]
    fn excel_break_formatting_quotes_displayed_values_and_empty_rows_survive() {
        let html = "<table><tbody><tr><td>\"open</td><td>close\"</td><td x:num='1234.5'>$1,234.50</td><td x:fmla='=1/8'>12.50%</td></tr><tr><td>line1<br>\nline2</td><td>top<br>\r\n<br>\rbottom</td><td>\"literal\"</td><td>=1+1</td></tr><tr><td></td><td></td><td></td><td></td></tr></tbody></table>";
        assert_eq!(
            decode_excel(html).unwrap(),
            vec![
                vec!["\"open", "close\"", "$1,234.50", "12.50%"],
                vec!["line1\nline2", "top\n\nbottom", "\"literal\"", "=1+1"],
                vec!["", "", "", ""],
            ]
        );
        assert_eq!(
            decode_excel("<table><tr><td>end</td></tr><tr><td></td></tr></table>").unwrap(),
            vec![vec!["end"], vec![""]]
        );
        assert_eq!(
            decode("<table><tr><td>line1<br>\nline2</td></tr></table>").unwrap(),
            vec![vec!["line1\n\nline2"]]
        );
    }

    #[test]
    fn excel_space_runs_do_not_rewrite_other_nonbreaking_spaces_or_literal_whitespace() {
        let html = "<table><tr><td> \t<span style='MSO-SPACERUN: yes'>&nbsp;<b>&nbsp;</b></span>&nbsp;<span style='mso-spacerun:no'>&nbsp;</span>  <br>\n next\nraw </td></tr></table>";
        assert_eq!(
            decode_excel(html).unwrap(),
            vec![vec![" \t  \u{a0}\u{a0}  \n next\nraw "]]
        );
        assert_eq!(
            decode(html).unwrap(),
            vec![vec![" \t\u{a0}\u{a0}\u{a0}\u{a0}  \n\n next\nraw "]]
        );
        for html in [
            "<table><tr><td colspan=2>merged</td></tr></table>",
            "<table><tr><td><table><tr><td>nested</td></tr></table></td></tr></table>",
            "<table><tr><td hidden>hidden</td></tr></table>",
            "<table><tr><td>unfinished",
        ] {
            assert!(decode_excel(html).is_err(), "{html}");
        }
        assert_eq!(
            decode_excel(&" ".repeat(MAX_SOURCE_BYTES + 1)),
            Err(TOO_LARGE)
        );
    }

    #[test]
    fn nested_merged_malformed_hidden_or_ambiguous_structures_are_rejected() {
        for html in [
            "<table><tr><td>missing",
            "<table><tr></tr></table>",
            "<table><td>no row</td></table>",
            "<table><tr><td><span>x</td></span></tr></table>",
            "<table><tr><td><table><tr><td>x</td></tr></table></td></tr></table>",
            "<table><tr><td>x</td></tr></table><table><tr><td>y</td></tr></table>",
            "<table><tr><td colspan=2>x</td></tr></table>",
            "<table><tr><td rowspan=0>x</td></tr></table>",
            "<table><tr><td COLSPAN=1 colspan=1>x</td></tr></table>",
            "<table><tr><td data-sheets-value='not JSON'>x</td></tr></table>",
            r#"<table><tr><td data-sheets-value='{"1":2}'>x</td></tr></table>"#,
            r#"<table><tr><td data-sheets-value='{"1":2,"1":3,"2":"x"}'>x</td></tr></table>"#,
            "<table><tr hidden><td>x</td></tr></table>",
            "<table style='display: none'><tr><td>x</td></tr></table>",
            "<table><tbody style='visibility: hidden'><tr><td>x</td></tr></tbody></table>",
            "<table><tr><td><span hidden>x</span></td></tr></table>",
            "<table><tr><td><img src='https://example.invalid/image.png'></td></tr></table>",
            "<plaintext><table><tr><td>x</td></tr></table>",
            "<template><table><tr><td>x</td></tr></table></template>",
        ] {
            assert!(decode(html).is_err(), "{html}");
        }
    }

    #[test]
    fn resource_limits_cover_input_depth_cells_and_ragged_padding() {
        assert_eq!(decode(&" ".repeat(MAX_SOURCE_BYTES + 1)), Err(TOO_LARGE));
        let deep = table(&format!(
            "<tr><td>{}x{}</td></tr>",
            "<span>".repeat(MAX_DEPTH),
            "</span>".repeat(MAX_DEPTH)
        ));
        assert_eq!(decode(&deep), Err(TOO_LARGE));
        let maximum = table(&"<tr><td></td></tr>".repeat(MAX_TRANSACTION_EDITS));
        assert_eq!(decode(&maximum).unwrap().len(), MAX_TRANSACTION_EDITS);
        let too_many = table(&"<tr><td></td></tr>".repeat(MAX_TRANSACTION_EDITS + 1));
        assert_eq!(decode(&too_many), Err(TOO_LARGE));
        let sparse = table(&format!(
            "<tr>{}</tr>{}",
            "<td></td>".repeat(1024),
            "<tr><td></td></tr>".repeat(64)
        ));
        assert_eq!(decode(&sparse), Err(TOO_LARGE));
    }
}

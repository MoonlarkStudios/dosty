# Strikethrough

Live and Read views draw strikethrough for parsed `~~deleted text~~` and
`~deleted text~`. Bold, italics, links, inline code, headings, quotations and table
cells keep their existing typography and semantic colors under the strike.
Escaped tildes and tildes inside code remain literal.

Use the Strikethrough toolbar control or **Cmd/Ctrl+Shift+X** to wrap selected
text in `~~`. As with the other inline commands, formatting applies separately
to selected lines and table cells. Selecting the complete contents of an existing
single- or double-tilde span and invoking the command removes its markers.
With no selection, the command inserts `~~deleted text~~` at the caret.

The active inline span reveals its editable markers. Revealed syntax is muted
and has no strike; concealed markers reserve no width. Strikethrough does not
change shaping, caret stops, selection geometry, or document height. Directional
source selections, source switching, and the existing transaction/undo path retain
their normal behavior.

See the [shared fixture](../tests/fixtures/strikethrough.md) and the
[visual validation guide](validation/README.md#software-previews).

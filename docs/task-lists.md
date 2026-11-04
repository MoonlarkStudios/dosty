# Task lists

Live view displays Markdown task markers as checkboxes. Click a checkbox to toggle
`[ ]` to `[x]`, or `[x]` / `[X]` to `[ ]`. The rest of the Markdown source stays
unchanged, including marker case until a checked task is explicitly toggled off.
Nested and quoted task items follow the parser's existing Markdown rules.

Cmd/Ctrl+Enter toggles the task on the caret's physical source line in Live or
Source view. A continuation line without its own task marker, an ordinary prose
line, or a literal marker inside code is unchanged. The command appears in
Appearance & shortcuts and the bundled guide. Read view retains its checkbox
controls and uses the same transaction path.

## Selection and pointer behavior

Toggling is one human undo group, separate from adjacent typing and other toggles.
The source selection, including anchor/head direction, remains unchanged through
toggling, Undo, and Redo. Existing source-based wrap affinity and viewport
anchoring apply to the same-length marker replacement. A checkbox click does not
move focus or collapse an existing text selection.

Pointer interaction uses measured glyph row stops, with no extra hit area over
nearby text. A toggle occurs on release over the same checkbox after a single
click. Dragging beyond four logical pixels cancels it, including dragging away
and back. A second press belonging to a double/triple click does not toggle
again. Scrolling, a layout change, source revision change, leaving the window,
or releasing outside the checkbox cancels an outstanding press. Right clicks
and ordinary text selection do not toggle checkboxes. Commands are ignored while
a document loads or an application dialog is open.

See the [shared fixture](../tests/fixtures/live-tasks.md) and the
[visual validation guide](validation/README.md#software-previews).

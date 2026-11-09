# Outline navigation and section moves

The optional right Outline sidebar lists document headings as a hierarchy. Disclosure triangles hide descendant rows in the outline only; the writing surface and Markdown remain unchanged. Search, Collapse All, and Expand All are compact header icons with tooltips, shown only when headings exist. Rows use hierarchy depth rather than Markdown heading number for indentation; drag grips appear on hover. The current section remains highlighted while editing its body, and a collapsed ancestor represents a hidden current child. Fold state is local to the open document session and resets when another document opens.

## Navigation

Open Outline with the right titlebar button, View → Outline on macOS, or Cmd/Ctrl+Alt+O. Files remains independently available on the left when a folder workspace is open. The toggle keeps Outline beside the writing surface until closed. In narrow windows it overlays the document to protect the writing column. A heading jump leaves Outline open. Hide Sidebars hides/restores the previous panel arrangement.

- Click a heading to jump to it. Read view stays in Read mode; Live and Source retain their mode.
- Click Search or press Cmd/Ctrl+Shift+J to reveal the inline heading filter, opening Outline if necessary. It matches case-insensitive terms in heading and ancestor titles. Results stay in document order with their ancestor context. Matching descendants remain visible through collapsed branches without changing fold state. There is no modal picker.
- While the filter input is focused, Up/Down selects a match and Enter jumps. Escape clears and closes the filter, returning to the editor without changing its source selection. Jumping expands the target's ancestors. Clear the filter before collapsing or rearranging rows.
- Cmd/Ctrl+Alt+Left collapses the current branch, or its parent if the current heading has no children or is already collapsed. Cmd/Ctrl+Alt+Right expands it. These commands affect the outline only.

## Rearranging

Drag the grip on the right of a heading row. The upper and lower halves of a target row mean before and after that target's **whole section**, respectively. An accent line marks a valid destination. Targets must have the same heading level; a move can cross parents, but never rewrites heading markers. Wheel scrolling remains available during a drag. Releasing outside a row, Escape, loss of window focus, or a source revision cancels the drag.

Cmd/Ctrl+Alt+Up/Down moves the section containing the caret before/after its previous/next sibling with the same parent and level. A section contains its heading, body, nested sections, and separating whitespace, ending immediately before the next heading of equal or lower level. Preamble text before the first heading stays in place. ATX and Setext headings are supported. Headings inside lists, quotations, HTML, or fenced code are excluded.

Each accepted move is one human undo group, separate from typing before and after it. A caret or selection inside moved text follows those bytes. Other endpoints rebase through the deletion/insertion; selections spanning reordered sections retain their anchor/head direction. Undo restores the original source and selection; redo restores the moved selection. Fold state follows locally moved headings through undo/redo, including duplicate titles.

The operation preserves existing bytes and line endings, adding only missing blank-line separation at the insertion boundaries. Undo removes that added separation. Before committing, the model checks that the candidate parses with the expected heading positions and levels. A move that would let an unfinished code fence or HTML block swallow another section is refused with an explanation. Invalid, adjacent no-op, and self-drops do not create undo entries. Existing transaction size limits, file guards, and dirty/recovery handling apply.

Private outline tests live beside the model and application. See
[historical measurements](performance/outline.md) for measured query and move costs.

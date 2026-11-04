# Outline navigation and section moves

The Outline sidebar lists document headings as a hierarchy. Disclosure triangles hide descendant rows in the outline only; the writing surface and Markdown remain unchanged. Collapse/expand all controls are available above the tree. The current section remains highlighted while editing its body, and a collapsed ancestor represents a hidden current child. Fold state is local to the open document session and resets when another document opens.

## Navigation

- Click a heading to jump to it. Read view stays in Read mode; Live and Source retain their mode.
- Cmd/Ctrl+Shift+J opens Quick Jump, including when the sidebar is hidden. Search is case-insensitive, matches whitespace-separated terms in heading titles and ancestor titles, and ranks title matches before ancestor-only matches. Duplicate titles show their immediate parent.
- Up/Down selects a result, Enter jumps, and Escape cancels without changing the source selection. Jumping expands the target's ancestors. The picker shows a bounded window around the selected result.
- Cmd/Ctrl+Alt+Left collapses the current branch, or its parent if the current heading has no children or is already collapsed. Cmd/Ctrl+Alt+Right expands it. These commands affect the outline only.

## Rearranging

Drag the grip on the right of a heading row. The upper and lower halves of a target row mean before and after that target's **whole section**, respectively. An accent line marks a valid destination. Targets must have the same heading level; a move can cross parents, but never rewrites heading markers. Wheel scrolling remains available during a drag. Releasing outside a row, Escape, loss of window focus, or a source revision cancels the drag.

Cmd/Ctrl+Alt+Up/Down moves the section containing the caret before/after its previous/next sibling with the same parent and level. A section contains its heading, body, nested sections, and separating whitespace, ending immediately before the next heading of equal or lower level. Preamble text before the first heading stays in place. ATX and Setext headings are supported. Headings inside lists, quotations, HTML, or fenced code are excluded.

Each accepted move is one human undo group, separate from typing before and after it. A caret or selection inside moved text follows those bytes. Other endpoints rebase through the deletion/insertion; selections spanning reordered sections retain their anchor/head direction. Undo restores the original source and selection; redo restores the moved selection. Fold state follows locally moved headings through undo/redo, including duplicate titles.

The operation preserves existing bytes and line endings, adding only missing blank-line separation at the insertion boundaries. Undo removes that added separation. Before committing, the model checks that the candidate parses with the expected heading positions and levels. A move that would let an unfinished code fence or HTML block swallow another section is refused with an explanation. Invalid, adjacent no-op, and self-drops do not create undo entries. Existing transaction size limits, file guards, and dirty/recovery handling apply.

Private outline tests live beside the model and application. See
[historical measurements](performance/outline.md) for measured query and move costs.

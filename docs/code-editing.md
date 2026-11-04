# Code editing and rendering

Code blocks share the document's focus, selection, source mappings, and undo history. Their controls are separate from the editable text.

## Behavior

- Live code blocks have a language header, Copy control with confirmation, monospace typography, soft wrapping, full-height blank lines, and a subtle active-line background. Empty fences offer a full-height content row with a “Start coding…” prompt.
- Inline code uses smaller monospace text and a background measured from the shaped spans. Concealed backticks reserve no width.
- Enter retains indentation, increases it after an opening bracket, and separates matching brackets onto their own lines. Python/YAML colons also continue indentation. Tab inserts to the next indentation stop; selections indent as complete lines, and Shift-Tab removes indentation. Backspace in leading spaces returns to the previous stop. Existing indentation is preferred; defaults are two or four spaces according to language. Complex Markdown containers retain their native edit handling.
- Empty-block typing, paste, and IME commits insert content before the closing fence while keeping that fence on its own line. Reverse selections and atomic undo/redo retain source coordinates.
- Copy controls return CommonMark's normalized code content without fences or quote/list indentation. Copying a complete code block as part of a rich selection retains its Markdown fences. Copying a fragment returns the selected text.
- Moving into fence syntax reveals that fence for literal editing, including the language. The content position immediately before a closing fence stays available for code entry. Source mode remains available for inspecting all syntax.
- Read view uses the same semantic surface and typography, syntax colors adjusted for contrast, and a Copy button. Its Markdown representation is created lazily and invalidated with the edited source block.

## Highlighting and bounds

Live highlighting uses the existing Iced grammar highlighter. Parser snapshots and preceding runs survive small edits. A changed line invalidates later runs, since a comment or string delimiter can change the styles of otherwise unchanged text. Highlighting advances only through requested resident lines; shaped paragraph keys include the resulting syntax styles.

The grammar cache holds at most 32 entries and 512 KiB of code text. Live blocks over 256 KiB, blocks with a line over 8,192 bytes, and unlabeled blocks use semantic monochrome text. These limits affect coloring; source editing, selection, wrapping, and copy remain available. The optional Read renderer retains its existing separate parser/highlighter. Completion, diagnostics, and language servers are unsupported.

See the [shared fixture](../tests/fixtures/code-editing.md) and the owning
modules' private tests for edge cases. [Historical measurements](performance/code-editing.md)
record long-code and mixed-document profiles.

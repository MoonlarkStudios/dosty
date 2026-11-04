# Table editing

Live tables show bold headers and subtle row rules. Selecting a cell reveals its
grid, row numbers, column letters, and controls without changing cell geometry.
Markdown remains authoritative; Source mode edits its syntax literally.

## Selecting and editing

Click to select a cell; type to replace its visible value. Double-click or Enter
opens a native draft of its existing text. Formatting markers stay concealed.
An unchanged draft preserves inline formatting; Source shows the exact syntax.

| Action | Selected cells | Inside a cell draft |
| --- | --- | --- |
| Arrows | Move between cells | Move the text caret |
| Shift+Arrow / Shift-click / drag | Extend a directed rectangle with a distinct active cell | Native text selection |
| Enter / Shift+Enter | Begin editing | Commit and move down/up |
| Tab / Shift+Tab | Traverse cells | Commit and move right/left |
| Backspace / Delete | Clear selected values, retaining the grid | Edit the draft |
| Escape | Collapse a range, then hide controls for a single cell | Cancel the draft without source/history changes |

Row/column labels and the corner select a row, column, or table. Shift+Space
selects a row; Ctrl+Space selects a column. Cmd/Ctrl+A selects the table, then the
document. Cmd/Ctrl+Arrow finds populated-region edges; adding Shift extends the
range. Home/End target row edges; Cmd/Ctrl adds table edges.

Cmd/Ctrl+Enter inserts a line break while editing a cell.
Tab traverses a rectangle across rows; Enter after editing traverses down columns
and wraps within the range. A Tab run remembers its starting column for Enter.
Tab at the final cell or Enter committing the final row adds a row. Clicking prose
commits a valid draft and hides table controls. After Escape hides the controls,
keyboard/IME entry resumes the remembered cell, including an implicit empty cell.

Plain Up from the header and Down from the last row move into surrounding text
or an existing blank paragraph, retaining horizontal intent. Adjacent tables are
reachable. At a document edge, a missing paragraph is created in one undo step;
typing there is a separate step. **Leave table** creates a paragraph after it.
Document-wide selections remain continuous when dragging from surrounding prose.

## Copy and paste

Cmd/Ctrl+C, X, and V transfer visible rectangular values. Quoted TSV preserves
embedded tabs, line breaks, literal quotes, Unicode, and empty cells. Input accepts
LF, CRLF, and CR records; output uses CRLF and quotes fields containing delimiters
or quotes. Ragged rows are padded. Transfers carry values, without formulas or
spreadsheet formatting. Paste inside a draft remains text in that cell.

Native copy also provides escaped HTML, text type hints, and whitespace markup.
Matching Sheets/Excel Web clipboard metadata supplies explicit cell boundaries
when plain-text quotes or final blank rows are ambiguous. Verified Sheets string
metadata preserves literal identifiers; other cells, including Excel Web currency
and percentages, use displayed text. Clipboard HTML is read inertly and never
rendered as document markup. Merged, nested, malformed, and unsupported tables
are rejected. A verified textless empty native cell clears its rectangle; an
unavailable ordinary clipboard changes nothing.

Excel Web can coerce leading-zero IDs, long numbers, formula-like text, and dates
during normal paste, even over Text-formatted destinations. Setting cells to
**Text** and using **Paste Values** preserved identifiers in the recorded macOS
browser checks, but Paste Values removed embedded tabs. Normal rich paste retained
tabs. Mixed exact identifiers/tabs have no verified universal lossless route.
Dosty retains original clipboard values without apostrophe or formula prefixes.
See the [native observations](validation/table-clipboard.md).

Paste at one selected cell grows the table as needed. Within a selected range,
one value fills the range; smaller rectangles tile only when both dimensions
divide evenly. Incompatible shapes leave source and selection unchanged. Values
do not wrap at the right edge. The pasted rectangle stays selected, retaining
range direction and the active cell.

Pasted Markdown punctuation, pipes, tabs, and edge spaces enter literally.
Untouched source/alignment survives, and safe simple outer emphasis is retained.
New edge spaces/tabs serialize as character references; cell line breaks use bare
`<br>`. Original Markdown padding remains concealed. Read and HTML export display
cell breaks; export escapes other HTML.

Copy resolves its target after preceding selection messages. Ordered clipboard
operations preserve Copy → Move → Paste in one event batch. Cut clears values
only after a verified write. Delayed cut/paste rejects changed source revisions
or selections. Rich native clipboard work runs off the UI path with retained
ownership. Each format is limited to 16 MiB and a rectangle to 65,536 cells;
resulting edits and growth also obey [transaction limits](collaboration.md#bounds-and-retention).
Oversized or unsafe expansion leaves the document unchanged.

## Contextual controls

Hover a row number or column letter to reveal its menu arrow. Clicking the label
still selects; Shift-click extends selection. Right-click a label for scoped
insert/select/clear/delete actions and column alignment. Right-click a cell,
use the toolbar table icon, Cmd/Ctrl+Shift+T, or Shift+F10 for row/column/table
groups. Clicking inside an existing rectangle preserves it.

Up/Down or Tab/Shift+Tab selects an action; Right/Enter opens a group; Enter
applies it. Left, Back, or Escape returns to the parent. Escape at the root,
outside click/wheel, or switching apps dismisses Live menus. Keyboard choices
scroll into view. Menus stay inside the viewport; cancellation preserves source
selection and history. Source uses its keyboard-operated dialog.

Opening controls commits a valid draft first. Menus capture checked table and
selection targets: unrelated replica changes rebase them; table changes reject
the action. Source dialogs remain open when switching apps.

Markdown requires a header. Inserting above it creates a blank header and moves
the old header into the body; deleting it promotes the first body row. Deleting
the only row or column removes the table. Excess physical cells must be resolved
in Source before unsafe promotion or paste. Selecting an implicit empty cell
does not change source; committing materializes it and preceding missing cells.

## Source and history

Structural commands preserve surviving text, inline formatting, separator widths,
container prefixes, line endings, and surrounding Markdown. Alignment edits only
the selected separator token. Required edge pipes and cells materialize locally.

Each committed draft, command, rectangular clear, cut, or paste is one human
transaction/undo step; paste growth is included. Enter/Tab row creation is a
separate structural step. Draft typing and undo stay local until commit; Escape
creates no document history. Saving, leaving, and closing commit a valid draft.
Unchanged values/alignment add no history. Undo restores source and selection;
paste and clear/cut also restore the directed grid and active cell. Reflow uses
source-based viewport anchoring.

Stable expected-text targets rebase unrelated edits. A changed draft target stays
available for copying or cancellation. Changed table targets or document sessions
reject commands. Loading/modal guards block contextual actions.

Merged cells, saved widths, resizing, fill handles, nested block paragraphs, and
office document import/export are unsupported. See the [release limits](evaluation.md),
[shared fixtures](../tests/fixtures), and [historical table measurements](performance/table-editing.md).

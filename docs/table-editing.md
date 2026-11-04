# Table editing

Live tables normally render with bold headers, a subtle header tint, and fine
horizontal rules. Selecting a cell reveals the grid, row numbers, column letters,
and contextual controls on that table. These overlays preserve the text and cell
geometry, keeping the transition on the page without reflow. Live editing uses
spreadsheet cell selection, following the selection/editing distinction in
[Google Sheets](https://support.google.com/accessibility/answer/1632199?hl=en).
The Markdown source remains authoritative; Source mode edits it literally.

## Keyboard interaction guideline

- **Click** selects a cell. Type to start replacing its value. **Double-click**
  or **Enter** edits its existing visible text in a native in-cell editor.
- **Arrow keys** move between selected cells. Plain **Up** from the header or
  **Down** from the last row moves into existing text or a blank paragraph outside
  the table, keeping the horizontal position near the active column. Adjacent
  tables remain reachable by the same movement. At a document edge, a missing
  outside paragraph is created with the caret ready to type. That creation is
  one undo step; movement into an existing paragraph adds no history.
  **Shift+Arrow**, Shift-click, and
  dragging extend a rectangle with a distinct active cell. Row numbers, column
  letters, and the corner select a row, column, or table. **Shift+Space** selects
  a row; **Ctrl+Space** selects a column. **Cmd/Ctrl+A** selects the table, then
  the document on a second press.
- **Cmd/Ctrl+Arrow** jumps to the edge of a populated region; adding Shift extends
  the selection. **Home/End** target the row edges; Cmd/Ctrl adds the table edges.
- While editing text, arrows move the text caret. **Enter / Shift+Enter** commit
  and move down/up; **Tab / Shift+Tab** commit and move right/left. A run of Tab
  entries remembers its starting column for the next Enter. Tab at the final
  cell, or Enter committing the final row, adds a row.
- Within a selected rectangle, Tab traverses across rows and Enter after editing
  traverses down columns, wrapping inside the selected range.
- **Cmd/Ctrl+Enter** inserts a cell line break while editing. Plain text pasted
  into the in-cell editor stays in that cell, including newlines and tabs.
- **Escape** cancels the pending draft without changing source or history.
  Outside text editing, it collapses a rectangle to its active cell. From a single
  selected cell, it hides the table apparatus while keeping the source caret and
  remembered cell target. Keyboard or IME entry resumes that cell, including an
  implicit empty cell in an imported short row.
- Clicking surrounding prose commits a valid draft and returns the table to its
  quiet reading presentation.
- **Backspace / Delete** clear only the selected rectangle, retaining its grid
  and all cells outside it. Inside the text editor they edit the local draft.
- **Leave table** creates a paragraph after the table. Document-wide text
  selections remain continuous when dragging from surrounding prose.
- **Shift+F10**, **Cmd/Ctrl+Shift+T**, the toolbar table icon, or right-clicking a
  cell opens the keyboard-accessible table controls.

Formatting markers stay concealed inside Live cells. The native draft edits the
visible value; unchanged inline formatting survives commit. Source shows the
exact syntax. Newly entered edge
spaces and literal tabs use character references so they remain visible after
leaving the cell or reopening the document. Interior spaces use ordinary source
spaces where possible. Original Markdown formatting padding stays concealed.
Line breaks serialize as bare `<br>` inside a physical Markdown row. Live, Read,
and HTML export display them as line breaks. Code tags remain literal, and
export escapes other HTML.

## Copy and paste

**Cmd/Ctrl+C, X, and V** transfer the selected rectangle as visible values using
quoted tab-separated text. Embedded tabs, line breaks, doubled quotes, Unicode,
and empty cells retain their cell boundaries. LF, CRLF, and CR input records are
accepted; line breaks inside values become logical cell breaks. Copy emits CRLF
records and quotes fields containing tabs, line breaks, or quotes. This follows
the delimiter and text-qualifier conventions described by
[Excel's text import documentation](https://support.microsoft.com/en-us/excel/text-import-wizard).

Native copy also supplies an escaped values-only HTML table with text type hints
for spreadsheet applications. Space and tab runs carry Office whitespace markup
to preserve edge spaces, repeated spaces, and embedded tabs. Google Sheets'
native clipboard format preserves IDs with leading zeros and formula-like text.
Without a native rich flavor, ordinary quoted TSV remains available.
Native Sheets and Excel Web copies are recognized from matching clipboard
metadata. Their HTML is read as an explicit table of cell values, preserving
adjacent unmatched quotes and final empty rows that plain-text quoting cannot
distinguish. Within a verified Sheets snapshot, typed string metadata supplies
literal strings; other cell types use displayed text. Excel Web cells also use
displayed text, including formatted currency and percentages, rather than
formulas or underlying number serials. Malformed, merged, nested, or unsupported
tables are rejected without changing source. Clipboard HTML is never rendered
or imported as document markup. Standard TSV retains its optional
terminal-separator convention.
An empty native spreadsheet cell can have no plain-text flavor. It clears only
its verified HTML rectangle; an unavailable ordinary clipboard leaves source
unchanged. The native HTML and empty text state must remain stable through the
read, and delayed results retain the usual revision and selection guards.

For identifier blocks in Excel Web, format destination cells as **Text**, then
use **Paste Values** (**Cmd+Shift+V** in the tested macOS browser). Normal paste
can replace Text formatting; General cells convert leading zeros, long IDs, and
date-like strings despite the clipboard's text hints. Excel Web's Paste Values
route also removed embedded tabs in native validation, while normal paste
preserved them. These Excel Web limits prevent a universally lossless default
round trip. Dosty preserves the original clipboard values without adding
apostrophe prefixes.

With one cell selected, paste starts there and expands rows and columns as
needed. Values never wrap from the right edge into a different row. With a range
selected, a single value fills it; a smaller block repeats when both selected
dimensions are exact multiples of the copied block. Other shapes leave the
document and selection unchanged with a brief message. The pasted rectangle
stays selected. Existing range anchor/head direction and active cell are retained.

Values enter literally, including Markdown punctuation, pipes, and edge spaces.
Untouched source and alignment survive; simple outer emphasis in changed cells
is retained where safe. One paste or cut is one source undo step. Identical values
add no history. Copy and paste intents resolve their target after preceding
selection commands. Ordered runtime clipboard writes and reads also preserve
Copy → Move → Paste when keys arrive in one native event batch. Cut clears values
only after a verified clipboard write, and rejects a delayed action if its source
revision or selection changed. Paste likewise rejects a delayed result if its
captured table selection or revision changed.

Inside the in-cell text editor, paste remains literal text in that draft. Grid
transfers carry values, without spreadsheet formulas or formatting. Native rich
clipboard access and serialization run on a worker, with the owner retained for
platforms that serve clipboard data from the copying process.
Ragged clipboard rows are padded with empty cells. Each serialized clipboard
format is capped at 16 MiB, and rectangles at 65,536 cells. Repeated values,
Markdown encoding, resulting source,
and newly materialized cells also respect transaction limits. Unsafe or oversized
expansion leaves the table unchanged. Excess physical source cells must be
resolved in Source before pasting. Fill handles, resizing, and saved widths
remain follow-up work.

## Contextual controls

Hover a row number or column letter to reveal a menu arrow inside its existing
handle space. Click the arrow to open its scoped menu. The row number stays in
place with the arrow below it; the column arrow uses the trailing edge of its
label. Clicking the label still selects the row or column. Shift-click retains
range extension, and arrows stay hidden during selection dragging or composition.
Only the active table has these controls; the quiet reading presentation stays
unchanged.

Right-click a row number for insert-above/below, select, clear, and delete row
actions. Right-click a column letter for insert-left/right, select, clear,
alignment, and delete column actions. A cell menu opens row, column, and table
groups plus clear contents. A click inside an existing rectangle preserves that
range. The target is highlighted while the menu is open; cancellation restores
the previous presentation without changing source selection or history.

The toolbar table icon, **Cmd/Ctrl+Shift+T**, and **Shift+F10** open the same menu
beside the active Live cell. Menus stay inside the window and scroll at limited
heights. The document remains undimmed, and the footer keeps its ordinary reading
and document controls. Use **Up/Down** or **Tab/Shift+Tab** to choose,
**Right/Enter** to open a group, and **Enter** to apply. **Left**, Back, or Escape
returns to the parent group with its trigger selected. Keyboard choices scroll
into view, including after manual menu scrolling; changing groups resets that
scroll position. Escape at the root, an
outside click, or an outside wheel event dismisses the menu and restores editor
focus. Switching apps also dismisses the context menu while leaving Source
dialogs intact. The alignment group marks the current value.

Source uses the keyboard-operated table dialog with the same structural commands.
Right-clicking or opening Live controls commits a valid pending draft first.
Selecting an implicit empty cell in an imported short row leaves source untouched;
committing a value materializes it and preceding missing cells in its row. Menus
capture checked table and selection targets, so unrelated replica changes can
rebase them while changes inside the table reject the action.

Markdown tables require a header. Inserting above it creates a blank header and
moves the old header into the body. Deleting it promotes the first body row.
Deleting the only row or column removes the table; menus and the dialog call out
this behavior. A body row with excess source cells cannot be promoted safely; resolve
those cells in Source first. Merged cells, saved column widths, nested block
paragraphs, and office document import/export remain outside this Markdown model.

## Source and history

Commands preserve surviving cell text, inline formatting, escaped pipes, separator
widths, container prefixes, and Markdown outside the table. Required edge pipes
and missing cells are materialized locally. New rows retain the table's line
ending and continuation prefix. Alignment edits only the selected separator token.

Each committed cell draft, command, paste, or rectangular clear is one human CRDT
transaction and one undo step. Row/column growth needed by paste is included in
that step. Enter/Tab row creation is a separate structural transaction.
Draft typing and local undo/redo stay in the pending native buffer; cancel creates
no document history. Saving, leaving the editor, and closing commit a valid draft
before continuing. Unchanged values and alignment add no history. Selections retain
anchor/head direction and follow surviving source. Undo restores source and
selection. Paste and clear/cut also store their directed grid and active cell in
the same history item, so undo/redo restores their original or expanded rectangle.
Source-based viewport anchoring applies through local reflow.

Menus and the Source dialog capture stable, expected-text-checked table and
selection positions.
Unrelated replica changes can rebase them. Changes inside the table or a different
document session reject the command without overwriting either version. A pending
draft checks its original cell value, allowing unrelated source changes to rebase
it. A changed target retains the draft for copying or cancellation. Pointer
targets check the source revision before acting. Loading and modal dialogs block
contextual commands; Read remains a comparison view. Existing document,
transaction, and collaboration limits apply.

## Validation

The shared [control](../tests/fixtures/table-controls.md) and
[document table](../tests/fixtures/document-tables.md) fixtures and private model
and application tests cover cell preservation, exact surrounding source, Unicode,
escaped pipes and backslashes, optional edge pipes, short/excess rows, nested
quotes/lists, LF/CRLF/CR endings, header promotion, removal of the final row or
column, reverse selections, stale dialogs, keyboard routing, and undo/redo.
Space/Tab regressions type complete sentences, Unicode, leading/trailing spaces,
and Backspace through the App and actual Live widget. Real widget tests also
cover cell selection, replacement typing, text caret movement, committing,
cancellation, rectangular dragging, Shift-click, row/column gutters, and IME
candidate keys. Cross-block replacements cover both selection directions, quoted
tables, and LF/CRLF/CR endings. Export tests retain safe cell breaks while escaping
other HTML. Measured-layout regressions
check caret movement, pointer hits, and selection geometry for edge spaces.
Mapping tests retain exact source hints inside concealed padding while Tab
crosses to the adjacent cell. Incremental-update regressions cover active edge
spaces during typing and unrelated replica imports; column insertion preserves
their text and source positions.
Application results are compared with freshly parsed projections and native
buffer snapshots in both Live and Source.

Run `mise run check`, `mise run check-repo`, and `mise run release`.
Software menu and dialog previews can be reproduced without opening native windows. Use
a temporary configuration directory to keep recent user paths out of screenshots:

```sh
DOSTY_CONFIG_DIR=/tmp/dosty-table-preview mise exec -- cargo test --locked table_controls_previews -- --ignored --nocapture --test-threads=1
DOSTY_CONFIG_DIR=/tmp/dosty-table-preview mise exec -- cargo test --locked document_table_previews -- --ignored --nocapture --test-threads=1
DOSTY_CONFIG_DIR=/tmp/dosty-table-preview mise exec -- cargo test --locked table_grid_previews -- --ignored --nocapture --test-threads=1
```

The previews are written beneath `target/theme-validation/` in every theme at
1180×820 and 760×520. Synthetic input and software rendering do not establish
native IME, screen-reader, or platform readiness; see the
[remaining release work](evaluation.md#remaining-release-work).

## Native Excel Web clipboard validation (2026-11-03)

The native macOS release build was exercised against a new Excel for the web
workbook in Chrome after the user signed in. Only synthetic scratch data was
used. Existing user documents and browser tabs were preserved. The earlier
[Sheets pass](#native-google-sheets-clipboard-validation-2026-11-03) remains
historical evidence; a new Sheets round trip also checks the Excel changes.

Native testing found and fixed three import/export differences:

- Excel's plain-text quoting is ambiguous for literal quotes. Its recognized native
  HTML now provides explicit cell boundaries and displayed values, preserving
  adjacent `"open` / `close"` cells and formatted currency/percent strings.
- Excel emits a source-formatting newline after each `<br>` and represents
  ordinary spaces inside `mso-spacerun` spans with nonbreaking-space entities.
  The Excel reader handles those conventions without changing Sheets decoding
  or literal nonbreaking spaces elsewhere. Export uses the matching span markup
  for ASCII space/tab runs; tabs and edge spaces survive normal Excel paste.
- A single empty Excel cell can provide HTML without a plain-text flavor.
  Missing plain text now triggers a narrow native check: only a recognized,
  complete, entirely empty table with stable HTML/text state may clear cells.
  Unavailable, nonempty, unsupported, or changed clipboard contents cannot be
  mistaken for a blank cell. Revision and selection guards still apply.

| Workflow | Observed result |
| --- | --- |
| Normal Dosty → Excel → Dosty, five rows × four columns | Quotes, whitespace, line breaks, Unicode, literal Markdown, and final blank row retained; Excel's converted numeric/date display values imported accurately |
| Adjacent quote fragments and quote-only cells from Excel | Explicit cell boundaries and literal quotes retained |
| Excel currency and percent cells | `$1,234.50` and `12.50%` imported as displayed strings |
| Native tab, edge spaces, multiline and blank-line cells | Exact visible values retained through the normal rich clipboard route |
| One empty cell, two empty cells, and `end` followed by empty | Correct clearing footprint; neighboring cells retained |
| Empty Dosty cell → nonempty Excel cell | One destination cell cleared |
| Paste Undo/Redo; Select All → Cut → Undo | Source and selection restored; Cut Undo restored the saved source byte for byte, including unterminated EOF |
| Google Sheets regression, five rows × four columns | All twenty original values retained without destination format changes |

Two **Excel Web limitations remain**, so this is not a claim of a universally
lossless default round trip:

- Normal rich paste converts `00123` to `123`, a 20-digit ID to a numeric display,
  and `2026-11-03` to a date display. It can replace destination Text formatting
  despite the exported text hints. For identifier blocks, setting the destination
  to Text and using Paste Values (**Cmd+Shift+V** in the tested browser) preserved
  all original ID/date/formula-like strings.
- Paste Values removed an embedded tab: Excel's formula bar already showed `ab`
  before copying back. Normal rich paste retained `a\tb`. A mixed block containing
  both exact identifiers and embedded tabs therefore has no verified single-step
  lossless Excel Web route in this pass.

The implementation keeps original values in TSV, Sheets metadata, and HTML
content. It adds no apostrophe/formula/zero-width prefixes or destination-specific
copy controls to hide those Excel behaviors. Desktop Excel, other browsers,
Windows/Linux, accessibility, native IME, OS failure handling, and input latency
remain unverified.

Formatting, strict Clippy, **355 ordinary tests**, and the release build passed,
with 29 optional tests ignored. The focused clipboard filter passed 54 tests,
with two optional tests ignored. The new coverage includes native producer
detection, explicit Excel cells, whitespace conventions, exact serialization
bounds, textless empty-cell snapshot guards, and App footprint/undo/stale-target
behavior. Automated clipboard tests use in-memory/fake seams and never replace
the system clipboard.

Reproduce with `mise run check`, `mise run check-repo`, `mise run release`, and
`mise exec -- cargo test --locked clipboard -- --test-threads=1`. For native QA,
copy the [golden values](validation/table-clipboard-excel/expected-values.json)
from a copy of the earlier [outbound table](validation/table-clipboard-native/outbound.md)
into a new default Excel range and copy it into a fresh Dosty table. Compare
visible values, then test Text plus Paste Values separately. Repeat with the
quoted, formatted, whitespace, and empty-cell ranges, and check one-step undo.

Raw evidence: [native results](validation/table-clipboard-excel/native-results.json),
[environment/source digest](validation/table-clipboard-excel/environment.json),
[checks](validation/table-clipboard-excel/check.txt),
[focused tests](validation/table-clipboard-excel/focused-tests.txt),
[release build](validation/table-clipboard-excel/release.txt), and
[repository checks](validation/table-clipboard-excel/check-repo.txt).
The [sanitized native Excel fixture](../tests/fixtures/excel-web-clipboard.html)
retains cell structure without clipboard tracking/account metadata. The result
record links synthetic saved source artifacts and distinguishes the two Excel
limits from fixed Dosty behavior. This is behavioral QA, without a new latency
or large-document performance measurement.

## Native Google Sheets clipboard validation (2026-11-03)

This earlier pass preceded the [Excel Web validation](#native-excel-web-clipboard-validation-2026-11-03).

The final release build was exercised through Chrome and a separate ad-hoc-signed
Dosty test app on macOS, using new scratch documents and a private scratch
spreadsheet. Existing user documents and browser tabs were preserved. The user
made the desktop available before the final checks. The test session remains
saved in its quiet table presentation.

Actual clipboard testing found and fixed these differences:

- Sheets leaves quote-only values such as `"literal"` unescaped. Two cells
  containing `"open` and `close"` can even have the same plain text as one quoted
  cell containing a tab. The native reader now uses explicit HTML cell boundaries
  and verified string metadata, preserving both the quotes and the rectangle.
- Generic HTML collapsed tabs/edge spaces and let Sheets infer numbers/formulas.
  The native Sheets wrapper, explicit text metadata, and preserved whitespace
  now retain the original visible strings without changing destination settings.
- A native one-column copy of `[end, empty]` contains `end\n`, with no optional
  record terminator. Native cell boundaries retain that last empty row; the
  Sheets text profile also covers it. Ordinary TSV keeps its existing terminator
  convention.
- Numeric/formula metadata contains underlying values. The reader takes displayed
  cell text for non-string types, preserving currency and percent formatting as
  literal values. It reads inert, complete, unmerged tables; unsupported, merged,
  nested, malformed, or oversized structures reject atomically.
- Producer detection now checks actual HTML tags and attributes. Cell text such
  as `data-dosty` cannot change the selected import path.
- Iced can deliver Select All and Copy/Cut before updating the view. Copy intent
  now resolves the current App range after prior selection messages. Ordered
  runtime clipboard actions also let Copy → Right → Paste read the new clipboard
  at the new cell when all keys arrive in one batch.

The final native pass verified:

| Workflow | Observed result |
| --- | --- |
| Dosty → Sheets → Dosty, five rows × four columns | All twenty values and the blank final row retained |
| `00123`, a 20-digit ID, `=1+1`, and a date-like value | Literal strings retained; `=1+1` copied back as text rather than `2` |
| Quotes, Unicode/combining accents, emoji, tabs, line breaks, Markdown punctuation and edge spaces | Exact visible values retained |
| Adjacent cells containing `"open` and `close"` | Two distinct cells retained, with both quotes |
| Formatted currency and percentages | `$1,234.50` and `12.50%` imported as displayed text |
| Cell text containing a producer-like marker | Literal `data-dosty` retained without changing neighboring quoted values |
| One blank cell; two blank cells; a value followed by a blank cell | Correct one- or two-cell footprint; nonempty destination values cleared |
| Paste into a two-column, two-row table | Both axes grew; the pasted rectangle stayed selected |
| Native paste Undo/Redo and Cut Undo | Original source and appropriate grid selection restored; Cut Undo restored the exact encoded input |
| Fast Select All → Cut | All twenty values copied and cleared, retaining the table and surrounding prose |
| Fast Copy → Right → Paste with different prior clipboard contents | Newly copied value pasted into the adjacent cell; one Undo restored exact source |
| Paste while editing a cell, then Escape | Data stayed in one pending draft; cancellation preserved source |
| Fresh paste before an unterminated EOF marker | Exact surrounding Markdown retained |

The [outbound source](validation/table-clipboard-native/outbound.md),
[final round trip](validation/table-clipboard-native/html-roundtrip.md),
[trailing-empty result](validation/table-clipboard-native/html-trailing-empty.md),
[cleared table](validation/table-clipboard-native/html-cut.md),
[adjacent quotes](validation/table-clipboard-native/ambiguous-fixed.md),
[formatted values](validation/table-clipboard-native/formatted.md),
[marker text](validation/table-clipboard-native/marker-text.md), and
[fast copy/paste](validation/table-clipboard-native/fast-copy-paste.md)
retain the observed source artifacts. The shared regression inputs are the
[native rectangle](../tests/fixtures/google-sheets-clipboard.tsv) and
[trailing empty cell](../tests/fixtures/google-sheets-trailing-empty.tsv), plus the
[ambiguous plain text](../tests/fixtures/google-sheets-ambiguous.tsv).
Source-mode capture normalizes physical clipboard line endings; it preserves
the logical record and cell delimiters used by these regressions.

Formatting, strict Clippy, **342 ordinary regressions**, repository checks, and
the release build passed; 29 optional tests were ignored. Eighteen focused
clipboard tests cover the widget/runtime route and native result delivery,
including batched keys, failed writes, and stale targets. Ten codec tests, six HTML
cell-reader tests, and five native ownership/metadata tests run without accessing
the system clipboard. The `clipboard` test filter passed 41 tests, with two optional
tests ignored. Native
validation used the real clipboard; it does not establish OS failure handling,
input latency, accessibility, or behavior on other platforms.

Reproduce automated checks with `mise run check`, `mise run check-repo`, and
`mise run release`. Run `mise exec -- cargo test --locked clipboard -- --test-threads=1`
for focused coverage. For native reproduction, open a copy of the outbound
source in Live mode, select its table, copy into a new default-formatted Sheets
range, copy that range into a fresh Dosty table, and compare visible values.
Repeat with a single blank cell and a one-column range ending in a blank cell,
adjacent quote fragments, and formatted currency/percent cells. Check rapid
Select All → Copy/Cut and Copy → Right → Paste, then Cut/Undo and paste inside a
pending cell draft.

Raw evidence: [checks](validation/table-clipboard-native/check.txt),
[focused tests](validation/table-clipboard-native/focused-tests.txt),
[repository checks](validation/table-clipboard-native/check-repo.txt),
[release build](validation/table-clipboard-native/release.txt), and
[environment/source digest](validation/table-clipboard-native/environment.json).
The [native result record](validation/table-clipboard-native/native-results.json)
records case coverage and source assertions. An earlier desktop-control app
launch stalled for roughly 585 seconds; it is excluded from application latency
claims. This pass records behavior, without a new performance measurement.

Desktop Excel was unavailable, and Excel for the web required sign-in. The user
deferred Excel to finish Sheets first. Excel, Windows, and Linux interoperability
remain unverified; Excel text hints and Linux owner lifetime have automated
coverage only. Transfers carry visible text, without formula or style transfer.

## Multi-cell clipboard validation (2026-11-03)

Formatting, strict Clippy, and **320 ordinary regressions** passed, with 29
optional tests ignored. Eleven focused real-widget clipboard tests use the
actual Cmd/Ctrl+C/X/V routes with an in-memory clipboard. They cover quoted
tabs, embedded line breaks, literal quotes and Markdown, Unicode, empty cells,
both-axis growth, preserved neighboring CRDT positions, scalar fill, tiled
blocks, incompatible shape rejection, no-op paste, and one-step source undo.
Undo/redo restores the original or expanded selection, including reverse
anchor/head direction and a Tab-moved active cell, even after another cell edit.
Failed clipboard writes and delayed cuts retain source. Draft paste remains local.

Six codec tests cover delimiter/quote parsing, incomplete literal quotes, ragged
rows, trailing blank records, and input limits. Five paste model tests cover
local source splices, optional edge pipes, implicit cells, literal values,
neighboring images, containers, LF/CRLF/CR endings, and bounded expansion. Three
history model tests cover directed implicit cells, unrelated prefix rebasing,
first-header replacement, and metadata isolation across plain or failed actions.

Nine software-rendered previews were inspected across Paper, Sage, and Midnight:
selected paste at 1180×820 and 760×520, plus the quiet 1180×820 presentation.
They include a new column and row, multiline notes, and quoted values. Examples:
[Paper selected](validation/table-clipboard/paper-range.png),
[Paper quiet](validation/table-clipboard/paper-quiet.png), and
[Midnight narrow](validation/table-clipboard/midnight-range-narrow.png).

The focused bulk-paste measurement takes five release samples each at 1,000 and
5,000 rows by three columns, growing a two-row table. Values include Unicode,
edge spaces, pipes, and Markdown punctuation. It measures the keyboard event,
App update, focus operations, and resident layout at 640×560, using the in-memory
clipboard and software text renderer. Setup, correctness checks, undo, drawing,
OS delivery, and display are excluded. Per-sample correctness and single-step
undo are checked outside timing. The raw log records min/median/max; these are
observations of the new operation, without a matched before/after comparison or
a claim about the existing 10k/50k document interaction profiles.

| Pasted rows × columns | Samples | Min / median / max (ms) |
| --- | ---: | ---: |
| 1,000 × 3 | 5 | 6.49 / 6.72 / 7.47 |
| 5,000 × 3 | 5 | 76.10 / 78.68 / 82.73 |

Reproduce with a temporary configuration directory:

```sh
mise run check
mise run check-repo
mise run release
DOSTY_CONFIG_DIR=/tmp/dosty-clipboard-validation mise exec -- cargo test --locked clipboard -- --test-threads=1
mise exec -- cargo test --locked grid_paste -- --test-threads=1
mise exec -- cargo test --locked table_history -- --test-threads=1
DOSTY_CONFIG_DIR=/tmp/dosty-clipboard-validation mise exec -- cargo test --locked table_clipboard_previews -- --ignored --nocapture --test-threads=1
DOSTY_CONFIG_DIR=/tmp/dosty-clipboard-validation mise exec -- cargo test --locked --release bulk_widget_paste_measurements -- --ignored --nocapture --test-threads=1
```

Raw evidence: [focused tests](validation/table-clipboard/focused-tests.txt),
[checks](validation/table-clipboard/check.txt),
[repository checks](validation/table-clipboard/check-repo.txt),
[release build](validation/table-clipboard/release.txt),
[preview run](validation/table-clipboard/previews.txt),
[bulk paste measurements](validation/table-clipboard/bulk-paste.txt), and
[environment/source digest](validation/table-clipboard/environment.json).

The clipboard tests establish values-based text handling and editor behavior
using representative spreadsheet payloads. They do not establish native
Google Sheets or Excel interoperability on any platform. OS clipboard ownership,
native event delivery, Office HTML, formula/style transfer, accessibility, GPU
presentation, and input-to-display latency remain outside this validation.
Existing windows, unsaved documents, and the system clipboard were untouched.

## Document-edge paragraph validation (2026-11-03)

Formatting, strict Clippy, and **295 ordinary regressions** passed, with 27 optional
tests ignored. Eight focused real-widget tests cover both existing destinations
and missing paragraphs at the document edges. Plain Up from the first row can
create a paragraph before the table/container; Down from the last row can create
one after it. The caret is ready for prose immediately.

The edge cases cover no, one, and two existing line endings, reuse of leading
whitespace, LF/CRLF/CR preservation, quoted and list-nested containers, retained
trailing quote rails, and repeated outward arrows without duplicate separators.
Creation and Unicode typing undo and redo independently. Existing prose and blank
paragraph destinations remain source/history-free navigation. Stale revisions,
wrong logical edges, invalid targets, modifiers, and pending drafts cannot
accidentally create a paragraph. The adjacent-table and virtualized 1,000-row
implicit-cell cases remain in the focused run.

Reproduce the focused cases with an isolated configuration directory:

```sh
DOSTY_CONFIG_DIR=/tmp/dosty-edge-paragraph-validation mise exec -- cargo test --locked plain_vertical_table_escape -- --test-threads=1
```

Raw evidence: [focused tests](validation/table-edge-paragraphs/focused-tests.txt),
[checks](validation/table-edge-paragraphs/check.txt),
[repository checks](validation/table-edge-paragraphs/check-repo.txt),
[release build](validation/table-edge-paragraphs/release.txt), and
[environment/source digest](validation/table-edge-paragraphs/environment.json).
Synthetic widget events and software geometry do not establish native OS input
delivery or accessibility behavior. This pass makes no performance claim and
leaves existing native windows and unsaved documents untouched. The working tree
includes retained user changes; historical measurements keep their original scope.

## Arrow navigation validation (2026-11-03)

Historical evidence: captured before missing paragraphs could be created at document edges.

Formatting, strict Clippy, and **293 ordinary regressions** passed, with 27 optional tests ignored.

Six focused regressions drive the real Live widget through plain Up/Down at the
table's logical row edges. They cover:

- Reaching the nearest surrounding prose from wrapped, multiline, and implicit
  cells, then typing Unicode and undoing it without changing the table.
- Quoted and list-nested tables with LF, CRLF, and CR endings, preserving their
  source bytes and container prefixes through exit, typing, and undo.
- Moving between adjacent tables into the same measured column.
- Retaining Shift range extension, modified-arrow region navigation, and native
  cell-draft text-caret movement.
- Clamping at document edges without an existing safe destination, and using
  existing blank paragraphs before or after a table without adding source text.
- Leaving the last implicit cell of a virtualized 1,000-row table in a narrow
  viewport, typing into following prose, and restoring it with one undo.

Navigation leaves Markdown and undo history unchanged. A subsequent prose edit
has one undo step, with source selection and fresh projection/native-buffer
oracles checked by the regressions. CRLF destinations stay outside the physical
line-ending pair. No paragraph is created merely by pressing an arrow key.

Reproduce the focused cases with an isolated configuration directory:

```sh
DOSTY_CONFIG_DIR=/tmp/dosty-arrow-validation mise exec -- cargo test --locked plain_vertical_table_escape -- --test-threads=1
```

Raw evidence: [focused tests](validation/table-arrow-navigation/focused-tests.txt),
[checks](validation/table-arrow-navigation/check.txt),
[repository checks](validation/table-arrow-navigation/check-repo.txt),
[release build](validation/table-arrow-navigation/release.txt), and
[environment/source digest](validation/table-arrow-navigation/environment.json).
Synthetic widget events do not establish native OS input delivery or accessibility
behavior. This pass makes no new performance claim; the virtualized case checks
correctness without timing assertions. Earlier measurements retain their original
scope and source digests.

## Table interaction polish validation (2026-11-03)

Historical polish evidence: recorded before arrow navigation out of tables.

Formatting, strict Clippy, and **287 ordinary regressions** passed, with 27
optional tests ignored. Four new regressions cover:

- Chevron clicks opening the scoped menu while preserving source selection,
  undo history, viewport-relative cell rectangles, and ordinary label selection,
  at 320/640px editor widths and 75/100/150% zoom.
- Header draft commit through the column arrow, pointer feedback over the native
  editor, Shift-click range extension, and redraws when crossing the arrow or
  changing modifier availability.
- Measured keyboard selection visibility in a constrained 240px-high layout,
  including Home/End and returning to a scrolled parent group.
- Context menu dismissal on window unfocus while retaining Source dialogs,
  source selections, and history.

The native window's minimum size is 760×520; the 240px layout deliberately
exercises a more constrained menu viewport. Twelve application software previews
cover row and column hover states in Paper, Sage, and Midnight at 1180×820 and
760×520. All twelve were inspected. Representative results:
[Paper column](validation/table-polish/paper-column.png),
[Paper row](validation/table-polish/paper-row.png), and
[Midnight narrow column](validation/table-polish/midnight-column-narrow.png).
These renders and synthetic events do not establish native OS input delivery,
accessibility, or input-to-display latency. Existing windows and unsaved documents
were left untouched.

Reproduce the previews with an isolated configuration directory:

```sh
DOSTY_CONFIG_DIR=/tmp/dosty-polish-validation mise exec -- cargo test --locked table_hover_previews -- --ignored --nocapture --test-threads=1
```

Raw evidence: [checks](validation/table-polish/check.txt),
[repository checks](validation/table-polish/check-repo.txt),
[release build](validation/table-polish/release.txt),
[preview run](validation/table-polish/previews.txt), and
[environment/source digest](validation/table-polish/environment.json).
This pass makes no performance claim. The working tree includes retained user
changes; the earlier performance evidence keeps its original source digests and
measurement scope.

## Context menu validation (2026-11-02)

Historical evidence: captured before hover controls, keyboard reveal, and window-unfocus dismissal.

Formatting, strict Clippy, and **283 ordinary regressions** passed, with 26 optional
tests ignored. Five new regressions cover menu preview/cancel without source or
selection changes; clearing only a row, column, or reverse rectangle with one-step
undo; structural/alignment commands and checked targets through replica changes;
viewport-edge positioning and actual outside-click dismissal; and real pointer
hits on row/column handles and the pending cell editor. The existing full-widget
keyboard test now opens the same Live menu through the shortcut, navigates into
column actions, and applies a command; Source retains its dialog coverage.

Eighteen application software previews cover cell, row, and column menus in all
three themes at 1180×820 and 760×520, including bottom/right edge placement.
All eighteen were inspected. Examples: [Paper row](validation/table-menus/paper-row.png),
[Paper column](validation/table-menus/paper-column.png),
[Sage narrow column](validation/table-menus/sage-column-narrow.png), and
[Midnight narrow cell](validation/table-menus/midnight-cell-narrow.png).
These renders do not establish native OS input delivery or screen-reader behavior.
No existing native windows or documents were changed.

Reproduce the previews with an isolated configuration directory:

```sh
DOSTY_CONFIG_DIR=/tmp/dosty-menu-validation mise exec -- cargo test --locked table_context_previews -- --ignored --nocapture --test-threads=1
```

Raw evidence: [checks](validation/table-menus/check.txt),
[repository checks](validation/table-menus/check-repo.txt),
[release build](validation/table-menus/release.txt),
[preview run](validation/table-menus/previews.txt), and
[environment/source digest](validation/table-menus/environment.json).
The working tree includes retained user changes. This pass changes menu presentation
and command routing; it makes no new performance claim. The earlier presentation
measurements below retain their original scope and source digests.

## Table presentation validation (2026-11-02)

Historical presentation evidence: captured before the footer controls moved into contextual menus.

Formatting, strict Clippy, and **278 ordinary regressions** passed, with 25
optional tests ignored. Five new regressions cover identical cell geometry and
viewport position across activation/Escape at two widths, prose exit with one-step
commit history, remembered implicit-cell keyboard/IME re-entry, handles restricted
to the active table at three zoom levels, and ignored offscreen handle clicks.
The existing source/projection, composition, directed-selection, and undo oracles
remain in the full check.

The spreadsheet fixture renders quiet, range-selected, and native draft states
in Paper, Sage, and Midnight at 1180×820 and 760×520. All eighteen software
previews were inspected. Representative results:
[Paper reading](validation/table-presentation/paper-quiet.png),
[Paper selected](validation/table-presentation/paper-range.png),
[Sage editing](validation/table-presentation/sage-draft.png), and
[Midnight narrow reading](validation/table-presentation/midnight-quiet-narrow.png).
At narrow heights, selection anchoring can scroll the table header out of view;
offscreen handles remain clipped and do not receive clicks. Existing native
windows and documents were left untouched. These are actual application software
renders, with synthetic input; native IME delivery, accessibility, clipboard
interoperability, and OS/GPU presentation still require manual validation.

The release table-input harness uses the same 1,000/5,000-row fixtures, bundled
fonts, 30 serial samples, measured phases, and untimed restoration described in
the historical spreadsheet interaction report below. It includes pending draft
input, Tab commit, cell navigation, and a 31×3 range. The mixed-document profiles
use 10k/50k blocks (910,450 / 4,574,450 bytes), 30 samples per repeated interaction,
and the event/layout/drawing/software-raster phases described in
[the profile methodology](performance.md#reproduce). Cold values
are separate single observations. These observations are not a matched speedup
comparison or a native input-to-display latency guarantee.

| Table interaction, including software rendering | 1,000 rows, ms p50 / p95 / max | 5,000 rows, ms p50 / p95 / max |
| --- | ---: | ---: |
| Space in pending draft | 2.866 / 3.004 / 3.018 | 2.820 / 2.941 / 2.961 |
| Text in pending draft | 2.832 / 3.120 / 3.145 | 2.799 / 2.928 / 3.010 |
| Tab commit and advance | 4.656 / 4.984 / 5.040 | 11.255 / 11.837 / 11.951 |
| Arrow navigation | 2.871 / 3.048 / 3.066 | 2.808 / 2.912 / 2.952 |
| Select 31×3 rectangle | 3.536 / 3.807 / 3.834 | 3.450 / 3.621 / 3.661 |

At 5,000 rows, pending-text median input/update/layout was 0.001 / 0.001 / 0.015 ms; Tab's median update was 8.502 ms, including its source commit. Rasterization dominated draft input. Phase medians must not be summed to reconstruct total percentiles.

| Mixed-document interaction | 10k blocks, ms p50 / p95 / max | 50k blocks, ms p50 / p95 / max |
| --- | ---: | ---: |
| Middle typing | 1.887 / 2.697 / 2.802 | 5.882 / 8.636 / 8.788 |
| Disclosure | 2.978 / 3.236 / 3.784 | 14.805 / 16.674 / 17.093 |
| Half-document selection | 1.258 / 1.366 / 1.373 | 1.626 / 1.769 / 1.894 |
| Scrolling | 1.312 / 1.559 / 1.741 | 1.317 / 1.624 / 1.744 |

Cold initialization was 39.965 / 223.554 ms (10k / 50k), separately from first layout/draw/software rendering at 28.520 / 46.122 ms. Each cold value has one sample. The 50k end-typing run retained a 127.873 ms maximum with a 6.211 ms p95. Structural scaling costs remain visible in the raw profiles.

Raw evidence: [checks](validation/table-presentation/check.txt),
[repository checks](validation/table-presentation/check-repo.txt),
[release build](validation/table-presentation/release.txt),
[preview run](validation/table-presentation/previews.txt),
[table input and selection](validation/table-presentation/table-input.txt),
[10k profile](validation/table-presentation/profile-10k.txt),
[50k profile](validation/table-presentation/profile-50k.txt), and
[environment and complete source digest](validation/table-presentation/environment.json).
Measurements use mise's Rust 1.95.0 toolchain, release mode with thin LTO and one
codegen unit, Iced 0.14, bundled Newsreader fonts, and macOS 26.7.1 arm64. CPU
model and physical memory were unavailable in this execution environment. The
working tree includes existing user changes. Benchmark/profile workloads ran
serially; other desktop activity was not controlled. Ordinary regressions contain
no timing gates.

## Spreadsheet interaction validation (2026-11-01)

Historical evidence: this section records the first spreadsheet interaction pass,
before the contextual reading/editing presentation described above.

Formatting, strict Clippy, and **273 ordinary regressions** passed, with 25
optional tests ignored. Thirteen focused regressions cover directed rectangles,
clearing only selected values, draft cancellation and local undo/redo, one-step
commit history, literal punctuation and Unicode, styled text preservation,
implicit imported cells, tab-entry columns, range traversal, stale clicks,
unrelated source changes, changed-cell conflicts, and a concurrently removed
table. Real widget tests exercise pointer selection, double-click, Shift-click,
dragging, row/column gutters, text-caret arrows, newline/commit/escape routing,
populated-region jumps, shortcuts, first-character undo, right-click after commit,
CRLF clipboard normalization, and IME candidate keys. A 1,000-row fixture
checks that column selection and cancellation preserve the active cell and
viewport. Fresh projection and native-buffer oracles retain the source contract.

The [spreadsheet fixture](../tests/fixtures/table-grid.md) renders range and native
draft states at 1180×820 and 760×520 in Paper, Sage, and Midnight. All twelve
software previews were inspected. Representative results:
[Paper range](validation/table-grid/paper-range.png),
[Paper draft](validation/table-grid/paper-draft.png),
[Midnight narrow range](validation/table-grid/midnight-range-narrow.png), and
[Sage narrow draft](validation/table-grid/sage-draft-narrow.png).
No native windows or user documents were opened or changed. Synthetic IME events
and software rendering do not establish native OS delivery, screen-reader,
clipboard interoperability, GPU presentation, or input-to-display latency.

The focused release profile uses 1,000/5,000-row, three-column tables with Unicode
and inline formatting. Bundled fonts are loaded before measurements. Space and
text update the native pending draft; Tab commits once and selects the next
cell. Reopening the growing first-cell draft is outside timing. Arrow navigation
and a 31×3 rectangle are measured in the middle of the table, with selection reset
outside timing. Each action has 30 serial samples, including event/application
updates, layout, drawing, and software rasterization. Native focus operations can
perform layout inside the update phase. Initial construction, restoration,
background IO, OS event delivery, and GPU/display work are excluded. This changes
the editing contract and harness from the historical document-table profile;
these runs are current observations, not a matched speedup comparison.

| Interaction, including software rendering | 1,000 rows, ms p50 / p95 / max | 5,000 rows, ms p50 / p95 / max |
| --- | ---: | ---: |
| Space in pending draft | 3.054 / 3.172 / 3.173 | 3.042 / 3.146 / 3.169 |
| Text in pending draft | 3.038 / 3.161 / 3.182 | 3.073 / 3.130 / 3.130 |
| Tab commit and advance | 4.788 / 4.948 / 5.168 | 11.216 / 11.480 / 11.741 |
| Arrow navigation | 3.244 / 3.377 / 3.390 | 3.253 / 3.368 / 3.380 |
| Select 31×3 rectangle | 4.070 / 4.148 / 4.408 | 4.047 / 4.169 / 4.399 |

At 5,000 rows, the median draft-text input/update/layout phases were
0.001 / 0.001 / 0.013 ms; rasterization dominated that sample. Tab's median update
was 8.252 ms, including the one source commit. These phase medians are descriptive
and must not be summed to reconstruct total percentiles.

The mixed-document 10k/50k-block profiles also passed with 30 samples per repeated
interaction. They retain continuous selection/dragging, disclosure, scrolling,
IME commit, and undo/redo coverage beyond tables.

| Mixed-document interaction | 10k blocks, ms p50 / p95 / max | 50k blocks, ms p50 / p95 / max |
| --- | ---: | ---: |
| Middle typing | 1.750 / 1.931 / 2.357 | 5.689 / 7.388 / 8.163 |
| Disclosure | 2.995 / 3.551 / 3.649 | 12.113 / 12.767 / 12.798 |
| Half-document selection | 1.228 / 1.308 / 1.340 | 1.605 / 1.703 / 1.720 |
| Scrolling | 1.529 / 2.005 / 2.091 | 1.503 / 1.992 / 2.127 |

Cold initialization was 38.654 / 203.496 ms (10k / 50k), separately from first
layout/draw/software rendering at 28.645 / 43.296 ms. Each cold value is one sample.
The 50k end-typing run retained a 151.082 ms maximum, with a 13.019 ms p95. These
observations retain the document's structural scaling limits and do not establish
a native frame-time guarantee.

Reproduce with mise's Rust 1.95.0 toolchain and release settings (thin LTO, one
codegen unit):

```sh
DOSTY_CONFIG_DIR=/tmp/dosty-table-preview mise exec -- cargo test --locked table_grid_previews -- --ignored --nocapture --test-threads=1
mise exec -- cargo test --locked --release performance_document_table_typing -- --ignored --nocapture --test-threads=1
DOSTY_PROFILE_BLOCKS=10000 DOSTY_PROFILE_SAMPLES=30 mise run profile
DOSTY_PROFILE_BLOCKS=50000 DOSTY_PROFILE_SAMPLES=30 mise run profile
```

Raw evidence: [checks](validation/table-grid/check.txt),
[repository checks](validation/table-grid/check-repo.txt),
[release build](validation/table-grid/release.txt),
[preview run](validation/table-grid/previews.txt),
[table input and selection profile](validation/table-grid/table-input.txt),
[10k profile](validation/table-grid/profile-10k.txt),
[50k profile](validation/table-grid/profile-50k.txt), and
[environment and complete source digest](validation/table-grid/environment.json).
The working tree includes existing user changes. macOS 26.7.1 arm64, Iced 0.14,
and the bundled Newsreader fonts were used; CPU model and physical memory were
unavailable in this execution environment. Other desktop activity was not
controlled. Initial queued release runs were stopped before measurements while
final correctness work continued; their [input](validation/table-grid/table-input-interruption.txt)
and [profile](validation/table-grid/profile-10k-interruption.txt) logs are retained
and excluded from reported results. Ordinary regressions contain no timing gates.

## Release measurements (2026-10-30)

Historical evidence: this section records the initial controls, before the column
transaction, keyboard feedback, and document editing changes.

The command benchmark uses 1,000 and 5,000 body rows, three columns, Unicode and
inline formatting, and 30 samples per action. Each sample starts from a fresh App;
construction and initial synchronization are outside timing. Opening the dialog,
planning and committing its transaction, projection/native synchronization, and
recovery/search bookkeeping are included. Widget focus effects are scheduled but
not executed. Fresh-parser/native snapshot oracles and Undo run outside timing.
Layout, rasterization, IO, GPU work, and native event/display delivery are excluded.

Reproduce serially with the pinned Rust 1.95.0 toolchain in release mode:

```sh
mise exec -- cargo test --locked --release performance_table_controls -- --ignored --nocapture --test-threads=1
DOSTY_PROFILE_BLOCKS=10000 DOSTY_PROFILE_SAMPLES=30 mise run profile
DOSTY_PROFILE_BLOCKS=50000 DOSTY_PROFILE_SAMPLES=30 mise run profile
```

These are local observations on macOS 26.7.1 (25G241), arm64, with Iced 0.14.
The sandbox did not expose the CPU model or memory through `sysctl`. Other desktop
activity was not controlled. No before/after speedup, native frame-time guarantee,
or timing assertion in ordinary tests is claimed.

| Command | 1,000 rows, ms p50 / p95 / max | 5,000 rows, ms p50 / p95 / max |
| --- | ---: | ---: |
| Insert row below | 1.531 / 1.872 / 1.938 | 8.067 / 8.894 / 8.894 |
| Insert column right | 36.831 / 37.406 / 37.685 | 838.486 / 844.333 / 869.331 |
| Delete column | 37.942 / 38.203 / 38.366 | 868.671 / 893.018 / 906.257 |
| Align right | 1.575 / 1.737 / 1.834 | 8.051 / 8.178 / 8.284 |

These initial column commands touched every physical row and restarted CRDT
position searches for every source edit. The approximately 0.84–0.87 second
medians at 5,000 rows describe that historical implementation. The later
[batch transaction optimization](table-column-performance.md) retains the source
edits and undo behavior while advancing one CRDT cursor through the batch.

The existing mixed-content interaction profile passed at 10k and 50k blocks with
30 samples per repeated interaction. It covers typing, selection/dragging,
disclosure, scrolling, IME, and Undo/Redo; it does not invoke the new dialog.
See the [profile methodology](performance.md#reproduce) for its phases and limits.

| Interaction | 10k blocks, ms p50 / p95 / max | 50k blocks, ms p50 / p95 / max |
| --- | ---: | ---: |
| Middle typing | 2.533 / 3.065 / 3.251 | 6.219 / 8.045 / 10.526 |
| Disclosure | 3.521 / 3.864 / 3.967 | 16.543 / 18.013 / 18.279 |
| Half-document selection | 1.735 / 1.826 / 1.934 | 2.925 / 3.113 / 3.163 |
| Scrolling | 1.503 / 1.922 / 2.007 | 1.525 / 1.868 / 1.911 |

Cold App/source initialization was 42.08 / 216.51 ms (10k / 50k), with first
layout/draw/software rasterization at 36.42 / 46.59 ms; each is one sample,
separate from repeated interactions. End typing retained a 125.72 ms maximum at
50k, so these observations do not remove existing structural scaling limits.

Final validation passed formatting, strict Clippy, **235 ordinary regressions**
(22 optional tests ignored), repository checks, the release build, the table
benchmark, both interaction profiles, and all six software dialog previews.
The first sandboxed suite could not bind the existing localhost HTTP image test;
the full suite passed with localhost access. No native windows or user documents
were opened or changed. Visual inspection covered every theme and both sizes.

Raw evidence: [checks](validation/table-controls/check.txt),
[sandbox limitation](validation/table-controls/check-sandboxed.txt),
[repository checks](validation/table-controls/check-repo.txt),
[release](validation/table-controls/release.txt),
[table benchmark](validation/table-controls/table-benchmark.txt),
[10k profile](validation/table-controls/profile-10k.txt),
[50k profile](validation/table-controls/profile-50k.txt), and
[environment/source hashes](validation/table-controls/environment.json).
Representative previews: [Paper](validation/table-controls/paper.png),
[Paper narrow](validation/table-controls/paper-narrow.png),
[Midnight narrow](validation/table-controls/midnight-narrow.png), and
[Sage narrow](validation/table-controls/sage-narrow.png).

## Space feedback validation (2026-10-30)

Historical evidence: the earlier keyboard feedback fix passed formatting, strict Clippy, 247 ordinary regressions
(22 optional tests ignored), repository checks, and the release build. Real
keyboard events cover leading/trailing spaces, Unicode, Tab and undo. Measured
geometry checks caret advance, selection and pointer hits; fresh-layout oracles
also cover disclosure in a virtual window. Clearing a value retains its original
padding, including a final cell without an edge pipe or line ending.

The same command benchmark and mixed-document profiles above ran serially with
30 samples, Rust 1.95.0, thin LTO and one codegen unit on an Apple M2 Max with
32 GB memory, macOS 26.7.1 (25G241). Other desktop activity was not controlled.
These runs check broad scaling; they do not specifically time a table-space
keypress or establish native input-to-display latency. The command benchmark
excludes layout; the document profiles include software rasterization.

| Interaction | Size | ms p50 / p95 / max |
| --- | --- | ---: |
| Insert column right | 5,000 rows | 69.442 / 71.501 / 71.878 |
| Delete column | 5,000 rows | 122.258 / 126.113 / 127.105 |
| Middle typing | 10k blocks | 2.198 / 2.381 / 2.728 |
| Middle typing | 50k blocks | 7.053 / 9.965 / 10.987 |
| Disclosure | 10k blocks | 3.447 / 4.163 / 4.403 |
| Disclosure | 50k blocks | 14.733 / 16.134 / 16.531 |
| Scrolling | 10k blocks | 1.540 / 1.824 / 1.825 |
| Scrolling | 50k blocks | 1.485 / 1.771 / 1.781 |

Cold initialization was 40.52 / 210.69 ms (10k / 50k); first layout, draw and
software rasterization took 29.34 / 44.79 ms. Each is a separate single sample.
No native windows or user documents were opened or changed.

Raw evidence: [checks](validation/table-keyboard/check.txt),
[repository checks](validation/table-keyboard/check-repo.txt),
[release](validation/table-keyboard/release.txt),
[command benchmark](validation/table-keyboard/benchmark.txt),
[10k profile](validation/table-keyboard/profile-10k.txt),
[50k profile](validation/table-keyboard/profile-50k.txt), and
[environment/source hash](validation/table-keyboard/environment.txt).


## Document table interaction validation (2026-10-30)

This later pass implements the current document editing contract above. Formatting,
strict Clippy, **260 ordinary tests** (24 optional tests ignored), repository
checks, and the release build passed. Real keyboard regressions cover Space,
Tab/Shift+Tab, Enter/Shift+Enter, literal tabs, leaving a table, selected rows, and
IME candidate keys without a premature source edit. Model/application tests cover
cell-boundary deletion, partial formatting repair, whole-row clearing, quoted
cross-block selections, short-row materialization, stale pointer targets, and
one-step undo. Measured Unicode/blank-line geometry tests catch physical-line
rebasing for both shaped carets and pointer hits. Read and export retain safe cell
line breaks; export escapes other HTML and preserves whitespace.

Software previews cover Live at 1180×820 and the 760×520 minimum window, plus Read,
in Paper, Sage, and Midnight. The contextual footer retains Source/Live, Read,
Undo, and Redo. Its height stays fixed as the caret enters/leaves a table. No native
windows or unsaved user documents were changed. These are headless checks;
native IME delivery, accessibility, OS clipboard interchange, and GPU presentation
still need platform validation.

The release runs below used Rust 1.95.0, Iced 0.14, thin LTO, one codegen unit,
macOS 26.7.1 arm64, and 30 serial samples per action. The sandbox did not expose CPU
model or physical memory. Other desktop activity was uncontrolled. Mixed-document
tail variation is retained; these are observations, not a speedup claim or native
latency guarantee. Ordinary tests contain no machine-dependent timing assertions.

| Interaction | Size | ms p50 / p95 / max |
| --- | --- | ---: |
| Space, real widget and software rendering | 1,000 rows | 4.373 / 4.693 / 5.251 |
| Space, real widget and software rendering | 5,000 rows | 10.887 / 11.256 / 11.406 |
| Text, real widget and software rendering | 5,000 rows | 10.302 / 10.725 / 10.893 |
| Tab, real widget and software rendering | 5,000 rows | 2.492 / 2.674 / 2.716 |
| Insert column right, no rendering | 5,000 rows | 71.177 / 77.820 / 80.477 |
| Delete column, no rendering | 5,000 rows | 123.425 / 125.684 / 130.040 |
| Middle typing, mixed document | 10k blocks | 2.750 / 6.565 / 7.176 |
| Middle typing, mixed document | 50k blocks | 7.545 / 9.568 / 9.930 |
| Disclosure, mixed document | 10k blocks | 3.477 / 3.998 / 4.092 |
| Disclosure, mixed document | 50k blocks | 14.813 / 16.022 / 16.042 |

The new table input profile warms the real widget at 720×800, then types Space
and `x`, Tabs to the next cell, and restores the first cell's caret outside timing.
It repeats this growing value 30 times in 1,000/5,000-row, three-column tables with
Unicode and inline formatting. Input, update, layout, drawing, and software raster
costs are recorded separately. The column profile uses independent documents and
excludes setup, validation, undo, and rendering, as described in the historical
report. All runs exclude OS event delivery and GPU/display presentation.

```sh
mise exec -- cargo test --locked --release performance_table_controls -- --ignored --nocapture --test-threads=1
mise exec -- cargo test --locked --release performance_document_table_typing -- --ignored --nocapture --test-threads=1
DOSTY_PROFILE_BLOCKS=10000 DOSTY_PROFILE_SAMPLES=30 mise run profile
DOSTY_PROFILE_BLOCKS=50000 DOSTY_PROFILE_SAMPLES=30 mise run profile
```

[Raw checks](validation/document-tables/check.txt),
[repository checks](validation/document-tables/check-repo.txt),
[release build](validation/document-tables/release.txt),
[column benchmark](validation/document-tables/column-benchmark.txt),
[table input profile](validation/document-tables/table-typing.txt),
[10k profile](validation/document-tables/profile-10k.txt),
[50k profile](validation/document-tables/profile-50k.txt), and
[environment/source digest](validation/document-tables/environment.txt) retain
reproduction evidence. An initial resource-reporting wrapper failed on the
sandbox's `kern.clockrate` access after its benchmark passed; the
[interruption log](validation/document-tables/column-wrapper-interruption.txt)
is retained, and reported numbers use the successful unwrapped rerun.

Representative previews:
[Paper at minimum size](validation/document-tables/paper-document-table-narrow.png),
[Midnight Live](validation/document-tables/midnight-document-table.png), and
[Sage Read](validation/document-tables/sage-document-table-read.png).

# Native spreadsheet clipboard observations — 2026-11-03

Historical macOS/Chrome observations using synthetic scratch documents and
spreadsheets. The source/environment digests below identify each build.
See [current table behavior](../table-editing.md#copy-and-paste) for supported workflows.

## Native Excel Web clipboard validation (2026-11-03)

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

Reproduce with `mise run check`, `mise run check-repo`, `mise run release`, and
`mise exec -- cargo test --locked clipboard -- --test-threads=1`. For native QA,
copy the [golden values](table-clipboard-excel/expected-values.json)
from a copy of the earlier [outbound table](table-clipboard-native/outbound.md)
into a new default Excel range and copy it into a fresh Dosty table. Compare
visible values, then test Text plus Paste Values separately. Repeat with the
quoted, formatted, whitespace, and empty-cell ranges, and check one-step undo.

Raw evidence: [native results](table-clipboard-excel/native-results.json),
[environment/source digest](table-clipboard-excel/environment.json),
[checks](table-clipboard-excel/check.txt),
[focused tests](table-clipboard-excel/focused-tests.txt),
[release build](table-clipboard-excel/release.txt), and
[repository checks](table-clipboard-excel/check-repo.txt).
The [sanitized native Excel fixture](../../tests/fixtures/excel-web-clipboard.html)
retains cell structure without clipboard tracking/account metadata. The result
record links synthetic saved source artifacts and distinguishes the two Excel
limits from fixed Dosty behavior. This is behavioral QA, without a new latency
or large-document performance measurement.

## Native Google Sheets clipboard validation (2026-11-03)

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

The [outbound source](table-clipboard-native/outbound.md),
[final round trip](table-clipboard-native/html-roundtrip.md),
[trailing-empty result](table-clipboard-native/html-trailing-empty.md),
[cleared table](table-clipboard-native/html-cut.md),
[adjacent quotes](table-clipboard-native/ambiguous-fixed.md),
[formatted values](table-clipboard-native/formatted.md),
[marker text](table-clipboard-native/marker-text.md), and
[fast copy/paste](table-clipboard-native/fast-copy-paste.md)
retain the observed source artifacts. The shared regression inputs are the
[native rectangle](../../tests/fixtures/google-sheets-clipboard.tsv) and
[trailing empty cell](../../tests/fixtures/google-sheets-trailing-empty.tsv), plus the
[ambiguous plain text](../../tests/fixtures/google-sheets-ambiguous.tsv).
Source-mode capture normalizes physical clipboard line endings; it preserves
the logical record and cell delimiters used by these regressions.

Reproduce automated checks with `mise run check`, `mise run check-repo`, and
`mise run release`. Run `mise exec -- cargo test --locked clipboard -- --test-threads=1`
for focused coverage. For native reproduction, open a copy of the outbound
source in Live mode, select its table, copy into a new default-formatted Sheets
range, copy that range into a fresh Dosty table, and compare visible values.
Repeat with a single blank cell and a one-column range ending in a blank cell,
adjacent quote fragments, and formatted currency/percent cells. Check rapid
Select All → Copy/Cut and Copy → Right → Paste, then Cut/Undo and paste inside a
pending cell draft.

Raw evidence: [checks](table-clipboard-native/check.txt),
[focused tests](table-clipboard-native/focused-tests.txt),
[repository checks](table-clipboard-native/check-repo.txt),
[release build](table-clipboard-native/release.txt), and
[environment/source digest](table-clipboard-native/environment.json).
The [native result record](table-clipboard-native/native-results.json)
records case coverage and source assertions. An earlier desktop-control app
launch stalled for roughly 585 seconds; it is excluded from application latency
claims. This pass records behavior, without a new performance measurement.

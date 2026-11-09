# Native file navigation and writing-only validation

Observed on 2026-11-04 with macOS 26.7.1 (25G241), Apple silicon,
Rust 1.95.0, and Iced 0.14. The uncommitted implementation was based on
`1c9f42bc2ebe10925c2f080880b471c597412dc2` on
`codex/minimal-file-workspace`. [Environment and executable/source hashes](environment.json)
identify both tested builds. These observations describe this environment;
[current behavior and limits](../../writing-workflows.md#files-and-folders) live separately.

## Method and observations

A separate **Dosty Files Validation** bundle used a temporary `DOSTY_CONFIG_DIR`
and a synthetic Writing folder with Drafts, Ideas, and Archive subdirectories.
Existing application instances and unsaved user documents were preserved.
The native pointer and keyboard drove the UI; files were then read independently
to verify location and exact saved bytes. The [saved fixture](fixture.md) contains
no private writing.

The initial debug build exercised folder collapse/expand, opening a document,
hover/context actions, dragging a file into Archive, Undo move, Source editing,
the New-document unsaved guard and Escape cancellation, F2 rename with unsaved
writing, keyboard New folder, writing-only mode, and bottom-edge footer access.
A stale old-path tooltip was observed after moving/renaming and corrected before
the final release pass.

The final optimized build repeated document opening, drag/drop, Undo move,
Source paste, New-document guard cancellation, and a second drag while dirty.
The dirty marker and unsaved text survived the move. Text Undo followed by Save
restored the fixture byte for byte at its new Archive location, independently
verified by SHA-256. Live mode, writing-only mode, Escape, and footer recovery
were checked again. No routine save toast interrupted writing.

[File navigation capture](native-files.jpg) shows the final release with the
active moved document, collapsible tree, and footer. [Writing-only capture](native-focus.jpg)
shows the same document with application controls hidden. Captures include the
native macOS window frame and capture indicator; those are outside Dosty's
application layout.

## Scope

The automated widget regression additionally exercises the drag threshold,
closed-folder hover expansion, focus-task delivery, repeated moves, and document
identity/selection/history invariants. File-operation tests cover conflicting
names, saved baselines, checkpoints, references retained by undo, symbolic links,
and bounded discovery. Those are synthetic checks, not claims of OS-level
coverage for every failure.

Windows/Linux, native edge autoscroll in a large tree, cross-volume gestures,
all filesystem failure modes, CJK composition, and full screen-reader workflows
were not validated in this pass. File moves intentionally remain on one
filesystem and stop when checked references cannot preserve their meaning.

## Native quiet follow-up — 2026-11-06

An isolated **Dosty Native Quiet** bundle used the optimized executable with
SHA-256 `49c3db6c45a4764ad1d438d1ea14813c8f32a435148ee303f15c290ca977878e`,
on the same macOS 26.7.1 (25G241) environment. Its configuration and synthetic
essay lived under a separate temporary directory. Existing applications and
user documents were preserved. This was an uncommitted Native quiet build based
on the revision above; the captures linked earlier describe the older footer UI.

Native pointer/keyboard observations confirmed that typing hid titlebar controls
while retaining the filename and modified symbol. Returning to the titlebar
revealed controls and opened temporary Files navigation. Selecting Outline and
pinning it placed navigation beside the document. Writing-only mode hid the
panel and controls; Escape restored the pinned panel. Clicking the modified
symbol saved the test file and restored the checkmark. The file's changed bytes
were independently confirmed on disk.

F10 and arrow keys opened Writing preferences. Turning off word count left
reading time visible, and the independently read preferences file retained those
choices with no document or folder history. The isolated test process was then
stopped. Source/widget tests and the software preview harness additionally cover
fade reversal, stable geometry, long filenames, save completion after later edits,
failure-state retention, narrow windows, and theme presentation. The final source
also keeps contextual file/footnote actions in document menus when writing hides
their notices and retains Files keyboard focus through pinning. Those small
follow-ups were made after this native observation.

These observations do not establish animation frame timing, fullscreen or window
management behavior, complete accessibility, IME behavior, or Windows/Linux
native integration. Software previews do not include native window decorations.

### Bottom statistics and compact titlebar — 2026-11-06

A subsequent isolated **Dosty Alignment** bundle used executable SHA-256
`05db3675b372f87d8af694c480c588c6be7c498ef87d9ef0e09bf1ecd45b9b08` with Midnight, a separate temporary
configuration directory, and a synthetic document. The compact macOS control row
places custom controls in the same 30-point top region as the native traffic
lights. Clicking the navigation control at its new position opened Files.
Switching to Outline and pinning it kept the statistics centered within the
remaining writing column. Typing faded titlebar controls while the bottom word
count updated and reading time remained visible. The test process was stopped
without interacting with user documents.

The native capture indicator obscured the traffic lights in captures, so this
pass established the custom control position and hit testing rather than a
pixel comparison against those system buttons. Software previews covered wide
and narrow Paper, Midnight, and Sage layouts. A widget geometry regression checks
that fading controls, pinning navigation, and disabling statistics retain the
writing viewport and statistics placement. This pass does not add fullscreen,
Windows/Linux, or accessibility coverage.

## Independent sidebars and native menus — 2026-11-06

The isolated **Dosty Sidebars** bundle used Paper, a temporary configuration,
and a synthetic Writing folder. The optimized sidebar build had executable
SHA-256 `26e114056d13af09feab890eb8f436a873e8355fbcc44d8734548552fde2e095`.
The final native-command ordering fix used executable SHA-256
`e8fd943fd1573fca0ef541598f4eff2ec7c6ad5380608942172b63e0a05329f2`.
Both were uncommitted builds on the branch and base revision recorded above,
on the same macOS environment. Existing application instances and user documents
were preserved.

Native observation confirmed the system File, Edit, Format, View, Window, and
Help menus after launch. Open Folder revealed the left workspace. Files and
Outline opened independently, and their titlebar toggles retained them without
pin controls. Heading jumps, typing, and Escape left the panels open. Hide
Sidebars hid and restored the arrangement. The header and sidebars shared one
background; statistics remained centered below the writing column. Typing faded
titlebar controls while retaining the filename and modified symbol.

The document-name menu switched to Source and showed its active label. Native
Select All and Undo restored Unicode text. Titlebar Format worked with keyboard
and pointer input, and undo restored the original content. During menu
integration, native Quit displayed Save/Discard/Cancel; Cancel retained the
unsaved buffer. The test instance was subsequently closed without saving its
temporary changes.

Rapid native Undo followed by Save initially exposed a stale-snapshot ordering
bug. After the fix, the final optimized build repeated the sequence and returned
to the saved checkmark. Independently reading the saved file confirmed the
original bytes, SHA-256
`3e7b4bc5a63c238f81b15a987a107c43bcd06ceedeb3a741329add4846ad9aa8`.
The final test application quit cleanly. The widget regression also covers two
queued Undos, a mode switch, and an actual save in both Write and Source modes.

These passes do not establish animation timing, fullscreen behavior, native
Windows/Linux integration, IME behavior, or full accessibility coverage. The
native capture indicator obscured the system window buttons. Software previews
additionally covered wide and narrow layouts and each built-in theme.

## Inline Outline filter and workspace availability — 2026-11-06

An isolated **Dosty Outline** bundle used Midnight and executable SHA-256
`4e12400ec29ace3f5387f0616503de31375d6988b4907c69f287306b9d9a0888`,
with a temporary configuration and a synthetic document containing third- and
fifth-level headings. This uncommitted build used the branch, base revision, and
macOS environment recorded above. Existing application windows and user writing
were preserved.

Native pointer/keyboard observation confirmed that an empty Outline showed no
search or fold controls, and a standalone document had no Files toggle or panel.
Opening the heading document showed compact header icons, shallow hierarchy
indentation, and full-row selection. Search focused an inline input; typing
`sea` left its matching heading and parent visible while the document remained
saved. Escape cleared the filter. Collapse All and Expand All changed the tree
without changing document text, and Cmd+Shift+J focused the inline filter.

Opening the synthetic Writing folder revealed Files and its titlebar toggle.
Closing that workspace removed both while retaining the saved document and its
Outline. The application then quit cleanly. Independently reading the fixture
confirmed unchanged bytes. Automated regressions additionally cover reverse
UTF-8 selection, fold retention, Read/Source behavior, keyboard navigation,
workspace scan failures, and tools appearing/disappearing with headings.

This pass does not establish Windows/Linux native behavior, IME or screen-reader
coverage, animation timing, or application startup performance. Software
previews cover light/dark filtering and empty-heading states; they exclude native
window decorations.

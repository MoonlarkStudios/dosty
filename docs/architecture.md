# Architecture

Dosty is one Cargo package with a library and a thin executable calling
`dosty::run()`. That entry point is the library's only public export.
Implementation modules are private or crate-visible.

## Responsibilities and dependencies

| Group | Owns | Dependencies within the application |
| --- | --- | --- |
| [app](../src/app) | State, messages, commands, views, and native startup | Coordinates all groups |
| [document](../src/document) | Authoritative Markdown, transactions, source selections, projection, and edit plans | Other document modules |
| [editor](../src/editor) | Native buffer, rich layout, input, navigation, and viewport | Document, theme, image cache, and application messages |
| [theme](../src/theme) | Theme definitions, resolved semantic tokens, and widget styles | Other theme modules |
| [io](../src/io) | Persistence, recovery, assets, image loading, export, and native clipboard | Document data and limits |

The document layer uses Iced types for native selection conversion, code actions,
and lazy Read parsing. Editor widgets emit application interaction messages.
These modules are specific to this application. IO returns completion values;
`App` routes them into its message loop.

`App` owns the open document and presentation state. Its workflow children share
private application state; the `app` root re-exports the messages widgets need.
`Document` keeps its CRDT, source, parsed blocks, statistics, revision, and change
windows private. Callers receive shared references or slices. Only document
methods mutate authoritative state and update its derived caches.

## Continuous editing

The editing path is source → projection → native buffer and measured layout.
User actions map back to source edits and enter `Document`'s transaction path.
Markdown remains authoritative; the projection, Read representation, native
content, and layout are derived state.

Live editing uses one document-wide native buffer and one source selection.
`RichEditor` supplies rich geometry, hit testing, selection drawing, scrolling,
and input-method positioning over that buffer. `RichLayout` supplies the same
measured geometry to drawing, caret placement, and navigation. Drawing does not
parse Markdown. The optional Read renderer is separate and initialized lazily.
Write lays out its text in a centered rectangle within the scrolling pane.
Text, caret, and cell-draft coordinates use that rectangle. The scrollbar and
wheel input use the pane bounds. Resizing the margins retains the cached layout
while the text column stays the same width.

Blank and filled Write rows use the same paragraph line height. The placeholder
shares the first row's origin and typography. The insertion bar uses the row's
typing size, centered within the row. Drawing and input-method placement use the
same rectangle, rounded to physical pixels at the current window scale.

Source/display mappings use UTF-8 byte offsets. Hidden syntax occupies no space;
images are atomic objects, and tables retain structural mappings. Both selection
endpoints and their direction survive mode switches and edits across rich blocks.

Contextual disclosure reveals the active heading prefix and innermost inline
span, including the gap immediately after its closing delimiter. Write mode
uses the same size and font for inline delimiters before and after recognition.
Disclosure preserves source selection, wrap affinity, preferred horizontal caret
position, and a source-based viewport anchor through local reflow. An offscreen
caret does not force the viewport to jump when focus or disclosure changes.

Open document sessions store Write mode folds against stable heading positions.
The active layout height index omits folded body units while the projection and native buffer
retain their complete text and mappings. A source selection meeting a folded
body opens it before layout, so editing operates on a visible source range.

## Selection, formatting, and history

Source selections retain distinct anchor/head positions. Visual wrap affinity,
source/display affinity, and CRDT insertion affinity are separate concepts.
Source-based targets preserve intent across asynchronous work and replica edits.
Local generations identify derived-cache revisions, not document identity or
causal history.

Native edits, formatting, rich-text repair, accepted proposals, imports, and
undo/redo share the document mutation boundary. IME preedit stays transient;
committed text creates one source transaction. Human and accepted-agent history
are separate and session-local. See [collaboration](collaboration.md) for the
coordinate, stable-target, transaction, and persistence contracts.

## Native accessibility prototype

The opt-in [macOS Source bridge](accessibility.md) belongs to the application
layer. It publishes committed Markdown and checked source selections through
AccessKit; native requests return through the ordinary message and document
transaction path. A narrow pinned `iced_winit` patch owns adapter creation before
window visibility, focus/scale events, snapshot delivery, and adapter teardown.
The existing Source widget still owns native editing and transient IME intent.

The semantic subtree is available only in Source mode without a modal, pending
load, or pending composition. Availability and document revisions invalidate
old accessibility targets. Rich Live/Read semantics and application-wide controls
remain outside this prototype.

## Parsing and performance

The source cache certifies conservative local reparse windows. Projection updates
reuse proven unchanged metadata and immutable mapping/highlight snapshots.
Uncertain boundaries, global Markdown dependencies, and oversized batches use the
general path. An optimization may fall back without dropping or changing CRDT
operations. Predecessor identities prevent applying a local patch to stale state.

Large documents shape resident paragraphs and table rows around the viewport.
Height estimates can refine during scrolling; exact resident geometry remains
the basis for interaction. Native content and its tracked snapshot stay under
one owner so mutations cannot bypass synchronization.

Contiguous buffers, suffix rebasing, global parsing, outline rebuilding, and very
long paragraphs or table rows remain scaling limits. The [performance guide](performance.md)
describes profiling and links to dated measurements.

## Rich blocks and writing tools

Feature contracts live with their behavior documentation:

- [Code](code-editing.md), [tasks](task-lists.md), [strikethrough](strikethrough.md),
  and [footnotes](footnotes.md) retain source-aware editing and continuous selection.
- [Tables](table-editing.md) use directed rectangular selection and a transient
  native cell draft; a successful commit becomes one document transaction.
- [Outline](outline.md) state is local; section moves preserve source and history.
- [Writing workflows](writing-workflows.md) cover checked link/image/quotation
  targets, asset import, Save As, recovery, and HTML export.

## Files

Blocking file work, native rich clipboard access, and image decoding run off the
UI event path with explicit limits. Opening guards against edits until completion.
Saving captures a source/state snapshot; subsequent edits remain dirty.

Application chrome owns independent Files/Outline panel states and a
Hide Sidebars override. Files occupies the left edge whenever it is open, with
or without a project, and shows the tree once a scan succeeds. Outline occupies the right. Its inline heading filter retains document order, ancestor context,
and fold state without creating an application overlay.
Responsive layout docks open panels while preserving a usable writing column,
then prioritizes one dock when space is limited. Formatting is transient. The
fixed-height titlebar keeps navigation, document identity, editing mode, and the
snapshot-aware save indicator visible.
Configurable statistics occupy a small fixed-height gutter below the writing
column. Presentation changes retain source selection and viewport anchoring. Shared sidebar metrics
and semantic surfaces keep the canvas file tree and widget outline consistent.
The workspace name belongs to the sidebar header; the tree renders its children.

On macOS a main-thread `muda` menu owner retains the application menu bar and
attaches it after the first window opens, then synchronizes enabled/checked state.
Its bounded event channel wakes Iced without polling or blocking. Application
commands reuse existing guards; native Edit commands are queued and delivered
once through the focused widget's normal key
bindings. Later native commands wait for those edits before taking actions such
as saving a snapshot. Replacing a document drops undelivered menu requests. This
preserves native editor selection, table drafts, clipboard handling, and modal text fields
without adding a second source mutation path. Other platforms retain the
in-window command menu. The macOS content extends into the transparent native
titlebar while retaining system window buttons.

Startup creates one blank document and reopens the last project on a normal
launch. Explicit file opens use the standalone load path. Preferences retain appearance, independent statistics visibility, writing
typeface, measure, spacing, and text size. The stored typography applies the
writing preferences on startup and on theme or preference changes. Layout
keys include the effective geometry and font. A stale custom theme load cannot
replace a later appearance choice, and preference writes coalesce before a
confirmed window close.

`app/session.rs` holds what must survive when a document leaves the page: its
text and history, selection, path, and saved baseline. `App` keeps one session on
the page and rebuilds view state (projection, editor buffer, table drafts) when a
session is installed. `app/background.rs` keeps outgoing sessions with their image
reference ledger and recovery identity; unsaved ones are never released, clean
ones are bounded. Each unsaved background session gets a recovery copy on the next
scheduler tick, ahead of the page's own, with one write in flight at a time.
`app/trail.rs` records visited documents for Back and Forward; a step moves the
trail only when its document arrives, since opening may be asynchronous or fail.
A never-saved session also carries the folder its first save writes into and
whether the writer named it; that save claims a unique empty file through the
ordinary guarded creation path and then saves over it with an empty expected
baseline, so no separate overwrite check exists.
Images are folder entries distinguished by extension, not documents: the
folder state's document accessors exclude them, so opening, reopening, and
switching never treat one as text. `app/image_preview.rs` loads a single image
through the image store's bounded reader without entering the per-document cache,
which is rebuilt from the projection on every update.
Only closing the window raises the unsaved guard. `app/home.rs` renders Welcome
at startup and on request from an open document. Workspace restoration settles
before the startup actions appear. When Welcome is reopened, its view preserves
the document's widget tree and limits layout, input, and widget operations to the
visible page. The document keeps its session and viewport until the writer
returns or opens another document.

`app/projects.rs` owns explicit setup. `io/project.rs` reads bounded portable
metadata and publishes a new `.dosty/project.toml` without overwriting an existing
manifest. `app/workspace.rs` owns the remembered projects and the active
workspace. `io/workspace_state.rs` stores personal session state and project
recency in the per-user data directory. Existing registries remain readable.
A successful scan adopts the new root and applies its remembered state.
Replacing a document first
records the outgoing selection; loading one restores its clamped selection through
the ordinary sync path. Writes coalesce so an older snapshot cannot land after a
newer one, wait for the stored registry so an early write cannot erase it, and
complete before a confirmed window close. Workspace state is a cache: write
failures are logged, never shown, and every remembered path is revalidated against
the current scan before use.

Quick Open is the switcher. It derives its rows when it opens from the open
project's bounded snapshot, the background documents, and the registry, and scans
the other projects on a cancellable worker task whose results it ignores once
closed or superseded. Its focused query intercepts row navigation locally.
Acceptance reuses file loading, background resumption, or project opening;
cancellation restores the previous widget focus without committing a table draft
or editing the source. Closing discards its index, query, and running scan. Heading queries reuse the current Outline. Project pins live
in the remembered workspace state and follow checked file moves.

The folder workflow owns navigation, expanded paths, bounded scan generations,
and operation identities. Folder discovery returns path-only snapshots from a
worker; stale completions cannot replace a newer scan. The native file-tree
widget owns pointer and keyboard selection, hover disclosure, and drag feedback,
then emits application actions. Batch moves check destination names before
publishing individual moves on a blocking worker. Each item uses the same
directory locks, checkpoint guards, and reference checks as a single move.
The application remaps every completed item and keeps the group together for
organization undo.
Moving an active file updates its path without reinstalling its document or
discarding history. Recovery identities rotate when that path changes. See
[files and folders](writing-workflows.md#files-and-folders) for limits and behavior.

Find in Folder searches the same workspace paths on a bounded worker. The
worker uses bounded snapshots of the active file and unsaved background files. A selected
result goes through the ordinary opening and save guards, and its source
digest is checked before selecting its byte range. Query changes and dialog
dismissal cancel the worker.

Link suggestions use the current folder snapshot for file paths and a bounded
worker read for target headings. The **Linked from** action scans saved
workspace files for explicit Markdown links on a cancellable worker. Resolved
destinations use the ordinary document opening path, including resumption of
unsaved background sessions. The destination document supplies the heading or
validates a backlink's source offset against the scanned bytes.

Markdown/checkpoint publication uses cooperative locks, checked baselines, atomic
writes, and a recovery journal. Conflicts preserve files and surface an error.
Each reopen uses a fresh writer identity. See [persistence and recovery](collaboration.md#persistence-and-recovery)
and [security boundaries](../SECURITY.md).

## Themes

Theme definitions resolve into semantic tokens and an Iced theme on load or
switch. `App` stores the `ResolvedTheme`; its theme callback returns that stored
handle. Widgets, highlights, and canvas drawing consume the same tokens through
centralized styles. See [appearance](design-system.md).

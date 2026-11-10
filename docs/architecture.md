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

Source/display mappings use UTF-8 byte offsets. Hidden syntax occupies no space;
images are atomic objects, and tables retain structural mappings. Both selection
endpoints and their direction survive mode switches and edits across rich blocks.

Contextual disclosure reveals the active heading prefix and innermost inline
span. It preserves source selection, wrap affinity, preferred horizontal caret
position, and a source-based viewport anchor through local reflow. An offscreen
caret does not force the viewport to jump when focus or disclosure changes.

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
Hide Sidebars override. Files occupies the left edge only after a workspace scan
succeeds; Outline occupies the right. Its inline heading filter retains document order, ancestor context,
and fold state without creating an application overlay.
Responsive layout docks open panels while preserving a usable writing column,
then uses overlays when space is limited. Formatting is transient. Editing modes
live in document actions. The fixed-height titlebar keeps navigation, document
identity, and the snapshot-aware save indicator continuously visible.
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

Startup creates one blank document, then reopens the last workspace unless a
document or folder argument was given. Preferences retain appearance, independent
statistics visibility, and writing typeface. The typeface is applied to the stored
resolved typography on startup, appearance changes, and preference changes;
layout/cache metrics include it so text is reshaped on a switch.

`app/workspace.rs` owns remembered workspaces; `io/workspace_state.rs` stores them
in the data directory. A successful scan of a new root adopts it as the current
workspace and applies state read before the scan. Replacing a document first
records the outgoing selection; loading one restores its clamped selection through
the ordinary sync path. Writes coalesce so an older snapshot cannot land after a
newer one, wait for the stored registry so an early write cannot erase it, and
complete before a confirmed window close. Workspace state is a cache: write
failures are logged, never shown, and every remembered path is revalidated against
the current scan before use.

Quick Open owns an ephemeral filename/path index derived from the existing bounded
workspace snapshot. Its focused query intercepts result navigation locally.
Acceptance reuses guarded file loading; cancellation restores the previous widget
focus without committing a table draft or editing the source. Workspace changes
invalidate it, refreshes update it, and closing discards its index and query.

The folder workflow owns navigation, expanded paths, bounded scan generations,
and operation identities. Folder discovery returns path-only snapshots from a
worker; stale completions cannot replace a newer scan. The native file-tree
widget owns pointer/keyboard interaction, hover disclosure, and drag feedback,
then emits application actions. File organization runs on blocking workers under
ordered directory and checkpoint locks, with destination and reference guards.
Moving an active file updates its path without reinstalling its document or
discarding history. Recovery identities rotate when that path changes. See
[files and folders](writing-workflows.md#files-and-folders) for limits and behavior.

Markdown/checkpoint publication uses cooperative locks, checked baselines, atomic
writes, and a recovery journal. Conflicts preserve files and surface an error.
Each reopen uses a fresh writer identity. See [persistence and recovery](collaboration.md#persistence-and-recovery)
and [security boundaries](../SECURITY.md).

## Themes

Theme definitions resolve into semantic tokens and an Iced theme on load or
switch. `App` stores the `ResolvedTheme`; its theme callback returns that stored
handle. Widgets, highlights, and canvas drawing consume the same tokens through
centralized styles. See [appearance](design-system.md).

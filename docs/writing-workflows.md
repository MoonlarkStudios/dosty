# Writing workflows

**Help → The Quiet Hour** opens a fresh editable copy of
the bundled essay and image under the configuration directory's `samples/`
folder. Copies work offline and do not replace one another. Save As moves a copy
into the writer's folder.

## Welcome

When there is no document to resume, Dosty opens to Welcome. Choose **New Document**
or press Cmd/Ctrl+N to start a standalone draft. **Open Markdown File…** edits a file
directly. The Projects section shows recent folders with their paths and offers
**New Project…** and **Open Project…**. Dosty waits for any remembered document to
load before showing the Welcome actions. Document controls appear in the editor.

To revisit Welcome while writing, choose **View → Welcome** on macOS or **File → Welcome**
in the in-window menu on Windows and Linux. F10 opens the File command menu on every
platform. **Return to Document** or Escape brings back the same document at its
previous scroll position, with its selection, unsaved edits, and undo history.

## New pages

**New document** (Cmd/Ctrl+N) opens an empty page at once, with no name or
location to choose. The page belongs to a folder straight away: the folder of the
document the writer was in, the folder of the draft they were in, or otherwise
the open project. With none of those it belongs nowhere yet.

Until it is first saved, a new page is a draft. It is titled from its first line
that has any words, without Markdown markers or characters that file names cannot
hold, up to 60 characters; an empty draft is `Untitled.md`. **Name…** in the
document menu (Rename on macOS) sets the name instead, as written, and the title
stops following the text. Naming writes nothing.

Nothing is written until the first save. That save claims the draft's name in its
folder as a new, empty file, numbering the name (`Title 2.md`, up to 99) when it
is taken, and then saves over that file under the same guard as any later save.
An existing file is never replaced. If the save fails, the claimed file is removed
when nothing has been written to it. A draft that belongs nowhere, and **Save
as…**, ask where to save as before. A draft restored from a recovery copy has
forgotten its folder and asks too.

Creating a document from a folder's menu in Files still asks for a name and
creates the file immediately, for when the name must follow a convention from
the start.

## Moving between documents

Replacing the document on the page never asks about unsaved changes. The
outgoing document stays open in the background with its unsaved changes, its
undo history, and its recovery copy, and a notice names it when it had unsaved
changes. Nothing is written to its file until it is saved.

**Back** (Cmd/Ctrl+[) and **Forward** (Cmd/Ctrl+]) move through the documents
visited in the window, up to 64. They are also in File on macOS and in the File
command menu elsewhere. Opening a document from anywhere other than Back or
Forward replaces the forward part of that trail.

A document with unsaved changes returns as it was left. Saving checks the file
or hosted revision against the baseline it was opened from. A clean document
is read from its destination again. If the saved copy is unchanged and its
background session is still open, the session returns with its undo history.
If the saved copy changed, Dosty opens the current contents. A hosted draft
prompts you to reconnect to its workspace when needed. Opening the current
unsaved document keeps its local edits on the page.

Unsaved documents are kept until saved or discarded. Clean background documents
are a convenience: at most 8, and 32 MiB of source, are kept, least recently
used released first. A clean document that was never saved, such as an untouched
new page, is dropped when replaced.

Dosty asks about unsaved changes when closing the window. A single prompt names the unsaved
documents. **Save changes** saves the page, then brings each unsaved background
document to the page in turn and saves it, asking for a name where one was never
chosen; a cancelled or failed save stops there with everything still open.
**Discard** closes without saving any of them. **Cancel** returns to writing.

A file with unsaved background changes, or a folder containing one, cannot be
moved or renamed until that document is saved: only the page's unsaved image
references are relocated with a move.

## Files and folders

**File → Open Markdown File…** (Cmd/Ctrl+O), dropping a Markdown file into Dosty,
and launching with a file path all edit the file directly. If its project is already open, Files can
show the document in the tree. Direct file opens never set up a project or
switch to another project.

### Projects

A project is a folder with ordinary nested folders and Markdown files.
**New Project…** asks for a name and parent folder, previews the destination,
publishes the folder and its manifest together, and opens a blank draft there.
The chosen path becomes visible only when the project is ready. **Open Project…**
(Cmd/Ctrl+Shift+O) chooses a folder. If it already has a project manifest, it
opens immediately. Choose **Use as Project** to initialize an existing folder.
Both setup flows create `.dosty/project.toml`. The manifest holds `version = 1`
and can contain a `name` used in Dosty. The folder name is the default. Setup
keeps existing Markdown files and any existing manifest.

**File → Projects…** and the project-name menu list remembered projects, most
recent first, with locations to disambiguate duplicate names. Each window shows
one project in Files. Quick Open and the document trail can follow a document
into another remembered project. If the open project contains the document, it
stays active. Otherwise, the deepest matching project opens.

Previous Places appear in Projects with their existing state. They can be
opened as before. **Set Up Project…** in the project-name menu adds the manifest
after confirmation. **Remove from Projects** forgets the workspace while keeping
the folder, manifest, and active document. Up to 64 projects can be remembered.
When the list is full, another project opens for the session with a notice.

On normal launch, Dosty restores the last project and its most recent available
document, along with the source selection, expanded folders, and sidebars.
Switching to a remembered project reopens its last document when the active page
is an empty draft or a clean document from the project being left. Unsaved
writing stays on the page when switching projects. An unavailable project is
removed from the list with a notice. Source selections are clamped against any
external edits when a document is reopened.

Recent activity, caret positions, expanded folders, and sidebars live in
`workspaces/` in the per-user
[data directory](identity-migration.md#existing-koan-and-daytrip-data).
`workspaces.toml` holds the remembered roots and the one to resume, and each
`state.toml` records personal state for a root. These files are bounded, written on
workers, and private to the user on Unix. Deleting `workspaces/` clears the local
project list and working state. The Windows uninstaller removes that directory
and keeps preferences and recovery copies.

The Files tree preserves the actual directory hierarchy, including empty folders.
Folders come first, then documents, each in reading order: case-insensitive, with
runs of digits compared as numbers, so `Chapter 2` precedes `Chapter 10`. Quick
Open orders a folder's contents the same way. Documents are shown without their
Markdown extension; a chevron marks a folder and documents carry no glyph.
Discovery reads paths and file types on a worker, not document contents. It finds
documents (`.md` and `.markdown`) and images (`.png`, `.jpg`, `.jpeg`, `.gif`,
`.webp`) case-insensitively, skips symlinks, hidden items, `target`,
`node_modules`, and `*.assets` folders, and stops at 32,768 visited entries,
48 levels, or 8 MiB of retained paths. Partial scans and unreadable subfolders
surface a notice. The open project is rescanned whenever the window regains focus
with nothing else open over the page, so files changed in another program are
there on return; the rows stay in place while it runs. **Refresh folder** does the
same on demand. There is no filesystem watcher, so changes made while Dosty stays
focused wait for one of those. Opening another folder cancels or invalidates an older scan.

Hover or keyboard-focus a row to reveal its action button; right-click,
Shift+F10, and the context-menu key offer the same actions. A delayed hover shows
the full path. Up/Down and Home/End select rows, Left/Right navigates the hierarchy,
Enter/Space opens a file or toggles a folder, and F2 renames. Menus support
Up/Down, Tab/Shift+Tab, Enter, and Escape.

Right-click a row for its file or folder actions, including Rename, Move to,
and Show location. Folder menus also create Markdown documents and folders.
Cmd-click on Mac or Ctrl-click on Windows and Linux adds or removes a row.
Shift-click selects a visible range. Right-click a selection to choose a folder
for the group. The document-name menu also lets a saved standalone file move
to a destination outside the open project.

Drag any selected row to move the group. A folder moves with its descendants.
The drag preview gives the item count and destination. Valid folders highlight,
a short pause expands a collapsed folder, and dragging near the list edges
scrolls it. Drop on **Project root** at the bottom of Files to move to the top
level, even in a full list. Items already in the target folder stay there.
Escape or leaving the application cancels a drag.

A completed move leaves its items selected and offers **Undo move** for the
whole group. Undo checks the reverse destinations and is separate from text
undo. When a file check stops a batch after some items move, Dosty reports what
happened and keeps Undo available for the completed moves. Opening files and
creating a new document [keep the outgoing document open](#moving-between-documents).

Organization runs on workers under ordered directory and document locks. Moves
stay on one filesystem, reject occupied names and folder cycles, and preserve
the saved Markdown/checkpoint baseline independently of active unsaved writing.
Moving the active document briefly blocks source edits while its source and
reference snapshot is relocated. Relative image assets needed by saved, current,
and session-history source are retained before publishing a file move. References
that cannot retain their meaning block the move instead of rewriting Markdown.
Folder moves keep their contained files and checkpoint siblings together and
reject symlinks. Windows cannot rename a folder while files inside it are open, so
there the moved folder's own locks are released just before the rename while both
parent folders stay locked; another Dosty window holding a file inside makes the
move fail with nothing changed. Renames that change only capitalization work on case-insensitive
volumes such as default APFS and NTFS; they pass through a temporary visible name. Incoming links from other documents are not rewritten.

A batch can move up to 128 selected items. Folder moves are bounded to 32,768 entries, 128 directories, 128 Markdown
documents, and 128 MiB of Markdown source; each document retains the ordinary
16 MiB file limit. Asset copying has a shared 128 MiB limit. Visible names are
limited to 240 UTF-8 bytes and cannot contain path separators. Cooperative locks
coordinate Dosty writers; external tools do not participate in those locks.

### Unsaved documents in Files

A document with unsaved changes carries the modified mark at the right edge of
its row, whether it is on the page or open in the background. A collapsed folder
with such a document anywhere inside carries a fainter mark, so folding never
hides unsaved writing; expanding the folder moves the mark to the document. The
mark gives way to a row's action button while the row is hovered or focused.

Unsaved documents that no row can stand for are listed above the tree under
**Not saved yet**, with the folder they belong to: drafts that have never been
saved, and documents outside the open project. With no project open, that is every
unsaved document. Choosing one returns it to the page as it was left; the page's
own entry is shown selected. A draft appears here rather than inside its folder
in the tree, because it has no file there yet.

### Images in a project

Files shows both project documents and supported images. Images appear in Files
and Quick Open under their full file name; other files stay out. Dosty's own
`*.assets` folders remain hidden.

Choosing an image previews it rather than opening it: the page and its document
are untouched. The preview is read and decoded on workers under the same 16 MiB,
8192-pixel, and 64 MiB decoded limits as a document's images, shares their
concurrency bound, and is dropped when it closes.

**Insert into document**, in the preview and in an image's menu in Files, inserts
a reference to the image where it already lives, at the selection, as one undo
step. Nothing is copied. The path is relative to the document's folder, or for a
draft to the folder its first save will use; a page that belongs nowhere must be
saved first. The image's name, without brackets, becomes the description. A path
that crosses to another drive, or holds `<`, `>`, or a line break, is refused.
Read mode does not insert. References that climb out of the document's folder
with `..` display normally, but **Save as…** to another folder stops rather than
relocate them, as it does for any such reference. Blog engines that expect
site-relative paths such as `/images/cover.png` need the inserted path adjusted.

Images cannot be renamed or moved from Dosty: links in other documents are not
rewritten, so a moved image would vanish from every page that uses it. Moving a
folder still moves the images inside it, with that same limitation. Dragging an
image from Files onto the page is not supported; use Insert into document.

## Controls and sidebars

Files and its titlebar toggle are always available. With no project open, the header reads Projects and the body offers **Open Project…**. The project name opens its actions, including Projects, location, refresh, creation, and removal from the list. Dropping into empty tree space targets the project root. Closing Files leaves the document open.

Outline opens independently on the right with its titlebar control, **View → Outline** on macOS, or Cmd/Ctrl+Alt+O. Its toggle keeps it open until you close it. When both panels cannot leave at least 480 logical pixels for the writing area, the most recently opened panel docks by itself. Opening the other switches panels. Widening the window restores both if they were open. Editing, heading jumps, document opens, and Escape preserve the panel arrangement. Its header offers icon controls only when headings exist. Search filters the tree inline, retaining ancestor context; Cmd/Ctrl+Shift+J opens and focuses that filter. Each project remembers whether Files and Outline were open; Hide Sidebars lasts only for the application session.

Cmd/Ctrl+Shift+B toggles Files. Cmd/Ctrl+Shift+F runs **Hide Sidebars**, hiding/restoring the existing arrangement without changing editing mode or control visibility. Escape dismisses the current editing context; it does not restore deliberately hidden panels. These operations retain source selection, undo, and viewport anchoring.

The titlebar groups Files and Quick Open on the left, the filename and save state in the center, and editing mode, Format, and Outline on the right. Controls remain visible while typing. Use the mode dropdown for **Write**, **Source**, and **Read**. The filename menu holds file actions, and Format opens formatting and insertion commands on demand.

On macOS the compact row shares the native titlebar and the system menu bar provides **File, Edit, Format, View, Window, Help** and **Dosty → Settings…**. Native Edit commands use the focused editor/text-field bindings, including table clipboard behavior. Close Window and Quit use Save/Discard/Cancel. Windows/Linux keep system decorations and an in-window File command menu. F10 opens the keyboard-operated command menu on all platforms.

The filename and save indicator stay visible. A check means all changes are saved. New drafts, unsaved changes, saving, and save failure have text labels. Hover for details or click the indicator to save or retry. A save marks its captured snapshot as saved, so later edits remain modified. The failure label remains after the notification is dismissed and clears after a successful save or when another document opens.

Word count and reading time stay at the bottom center of the writing column while typing. A selection shows its prose word count alongside the document reading time. Click the statistics or press Cmd/Ctrl+Alt+I for the document word count, Markdown character count, and reading time. Escape closes the details and keeps the selection. The Writing tab in Settings puts text size, line width, line spacing, a live typeface sample, and the two gutter statistics together. Line width can follow the theme or use the Narrow or Wide measure. Line spacing can follow the theme or use Compact or Relaxed. Source Sans 3 is the default typeface. Writing preferences survive app launches and color theme switches. In Write, typography changes preserve source selection and viewport anchoring. Reading time assumes about 200 words per minute. Changing statistics visibility preserves the writing viewport. macOS View also exposes these preferences; the fallback File menu groups writing preferences and Help/settings on other platforms. Preferences retain no recent-file or folder-session history.

### Quick Open

Cmd/Ctrl+P, **File → Quick Open…**, or the titlebar search icon summons the switcher
over the page, with or without a project open. Choose a document to open it and dismiss the switcher.

With an empty query it shows, in order:

- **Unsaved:** documents open in the background with unsaved changes, marked, and
  never the page itself. Choosing one returns it as it was left.
- **Pinned:** files pinned from their context menu in Files for this project.
  The pins are remembered in Dosty's workspace state.
- **Recent:** up to six recently active documents of the open project, leaving out
  the page and pinned files. The previous document is preselected in Pinned or
  Recent, so Cmd/Ctrl+P then Enter switches back.
- **The open project:** its top-level folders, then its top-level documents. Right
  or Enter steps into the selected folder and Left steps out, returning to the
  folder that was left. Stepping never changes the page.
- **Projects:** the other projects. Choosing one opens it and keeps the switcher open
  on its contents.

Typing searches documents by name and path across the whole library. Matching is
case-insensitive and requires every whitespace-separated term; exact filenames
and filename prefixes rank first, and within a rank recently active documents
lead, then the open project, then other projects. A document in another project can be
matched by its project's name and shows that project beside its folder. Unsaved
documents match by title. Documents are shown by filename without the Markdown
extension. The first 100 rows are shown with a match count when more exist.

Type `#` followed by a space and heading terms to search the current document's
headings, including on a standalone page. Parent titles distinguish repeated
headings. Use Up/Down and Enter to jump in your current editing mode.

The open project is searched from the same bounded snapshot as Files, including
collapsed folders; after external file changes, use Refresh in the project-name
menu. Other projects are scanned on a worker, one at a time, when the switcher
opens, within a shared bound of 65,536 documents; their documents join the
results when that scan finishes. A project that cannot be read is absent from
search. Nothing read for the switcher outlives it: closing discards the query,
the index, and any scan still running. No search history is persisted.

Up/Down selects without moving the query caret; Left and Right step through
folders only while the query is empty. Escape or an outside click restores the
previous focused widget, source selection, and writing viewport. Choosing the
document on the page only closes the switcher. Opening a project from outside the
switcher closes it; a refresh updates its rows. It is unavailable while another
modal or file load is active.

### Find in Folder

Press Cmd/Ctrl+Alt+F to search the open project. Enter a literal, case-sensitive
phrase and press Enter or choose **Search**. Results show an excerpt, relative
filename, and line number. Use Up/Down and Enter, or click a result, to select
the match in its document. Matches in concealed Markdown syntax open in Source.
Switching files keeps unsaved writing open in the background. If a result has
changed since the search, run the search again to update its location.

Search uses the current Files snapshot and includes the active Markdown file
and unsaved background versions of files in the open project. Refresh the folder
to discover newly added files. Search examines up to 4,096 files and 128 MiB of
text, including up to 32 MiB of open buffers, and shows up to 100 matches. The
status reports partial results or skipped files. Reading files runs on a worker
and stops when the query changes or the dialog closes.

## Writing and details

- **Link:** select a phrase and use Cmd/Ctrl+K or the link control. Apply edits
  the destination; Unlink retains the label. Destination-only changes preserve
  label formatting and Markdown titles. Editing a reference-style link normalizes
  it to an inline link. Pasting a web/email URL over selected text creates a link;
  cross-paragraph selections do not become one link.
- **Find file or heading:** when a document is saved in the open project or a
  draft will first save there, open the link dialog and choose **Find file or heading…**, or press Down in the
  Destination field. Filter file names or paths, choose a result, and pick the
  whole file or one of its headings. Up/Down and Enter choose a result. Escape
  returns to the destination. Apply inserts an ordinary relative Markdown link
  such as `[Methods](<chapter-2.md#methods>)`.
- **Open a link:** Cmd/Ctrl-click or use Open in its details. HTTP/HTTPS and email
  use the OS handler. `#heading-slug` navigates within the current file.
  Links to `.md`, `.markdown`, and `.txt` files keep the outgoing document
  open in the background and can jump to a named heading. Heading labels become
  lowercase slugs, or use an explicit ID from `{#id}`. Repeated IDs receive a
  numeric suffix.
- **Linked from:** open the filename menu to find files in the open project that
  link to the current file. Choose a result to switch to that file and jump to
  the link. The panel reads saved Markdown and marks incomplete scans. A
  document edited since the scan returns with its remembered position.
- **Image:** choose a file/URL, paste one copied image file or raster image, or
  drop PNG/JPEG/GIF/WebP at the current source selection. Double-click an image,
  or use Image from its caption/credit, to edit details. Images preserve aspect
  ratio, fit the column without upscaling, and cap height at 520 pixels times
  zoom. GIFs show their first frame. Cropping/resizing overrides are unsupported.
- **Quotation:** use the quotation control to edit Markdown with an author,
  source title, and optional destination. Credits stay inside the blockquote.

Apply validates stable, expected-text targets. Unrelated edits rebase them; an
overlap leaves the dialog open with a conflict and changes no source. Each Apply
is one human transaction and undo group. Deferred paste/import follows stable
targets, preserves later writing, and rejects earlier document sessions. A changed
replacement target collapses instead of deleting intervening text. Completion
does not steal focus from another field or cancel its composition.

Figures and quotations remain portable Markdown:

```markdown
![Description](<essay.assets/content-hash.jpg>)

*A caption.*

Photo: [Photographer](<https:/source.example/photo>)

> A quotation.
>
> — Author, [Source](<https:/source.example/essay>)
```

Controls recognize standalone captions and `Photo:` credits without consuming
following prose. Ordinary Source editing remains available for more elaborate
Markdown. Blank Markdown lines occupy 65% of a body row; all layout paths share
that metric while preserving source selections and viewport anchors.

## Assets and saving

Importing an image requires a first save to establish its base folder; canceling
that save cancels import. Supported original bytes are validated and published
immutably into `<document stem>.assets/` under content-hash filenames. Identical
bytes are reused; conflicting contents or symlink destinations fail. Clipboard
RGBA becomes PNG. Unreferenced assets remain for undo; there is no automatic
deletion. Replicas must carry referenced asset folders separately from text state.

Imported images are bounded to 16 MiB encoded, 8192 pixels per dimension, and
64 MiB decoded. The native clipboard can materialize data before Dosty checks its
size. Decoding, copying, clipboard reads, and export run on workers. Live network
images have separate bounded loading and a 128 MiB document cache.

Save As copies current and session-history relative assets before publishing
Markdown, preserving source paths and undo references. Identical destinations
are reused; conflicting/missing current assets, symlink destinations, `..` escapes,
or copies over 128 MiB stop relocation. Absolute/file URLs remain absolute. A first
save establishes a base without relocating manually written references. Broken
historical references are skipped; existing undo assets are carried forward.
A failed relocation may leave unused assets but does not change the active path
or publish Markdown.

The session ledger retains image and hyperlink destinations, bounded together
to 16 MiB of URI text and 65,536 references. If
exhausted, ordinary saving remains available, but relocation requires saving and
reopening to begin a new undo session. No CRDT operations are dropped. Source,
checkpoint, external-change, permissions, and symlink guards are described in
[persistence](collaboration.md#persistence-and-recovery).

## Recovery and export

Dirty documents checkpoint after two idle seconds or 30 seconds of continuous
writing, with a one-second scheduler. Background serialization/IO stores full
compatible CRDT state, the save destination and baseline, the document name,
and directional selection in one checksummed recovery file. For hosted drafts
this includes the workspace URL, post ID, saved revision, and both saved and
edited post details. Title, section, and cover edits trigger checkpoints on
their own. Recovery files are bounded to 112 MiB and use private temporary
permissions. The recovery directory is private on Unix.

Startup announces available copies. **Help → Recovery Copies…** lists them by name and
recency. Open copy keeps the outgoing document open in the background. Save
checks the recovered baseline against the file or hosted workspace. A restored
hosted draft prompts for the access key when it needs to reconnect. If another
device saved a newer revision, Dosty keeps the local edits open and can save
them as a new hosted draft, including the post details. **Keep recovery copies**
in the close dialog checkpoints all unsaved documents before closing the window.
Corrupt, oversized, or unsupported copies remain on disk with controlled errors.

A clean save or explicit discard retires this session's copy after any in-flight
write finishes. A recovered document gets its own recovery identity. Earlier
copies remain in the recovery folder until explicitly removed.
Moving a document rotates its recovery identity so late writes at the old path
cannot mark the new path as checkpointed; the prior copy is retained until its
replacement is saved or the session is explicitly retired.
Undo stacks are session-local. Up to 30 seconds plus scheduler/write time of work
can be lost after a crash; recovery complements ordinary saving and backups.

**Export HTML** captures a source revision and embeds local images, captions,
credits, alt text, tables, and typography into one file. Missing/invalid local
images stop export; embedded payloads are capped at 128 MiB. HTTP/HTTPS images
remain remote and are not fetched by export. Raw HTML is escaped, unsupported
schemes are disabled, and a restrictive CSP blocks scripts. Export uses atomic
publication. PDF/office export is unsupported.

## Sample image provenance

`assets/writing/coast.png` is an illustrative image generated with OpenAI's image
generation tool. The sample credits it as generated; it asserts no real
photographer or location. The checked-in asset needs no generation tools at
build/runtime. See [native workflow observations](validation/native-writing.md)
and [historical measurements](performance/writing-workflows.md) for retained evidence.

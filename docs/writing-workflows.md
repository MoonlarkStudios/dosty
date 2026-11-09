# Writing workflows

**Help → The Quiet Hour** opens a fresh editable copy of
the bundled essay and image under the configuration directory's `samples/`
folder. Copies work offline and do not replace one another. Save As moves a copy
into the writer's folder.

## Files and folders

Startup opens one blank document with both sidebars hidden. **File → Open…**
opens a single document; **Open folder…** or Cmd/Ctrl+Shift+O adds optional folder
navigation without replacing the current document. Dosty stores no recent-file
history, open-folder session, or automatic document-reopen state. Existing legacy
recent-file preferences are removed when preferences are rewritten.

The Files tree preserves the actual directory hierarchy, including empty folders.
Discovery reads paths and file types on a worker, not document contents. It finds
`.md` and `.markdown` case-insensitively, skips symlinks, hidden items, `target`,
`node_modules`, and `*.assets` folders, and stops at 32,768 visited entries,
48 levels, or 8 MiB of retained paths. Partial scans and unreadable subfolders
surface a notice. **Refresh folder** discovers external changes; there is no
filesystem watcher. Opening another folder cancels or invalidates an older scan.

Hover or keyboard-focus a row to reveal its action button; right-click,
Shift+F10, and the context-menu key offer the same actions. A delayed hover shows
the full path. Up/Down and Home/End select rows, Left/Right navigates the hierarchy,
Enter/Space opens a file or toggles a folder, and F2 renames. Menus support
Up/Down, Tab/Shift+Tab, Enter, and Escape.

Folder actions create Markdown documents and folders, rename or move items below
the open root, and show their location. The root also offers Refresh and Close;
closing navigation leaves the active document open. The titlebar's document-name
menu offers Rename, Move to, and Show location for a saved standalone file. Move
to can select a destination outside the open folder. There is no delete,
multi-selection, or general-purpose file-manager view.

Dragging starts after a small movement threshold. A preview follows the pointer,
valid folders highlight, and a 600 ms pause expands a collapsed destination.
Escape or leaving the application cancels a drag. A completed move briefly
highlights the destination and offers **Undo move** in a contextual notice. Undo move is a
checked reverse move and may fail if the filesystem has changed; it is separate
from the document's preserved text undo/redo. Opening files and creating a new
document use the existing Save/Discard/Cancel guard.

Organization runs on workers under ordered directory and document locks. Moves
stay on one filesystem, reject occupied names and folder cycles, and preserve
the saved Markdown/checkpoint baseline independently of active unsaved writing.
Moving the active document briefly blocks source edits while its source and
reference snapshot is relocated. Relative image assets needed by saved, current,
and session-history source are retained before publishing a file move. References
that cannot retain their meaning block the move instead of rewriting Markdown.
Folder moves keep their contained files and checkpoint siblings together and
reject symlinks. Renames that change only capitalization work on case-insensitive
volumes such as default APFS and NTFS; they pass through a temporary visible name. Incoming links from other documents are not rewritten.

Folder moves are bounded to 32,768 entries, 128 directories, 128 Markdown
documents, and 128 MiB of Markdown source; each document retains the ordinary
16 MiB file limit. Asset copying has a shared 128 MiB limit. Visible names are
limited to 240 UTF-8 bytes and cannot contain path separators. Cooperative locks
coordinate Dosty writers; external tools do not participate in those locks.

## Controls and sidebars

Files and its titlebar toggle are available only within a folder workspace. A successful folder open reveals Files on the left without replacing the current document. The folder name is its header and opens workspace actions: switch folders, create a document or folder, refresh, show its location, reveal it in Finder/the file manager, or close the workspace. The folder root is not repeated as a tree row; dropping into empty tree space targets the root. Its titlebar toggle keeps it open until you close it. Closing it leaves the document open.

Outline opens independently on the right with its titlebar control, **View → Outline** on macOS, or Cmd/Ctrl+Alt+O. Its toggle keeps it open until you close it. When both panels cannot leave a writing column of at least 480 logical pixels, Files stays docked and Outline overlays the document. Editing, heading jumps, document opens, and Escape preserve the panel arrangement. Its header offers icon controls only when headings exist. Search filters the tree inline, retaining ancestor context; Cmd/Ctrl+Shift+J opens and focuses that filter. Visibility lasts only for the application session.

Cmd/Ctrl+Shift+B toggles Files. Cmd/Ctrl+Shift+F runs **Hide Sidebars**, hiding/restoring the existing arrangement without changing editing mode or control visibility. Escape dismisses the current editing context; it does not restore deliberately hidden panels. These operations retain source selection, undo, and viewport anchoring.

The titlebar contains Files, Format, Outline, the document name, and save state. Controls remain visible while typing, without requiring pointer movement. Format opens formatting/insertion commands on demand. The document-name menu contains file actions and **Write / Source / Read**; Source and Read show a small active-mode label. There is no ribbon or Focus control.

On macOS the compact row shares the native titlebar and the system menu bar provides **File, Edit, Format, View, Window, Help** and **Dosty → Settings…**. Native Edit commands use the focused editor/text-field bindings, including table clipboard behavior. Close Window and Quit use Save/Discard/Cancel. Windows/Linux keep system decorations and an in-window File command menu. F10 opens the keyboard-operated command menu on all platforms.

The filename and save indicator stay visible: an empty circle means never saved, a filled dot means modified, a check means saved, a save icon means saving, and a warning means the last save failed. Hover for details; click to save. Save completion only marks its captured snapshot as saved, so later edits remain modified. Dismissing an error notification leaves the warning until a successful save or different document.

Word count and reading time stay at the bottom center of the writing column while typing. Settings configures them independently and offers a Typeface preference: system sans-serif by default, or Newsreader serif. Typeface changes preserve the source selection and writing viewport, and remain selected when switching color themes. Reading time assumes about 200 words per minute. Changing statistics visibility preserves the writing viewport. macOS View also exposes these preferences; the fallback File menu groups writing preferences and Help/settings on other platforms. Preferences retain no recent-file or folder-session history.

### Quick Open

Cmd/Ctrl+P, **File → Quick Open…**, or the Files search icon opens filename/path search within the current workspace. It searches the same bounded Markdown path snapshot as Files, including collapsed folders. Matching is case-insensitive and requires every whitespace-separated term; exact filenames and filename prefixes rank first. The first 100 results are shown with a match count when more exist. Narrow the query to reach other matches. After external file changes, use the folder-name menu’s Refresh action to update the snapshot.

Up/Down selects a result without moving the query caret; Enter opens it through the existing file-loading and Save/Discard/Cancel guards. Escape or an outside click restores the previous focused widget, source selection, and writing viewport. Opening the active file only closes the picker. A workspace change closes it; a refresh updates its results. It is unavailable while another modal or file load is active. Closing it discards the query and candidate index. No search history, recents, or workspace sessions are persisted.

## Writing and details

- **Link:** select a phrase and use Cmd/Ctrl+K or the link control. Apply edits
  the destination; Unlink retains the label. Destination-only changes preserve
  label formatting and Markdown titles. Editing a reference-style link normalizes
  it to an inline link. Pasting a web/email URL over selected text creates a link;
  cross-paragraph selections do not become one link.
- **Open a link:** Cmd/Ctrl-click or use Open in its details. HTTP/HTTPS and email
  use the OS handler. `#heading-slug` navigates locally. Relative `.md`, `.markdown`,
  and `.txt` links use Dosty's unsaved-document guard. Unsupported schemes produce
  a message. Cross-file fragments and hover cards are unsupported.
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
compatible CRDT state, saved-file baselines, name/path, and directional selection
in one checksummed recovery file. Each session gets a fresh random filename and
writer identity. Recovery files are bounded to 112 MiB and use private temporary
permissions; the recovery directory is private on Unix.

Startup announces available copies. **Help → Recovery Copies…** lists them by name and
recency; Open copy uses Save/Discard/Cancel guards. Restore preserves the original
saved-file baseline, so Save still detects external changes. Save As keeps both.
Corrupt, oversized, or unsupported copies remain on disk with controlled errors.

A clean save or explicit discard retires this session's copy after any in-flight
write finishes. Earlier/crashed/restored copies remain until explicitly removed
from the recovery folder. Other windows' copies are not deleted automatically.
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

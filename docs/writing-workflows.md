# Writing workflows

The sidebar's **The quiet hour · sample** action opens a fresh editable copy of
the bundled essay and image under the configuration directory's `samples/`
folder. Copies work offline and do not replace one another. Save As moves a copy
into the writer's folder.

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

The session ledger is bounded to 16 MiB of URI text and 65,536 references. If
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

Startup announces available copies. **Recovery copies** lists them by name and
recency; Open copy uses Save/Discard/Cancel guards. Restore preserves the original
saved-file baseline, so Save still detects external changes. Save As keeps both.
Corrupt, oversized, or unsupported copies remain on disk with controlled errors.

A clean save or explicit discard retires this session's copy after any in-flight
write finishes. Earlier/crashed/restored copies remain until explicitly removed
from the recovery folder. Other windows' copies are not deleted automatically.
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

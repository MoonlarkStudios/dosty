# macOS Source accessibility

Dosty has an opt-in native accessibility prototype for its **Markdown Source**
editor. It exposes one labeled multiline text input, committed Markdown,
directed selection, and focus/selection/value actions. The ordinary launch path
does not enable it. Complete application screen-reader support remains a
[release blocker](evaluation.md#remaining-release-work).

## Native bridge and ownership

The pinned Iced widget interface has no accessibility tree hook. Its native
window callback exposes raw handles, but lacks the safe Winit window/event-loop
access that AccessKit needs before first visibility. The narrow
[vendored window-runtime patch](../vendor/README.md#iced-window-runtime-0141)
provides that lifecycle integration without replacing the editor or renderer.
It installs the adapter while the window is hidden, forwards focus and scale
events, publishes application snapshots, and drops the adapter on close.

The application owns the semantic snapshot and handles native actions through
its ordinary update path. The existing Source widget owns editing and transient
IME intent; `Document` remains the sole authority for committed Markdown and
history. Logical editor bounds come from the widget operation and are scaled
at the bridge root for native physical coordinates. Platform-specific native
integration stays inside upstream AccessKit, using safe Rust APIs from Dosty.

The bridge uses AccessKit 0.21.1, accesskit_winit 0.29.2 and accesskit_macos
0.22.2 with the existing pinned Iced/Winit stack. Upstream contracts for
[tree updates](https://docs.rs/accesskit/0.21.1/accesskit/struct.TreeUpdate.html),
[text positions](https://docs.rs/accesskit/0.21.1/accesskit/struct.TextPosition.html),
and the
[Winit adapter lifecycle](https://github.com/AccessKit/accesskit/blob/5801c4a49ff9b2c9ec3f77b3c31272d0da4c5cd3/platforms/winit/src/lib.rs)
explain the cross-module boundary. Cargo manifests and the lockfile define the
selected dependency versions.

## Run an isolated prototype

Use a synthetic document and separate configuration. Preserve existing Dosty
windows and unsaved documents. From the repository root on macOS:

```sh
accessibility_dir=$(mktemp -d "${TMPDIR:-/tmp}/dosty-accessibility.XXXXXX")
mkdir -p "$accessibility_dir/config"
cat > "$accessibility_dir/Accessibility.md" <<'MARKDOWN'
# Accessibility sample

Hello 日本語, 中文, 한국어, café, é and 🧑‍💻.

Second paragraph with **bold** and [a link](https://example.com).
MARKDOWN
DOSTY_CONFIG_DIR="$accessibility_dir/config" \
DOSTY_ACCESSIBILITY_PROTOTYPE=1 \
mise exec -- cargo run --locked --release -- "$accessibility_dir/Accessibility.md"
```

The flag must be exactly `1`. The prototype starts in Source mode and initially
focuses its existing editor. The environment variable is read at startup and
is not persisted in preferences. Other platforms do not enable this bridge.

For native inspection, locate the new window and its **Markdown source** text
area with Accessibility Inspector or an accessibility API client. Check exact
text, UTF-16 ranges, selection and focus. In an approved VoiceOver validation
session, check that it reaches and reads the text area, follows typing and
selection, and announces committed text. Restore desktop and VoiceOver settings
as found. Record observed operations separately from automated assertions;
API inspection alone cannot establish spoken output.

## Text and action contract

The native tree has one window root and one multiline input labeled **Markdown
source**. Its value is exact committed Markdown. One text run represents each
physical source line, including empty documents and the final empty line after
a break. Selectable character lengths are UTF-8 byte lengths of Unicode
grapheme clusters, matching Source character motions. CRLF and LFCR are atomic
line endings; LF and lone CR retain their original bytes.

The macOS adapter maps native UTF-16 ranges to text-run positions; Dosty maps
those positions to UTF-8 source bytes. Anchor and head remain distinct, including
reverse selections. A native caret cannot enter a combining sequence, joined
emoji, modifier cluster, or paired line ending. If an existing source endpoint
is inside one, the snapshot omits its selected range without moving the source
selection. Invalid positions, run IDs and character indices are rejected.

AccessKit stores each selectable character's byte length in a `u8`. If any
grapheme exceeds 255 bytes, the prototype preserves the exact full value and
Focus/SetValue actions but omits text runs, selected ranges and SetTextSelection.
It does not invent caret stops or truncate source.

| Action | Behavior |
| --- | --- |
| Focus | Focus the existing Source editor through its widget operation |
| SetTextSelection | Restore the exact directed source selection without editing text |
| SetValue | Replace the full source through one validated human undo group |

Setting the current value again is a no-op: it preserves source, selection,
undo/redo state and history grouping. Failed replacements retain the existing
source and selection through the ordinary transaction error path.

Native callbacks enqueue bounded requests without mutating application state
reentrantly. A full queue can drop a request. Editor/run IDs change with source
revision, document identity, replica session or availability; selection and
focus updates alone retain them. Requests for old IDs, unsupported actions or
mismatched payloads are rejected. Current loading, mode and modal guards are
rechecked at delivery.

The Source subtree is hidden in Live/Read, while loading, under an application
modal, and while the Source widget holds pending IME intent. A cleared, empty
preedit can precede a commit, so it remains pending until the native wrapper
retires its target; it does not briefly re-enable accessibility actions.
Returning to available Source mode publishes new IDs, preventing old requests
from becoming valid again. Committed text remains authoritative throughout.

## Prototype limits

- Live/Read content, rich heading/table semantics, formatting controls, search,
  sidebars and application dialogs are not exposed. Native window chrome and
  file dialogs can have their own accessibility. Hiding Source under a modal
  does not make the modal accessible.
- Glyph positions, character widths, visual wrapping and word boundaries are
  absent. Spatial text queries, magnifier/cursor geometry, screen-reader
  highlights and visual-line/word navigation are not established. Physical
  source lines are not measured visual lines; AccessKit's paragraph logic
  principally recognizes LF.
- Only committed text is exposed. The pending-composition guard preserves
  existing native intent, but real CJK preedit/candidate presentation and
  accessibility interactions with actual composition remain unverified.
- The window-runtime config shares one semantic tree for the application's
  single document window. It does not provide independent multiwindow state.
- Publications build complete trees; revisions also rebuild source and line
  metadata. Large documents add allocation and CPU work. Incremental updates
  and activation-gated work need measurement before performance claims.
- Source revisions replace native node IDs. Extended VoiceOver editing and
  cursor continuity after repeated changes still need testing.

The [dated native evidence](validation/accessibility.md) records the initial
prototype's API and user-confirmed spoken results, its exact implementation,
and the limits of that observation. It predates subsequent review fixes and
ownership refactoring; it is not validation of every later revision.

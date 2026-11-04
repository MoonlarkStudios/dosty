# Native Source accessibility observations — 2026-11-04

Historical macOS observations of the initial opt-in accessibility prototype.
See [current Source accessibility](../accessibility.md) for the supported
contract, run recipe and limits.

## Recorded implementation and method

The sessions ran on macOS 26.7.1 (25G241), Apple silicon, mise Rust 1.95.0 and
the pinned Iced/Winit stack. The prototype was an uncommitted change based on
`5f8d163c34cba0596647037b68c635f9fe60dea6`.
[Source and executable digests](accessibility/environment.json) identify the
last implementation tested in these sessions. A separate ad-hoc-signed
**Dosty Accessibility Prototype** app used its own configuration and a
[124-byte synthetic Unicode fixture](accessibility/native-source.md). Existing
Dosty windows and unsaved documents were preserved.

The `native-final.json` and `environment.json` filenames refer to the last
pass in this initial prototype session. These observations preceded the later
same-value SetValue and pending-composition review fixes, and the subsequent
application/document ownership refactor. They do not establish spoken behavior
of the current reviewed implementation. The raw captures and hashes retain
their original contents for that reason.

## Preliminary native API pass

The [preliminary record](accessibility/native-preliminary.json) describes the
build before the grapheme-boundary correction. The app exposed and focused a
native **Markdown source** text area. A native value action replaced source
with the fixture; an AX range selected `日本語`, typing replaced that range,
and Cmd+Z restored both text and selection. A native save panel wrote
byte-identical Markdown. Closing only the test window exited its process,
while the existing Dosty validation app stayed running.

This pass does not establish the later grapheme correction or spoken VoiceOver
output. Its executable digest is retained separately from the later pass.

## Last native API and VoiceOver pass

The [last native record](accessibility/native-final.json) used the
grapheme-corrected [optimized build](accessibility/release.txt) in the isolated
bundle. A native AX caret before `🧑‍💻`, followed by Right and typing `!`,
produced `🧑‍💻!.` without splitting the emoji. Cmd+Z restored the fixture;
the native save again matched all 124 bytes.

With the user's explicit approval, VoiceOver was briefly enabled from its
observed **off** state. The focused Source area received VoiceOver's interact,
read-contents and read-line commands. The user confirmed: **“Yes, it announced
or read the source.”** The automation interface could not attach to VoiceOver's
own feedback UI, so this is a user-confirmed spoken result rather than an
automatically captured speech transcript. VoiceOver was then observed **off**,
System Settings returned to its original Microphone pane, and the saved
synthetic **Native final.md** was left open for review.

The native value action briefly invalidated the automation's old element ID;
a refreshed tree confirmed the mutation had applied. The operation was not
repeated on the basis of that stale-element error.

## Native value preservation after review fixes

A [separate native API pass](accessibility/native-review.json) records the
reviewed implementation on the later ownership-refactor base, with fresh source
and executable hashes. Its isolated **Dosty Accessibility Review** bundle used a
[short Unicode fixture](accessibility/native-review-source.md). Native AX range
calls selected `unchanged` and then `🧑‍💻`; after observing each exact selection,
resending the identical source value preserved that selection. A native save
matched the fixture bytes exactly. Existing prototype and writing windows were
preserved.

This pass did not change VoiceOver settings or repeat the spoken test. The
pending-composition guard is covered by real Source widget event regressions,
including clear-before-commit; native CJK candidate behavior remains unverified.

## Reproduction and limits

Use the [isolated run recipe](../accessibility.md#run-an-isolated-prototype) with
a copy of the retained fixture. Inspect the native multiline value, select a
range through the accessibility API, edit, undo and save. Compare exact saved
bytes. Check joined-emoji movement, then perform an explicitly approved short
VoiceOver session and restore settings. Record new source/executable hashes
rather than reusing these observations for another build.

The results establish a small Source reading/editing path for the recorded
implementation. They do not establish full application screen-reader support,
spatial text accuracy, extended VoiceOver cursor continuity, real IME/CJK
candidate behavior, display transitions, other macOS versions or other
platforms. No performance improvement or native latency claim is made.

The original session also retained its [application check](accessibility/check.txt),
[window-runtime checks](accessibility/window-runtime-tests.txt),
[repository checks](accessibility/check-repo.txt) and
[dependency audit](accessibility/audit.txt). Those logs describe that historical
source; routine checks for subsequent changes belong in their PR/CI results.
The [vendor patch reversal](accessibility/vendor-provenance.json) records
checksum verification of all original imported files.

# Validation evidence

These artifacts document observations of specific revisions and environments.
They do not replace the [current release limits](../evaluation.md), CI results,
or [contribution checks](../../CONTRIBUTING.md#validation-and-performance-evidence).
Raw logs, captures, fixtures, and source/executable digests remain byte-exact.
[History references](../history.md) explain older commit IDs and normalized names.

## Native observations

- [Files, organization, and writing-only layout](file-navigation/README.md):
  isolated macOS native folder navigation, drag/drop, undo, guarded switching,
  rename/create, and footer/focus observations on 2026-11-04, with a Native quiet
  titlebar/navigation/save/preferences follow-up on 2026-11-06.
- [Clipboard, image drop, file dialogs, and input methods](native-writing.md):
  macOS sessions on 2026-10-23, including actual PNG drop and accented input;
  real CJK candidate-window behavior remains unverified.
- [Source accessibility](accessibility.md): initial macOS native text/editing
  and user-confirmed VoiceOver observations on 2026-11-04; these predate later
  prototype review fixes and ownership refactoring.
- [Spreadsheet clipboard](table-clipboard.md): macOS/Chrome Sheets and Excel Web
  observations on 2026-11-03, with exact-value artifacts and coercion limitations.
- [Brand fidelity and native presentation](dosty-branding/README.md): reference
  overlays, wordmark rendering, and isolated native presentation.

## Software previews

Ignored private tests render application/widget states without opening native
windows. Run with an isolated configuration directory; outputs and focused test
names live beside their implementations in `src/app/` and `src/editor/`.

```sh
DOSTY_CONFIG_DIR=/tmp/dosty-preview mise exec -- cargo test --locked theme_ui_previews -- --ignored --nocapture --test-threads=1
DOSTY_CONFIG_DIR=/tmp/dosty-preview mise exec -- cargo test --locked table_controls_previews -- --ignored --nocapture --test-threads=1
```

These exercise software rendering and synthetic input. They cannot establish
OS event delivery, screen-reader navigation, real IME candidate placement, or
GPU/display latency.

| Retained captures | Scope |
| --- | --- |
| [Code](code-editing) | Live/Read surfaces in three themes |
| [Footnotes](footnotes) | Live/Read and wide/narrow details dialogs |
| [Table controls](table-controls), [document tables](document-tables), [cell drafts](table-grid) | Successive table UI revisions |
| [Presentation](table-presentation), [menus](table-menus), [hover controls](table-polish) | Quiet/active geometry and contextual controls |
| [Clipboard](table-clipboard) | Range paste and narrow/wide presentation |

The captures retain the appearance of those builds, including superseded UI.
Current behavior belongs in the feature pages. Automated-only table navigation
and keyboard checks remain in [their raw records](table-arrow-navigation),
[edge-paragraph records](table-edge-paragraphs), and [keyboard records](table-keyboard);
routine test summaries are available in the logs and Git history.

[Performance reports](../performance/README.md) retain measurement methodology and
results separately from these native and visual observations.

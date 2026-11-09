# Dosty appearance

Dosty's curled-page **d** with a negative-space fountain pen and outlined DM Sans wordmark form the identity. The application extends the brand into a quiet writing environment: warm surfaces, confident ink, restrained accents, and generous space around the document. The logo has no tagline.

## Atmospheres

| Preset | Canvas and chrome | Ink and emphasis |
| --- | --- | --- |
| Paper | Warm paper, pale sage sidebar | Charcoal ink, smoked teal controls, sage and parchment mark |
| Midnight | Warm charcoal surfaces | Cream text, muted teal controls |
| Sage | Linen paper, pale botanical chrome | Dark green ink, leaf accents |

The brand palette is in [the brand kit](../branding/dosty/README.md). The decorative logo colors are separate from the stronger interactive accent: a small label or link must remain readable. Each preset has its own semantic colors in `themes/`, rather than a tint over one universal palette.

DM Sans supplies the outlined brand wordmark, shared with the Moonlark Studios family. System sans serif supplies document text, headings, and controls by default. Settings → Writing → Typeface offers Newsreader as a serif alternative. The preference applies to Write and Read modes, including table drafts, and survives appearance changes. Built-in themes use 17-point text, a 1.65 line height, a 610-point writing measure, and restrained heading sizes. Monospace supplies source, code, and paths. The writing surface has no persistent logo or signature; the brand masters remain in the brand kit.

## Roles and state

`ResolvedTheme` owns the semantic tokens and Iced palette. Presets and previews resolve at startup; custom themes resolve when loaded. Views consume the stored result.

- `canvas`, `surface`, and `elevated` distinguish the document, navigation, and dialogs.
- `text`, `muted`, and `faint` establish hierarchy while retaining readable small labels.
- `accent` identifies actions; `accent_soft` identifies the current selection. `selection` is reserved for selected document text.
- `brand_ink`, `brand_accent`, and `brand_paper` color the mark body, upper curl, and page underside. All are optional in custom themes; they fall back to `text`, `accent`, and `muted` respectively. The pen remains transparent negative space and the wordmark uses `text`.
- Primary and selected foregrounds, hover surfaces, control borders, disabled colors, scrims, and shadows derive once during resolution. Primary foregrounds accommodate both dark and pale custom accents.
- Hover and pressed states provide feedback. Disabled undo/redo controls are visibly unavailable; the current file retains its selected treatment even without an action.
- Inputs use a visible boundary and a stronger focus outline. Scrollbars, notifications, code surfaces, and dialogs use the same palette as the document.

Built-in theme tests check normal text and interactive accent contrast against their intended surfaces at **4.5:1**, selection readability, and input boundaries at **3:1**. These thresholds follow [WCAG contrast guidance](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum); they are color checks, not an accessibility certification. Custom files retain their existing colors and validation behavior; arbitrary custom palettes are not automatically contrast-corrected.

## Layout and behavior

The document is the central surface, with no ribbon, permanent formatting toolbar, or footer command bar. A quiet titlebar holds Files, Format, and Outline controls on the same background as the sidebars. On macOS it uses a compact row aligned with the native window buttons; the system buttons remain native. File, Edit, Format, View, Window, and Help live in the macOS menu bar. Windows/Linux keep system decorations and an in-window File command menu. F10 opens the keyboard-operated command menu on every platform.

The filename opens document actions and the Write, Source, and Read modes. Normal writing needs no mode label; Source and Read display a small label beside the filename. Titlebar controls and the save indicator remain visible while typing: a check means saved, a filled dot means modified, an empty circle means never saved, a save icon means saving, and a warning means saving failed. Tooltips explain these shapes; clicking can save. A save error stays represented after its notification is dismissed until a successful save or a different document is installed.

Navigation and formatting controls stay available without pointer movement. Word count and approximate reading time (200 words per minute) stay at the bottom center of the writing column. Their small reading gutter keeps the same height when either statistic is disabled, preserving the writing viewport. Each statistic is independently configurable. Preferences persist without document or workspace history.

Workspace/Files and its titlebar toggle are available only while a folder workspace is open. The folder name identifies the workspace and opens its actions; the root is not repeated in the tree. The header’s search icon opens Quick Open (Cmd/Ctrl+P). Workspace actions include switching folders, creation, refresh, location/Finder, and closing. Outline opens on demand on the right. Its header uses search and fold icons, shown only when headings exist. Heading search filters the tree inline, with parent context and hover-only drag grips. Each titlebar toggle keeps its sidebar open; there are no separate pin controls. Open panels occupy space beside the document when at least 480 logical pixels remain for the writing column. If both cannot fit, Files retains its dock and Outline overlays the writing area. Editing, heading jumps, document opens, and Escape preserve sidebar visibility. Files and Outline share 32-point rows, 13-point labels, 14-point hierarchy steps, compact header controls, and the same hover/selection surfaces. Panel visibility is session-local.

Files retains collapsible folders, hover/keyboard row actions, context menus, restrained drag targets, delayed expansion, and destination feedback. Undo move appears in a contextual notice and document actions, separate from text undo. Formatting and insertion commands appear only when Format is opened. Native Edit-menu commands enter the same focused-widget bindings as keyboard actions so tables and text fields retain their selection and clipboard behavior.

Hide Sidebars (Cmd/Ctrl+Shift+F) hides/restores the existing panel arrangement. It preserves the editing mode and leaves navigation controls visible. Escape dismisses the current context; it does not restore deliberately hidden sidebars. Files (Cmd/Ctrl+Shift+B) and Outline (Cmd/Ctrl+Alt+O) remain independently accessible. Presentation changes preserve source selection and viewport anchoring through local reflow.

Settings keeps its header/footer available while its body scrolls. Preset previews identify the current selection. Help contains samples, the guide, and recovery; Settings contains appearance and writing preferences. These actions remain in File's fallback command groups on Windows/Linux.

HTML exports use Paper's resolved palette for a consistent, portable reading page. Raw HTML escaping, content security policy, embedded assets, and print behavior remain intact. Exports add no logo or promotional footer to the writer's document.

The [visual validation guide](validation/README.md#software-previews) describes
software previews. [Brand validation evidence](validation/dosty-branding/README.md)
records reference fidelity and native presentation for identified builds.

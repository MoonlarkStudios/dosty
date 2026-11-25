# Dosty appearance

Dosty's curled-page **d** with a negative-space fountain pen and outlined DM Sans wordmark form the identity. The application extends the brand into a quiet writing environment: warm surfaces, confident ink, restrained accents, and generous space around the document. The logo has no tagline.

## Atmospheres

| Preset | Canvas and chrome | Ink and emphasis |
| --- | --- | --- |
| Paper | Warm paper, pale sage sidebar | Charcoal ink, smoked teal controls, sage and parchment mark |
| Midnight | Warm charcoal surfaces | Cream text, muted teal controls |
| Sage | Linen paper, pale botanical chrome | Dark green ink, leaf accents |
| Ink | White page, cool gray chrome | Dark ink, deep blue controls |

The brand palette is in [the brand kit](../branding/dosty/README.md). The decorative logo colors are separate from the stronger interactive accent: a small label or link must remain readable. Each preset has its own semantic colors in `themes/`, rather than a tint over one universal palette.

DM Sans supplies the outlined Dosty wordmark. Source Sans 3 supplies application controls and defaults to the document typeface. Settings → Writing offers a live sample, Newsreader, and System Sans. The document typeface applies to Write and Read and to table drafts. Built-in themes use 17-point text, a 1.65 line height, and a 610-point writing measure. The Writing tab also offers two fixed measures (520 or 720 points), compact or relaxed line spacing, and text size. These preferences carry across presets and custom themes. Source and code use monospace.

## Roles and state

Write shows editable Markdown markers in muted ink. The heading marker stays
centered in its row as the title is entered, along with any visible emphasis or
code delimiters in the heading.

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

Welcome has its own layout and appears when there is no document to resume. The Dosty signature identifies the screen. Documents offers New Document, Open Markdown File, and Quick Open. A returning writer can resume the document from the primary action. Projects shows recent folders with their paths alongside New Project and Open Project. The two sections stack at compact widths. Filename, editing mode, and statistics belong to the document workspace.

The document is the central surface, with no ribbon, permanent formatting toolbar, or footer command bar. A quiet titlebar holds Files, Quick Open, Format, and Outline controls on the same background as the sidebars. On macOS it uses a compact row aligned with the native window buttons; the system buttons remain native. File, Edit, Format, View, Window, and Help live in the macOS menu bar. Windows/Linux keep system decorations and an in-window File command menu. F10 opens the keyboard-operated command menu on every platform.

The left side groups Files, Welcome, Back, Forward, and Quick Open. The filename and save state sit in the center. The filename opens file actions. The right side holds the named mode, Format, Outline, and Settings. Navigation hints use platform keyboard conventions. The save indicator combines an icon and short state label for drafts, unsaved changes, saving, and save failure. Saved uses a check. Click to save or retry. A failure stays visible after dismissing its notification until a successful save or a different document is installed.

Navigation and formatting controls stay available without pointer movement. Word count and approximate reading time (200 words per minute) stay at the bottom center of the writing column. Their small reading gutter keeps the same height when either statistic is disabled, preserving the writing viewport. Each statistic is independently configurable. The word count follows the active text selection. The gutter control opens a compact, keyboard-accessible popover anchored above it for document totals.

The writing column stays centered within the available pane, with side margins at narrow widths. Write places its scrollbar at the pane's right edge, beside Outline when it is docked. The thumb appears when the document extends below the viewport. Drag the thumb or scroll from anywhere across the writing pane, including its margins.

Files offers Open Project before a project is active. The project name opens its actions, including the project picker and location. Unsaved documents carry the accent modified mark at the right edge of their row. A collapsed folder holding one uses the same mark in muted ink. Drafts sit above the tree, aligned to the rows below. Quick Open (Cmd/Ctrl+P) lists unsaved and recent documents, folders, and projects with aligned titles and location labels. Its footer shows keyboard actions.

Outline opens on demand on the right. Its heading search filters the tree inline with parent context. The panel also offers folding and hover drag grips. Files and Outline share 32-point rows, 13-point labels, 14-point hierarchy steps, and matching hover and selection surfaces. Both docks leave at least 480 logical pixels for the writing area. If both cannot fit, the most recently opened panel docks. The other returns when selected or when the window widens. Projects remember panel visibility, and Hide Sidebars toggles it for the session.

Images are listed under their full file name, which is what distinguishes them; in Quick Open they also carry the image mark. Their preview is a card with the name, location, the image scaled down to fit, and its two actions. Files rows are names only: a chevron marks a folder, documents have no glyph and no extension, and both start on the same line so the hierarchy reads from indentation alone. Files supports modifier-click selection and group context menus. While dragging, the preview shows the number of items and the destination. Folder highlights, delayed expansion, and a fixed Project root target make drops explicit. Undo move appears in a contextual notice and document actions, separate from text undo. Formatting and insertion commands appear only when Format is opened. Native Edit-menu commands enter the same focused-widget bindings as keyboard actions so tables and text fields retain their selection and clipboard behavior.

Hide Sidebars (Cmd/Ctrl+Shift+F) hides/restores the existing panel arrangement. It preserves the editing mode and leaves navigation controls visible. Escape dismisses the current context; it does not restore deliberately hidden sidebars. Files (Cmd/Ctrl+Shift+B) and Outline (Cmd/Ctrl+Alt+O) remain independently accessible. Presentation changes preserve source selection and viewport anchoring through local reflow.

Settings has Writing, Appearance, and Shortcuts tabs. The header, tabs, and footer stay visible while the body scrolls. Preset previews show the relationship between the page and navigation colors and identify the selected theme. Custom themes load from UTF-8 TOML files up to 64 KiB. Each file follows the schema shown in [Paper](../themes/paper.toml). Help contains samples, the guide, and recovery. These actions remain in File's fallback command groups on Windows/Linux.

HTML exports use Paper's resolved palette for a consistent, portable reading page. Raw HTML escaping, content security policy, embedded assets, and print behavior remain intact. Exports add no logo or promotional footer to the writer's document.

The [visual validation guide](validation/README.md#software-previews) describes
software previews. [Brand validation evidence](validation/dosty-branding/README.md)
records reference fidelity and native presentation for identified builds.

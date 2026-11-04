# Dosty appearance

Dosty's curled-page **d** with a negative-space fountain pen and outlined DM Sans wordmark form the identity. The application extends the brand into a quiet writing environment: warm surfaces, confident ink, restrained accents, and generous space around the document. The logo has no tagline.

## Atmospheres

| Preset | Canvas and chrome | Ink and emphasis |
| --- | --- | --- |
| Paper | Warm paper, pale sage sidebar | Charcoal ink, smoked teal controls, sage and parchment mark |
| Midnight | Warm charcoal surfaces | Cream text, muted teal controls |
| Sage | Linen paper, pale botanical chrome | Dark green ink, leaf accents |

The brand palette is in [the brand kit](../branding/dosty/README.md). The decorative logo colors are separate from the stronger interactive accent: a small label or link must remain readable. Each preset has its own semantic colors in `themes/`, rather than a tint over one universal palette.

DM Sans supplies the outlined brand wordmark, shared with the Moonlark Studios family. Newsreader supplies document text and editorial headings. System sans serif supplies controls. Monospace supplies source, code, and paths. The sidebar signature uses the actual outlined wordmark, never a font approximation. Its Iced paths are generated from the SVG masters by the brand exporter; no SVG parsing, image decoding, or font lookup happens to render the signature.

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

The sidebar keeps the signature, Files/Outline navigation, and document actions together. The document remains the central surface. Formatting uses a consistent vector icon set with tooltips. Focus mode hides the sidebar and formatting toolbar while retaining the document title, exit control, and writing status.

The status bar reduces secondary detail in narrow windows. Dialogs remain within the viewport; appearance settings keep their header and footer available while their contents scroll. Preset previews display each palette and identify the current selection. Text-size controls use the existing session zoom behavior and keyboard shortcuts.

HTML exports use Paper's resolved palette for a consistent, portable reading page. Raw HTML escaping, content security policy, embedded assets, and print behavior remain intact. Exports add no logo or promotional footer to the writer's document.

The [visual validation guide](validation/README.md#software-previews) describes
software previews. [Brand validation evidence](validation/dosty-branding/README.md)
records reference fidelity and native presentation for identified builds.

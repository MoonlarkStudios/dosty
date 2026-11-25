# Dosty

[![Check](https://github.com/MoonlarkStudios/dosty/actions/workflows/ci.yml/badge.svg)](https://github.com/MoonlarkStudios/dosty/actions/workflows/ci.yml)

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="branding/dosty/logos/pen-page-reverse.svg">
  <source media="(prefers-color-scheme: light)" srcset="branding/dosty/logos/pen-page.svg">
  <img src="branding/dosty/logos/pen-page.svg" alt="Dosty inkwell logo" width="96" height="96">
</picture>

A quiet, native Markdown editor built with Rust and Iced. Write with headings,
emphasis, strikethrough, editable tables, images, task checkboxes, footnotes, and
themes, or switch to the Markdown source at any time.

Start a document from Welcome, open a standalone Markdown file, or open a project. A
project is an ordinary folder with optional `.dosty` metadata. Files shows its
nested folders on the left, and Outline opens on the right. **Projects…** switches
between remembered folders. Dosty resumes the last project with its document,
caret, expanded folders, and sidebars. Quick Open (Cmd/Ctrl+P) finds drafts,
recent documents, and files across projects. Find in Folder (Cmd/Ctrl+Alt+F)
searches Markdown text in the active project.

The titlebar keeps navigation, editing mode, and save state visible and opens
formatting on demand. Writing uses sans-serif text by default, with serif
available in Settings. macOS uses native application menus. Configurable word
count and reading time sit below the writing area. Preview project images and
reference them from where they already live. Unsaved documents stay open in the
background when you switch, and Back and Forward retrace your steps.

See the [writing guide](assets/guide.md) for everyday editing and
[table editing](docs/table-editing.md) for cell selection, drafts, and spreadsheet transfers.

Dosty is in early development. See the [current limitations](docs/evaluation.md).
An opt-in [macOS Source accessibility prototype](docs/accessibility.md) is available
for native testing; application-wide screen-reader support remains incomplete.
Previously named Koan and Daytrip; [existing settings and drafts remain compatible](docs/identity-migration.md).

Native DMG, Windows setup EXE, DEB and RPM packaging is available through the
[draft release workflow](docs/releases.md). Release publication remains manual;
macOS builds are ad-hoc signed and Windows builds are unsigned.

## Run

Install [mise](https://mise.jdx.dev/getting-started.html) and the
[native build tools](CONTRIBUTING.md#development-environment) for your platform, then:

```sh
git clone https://github.com/MoonlarkStudios/dosty.git
cd dosty
mise trust
mise install
mise run dev
```

## Contribute

Read [CONTRIBUTING.md](CONTRIBUTING.md) for setup, checks, and pull requests.
The [documentation](docs/README.md) covers editing, architecture, and design.
Report bugs and suggest features through [GitHub issues](https://github.com/MoonlarkStudios/dosty/issues).
Report vulnerabilities [privately](SECURITY.md).

Copyright © 2026 Dosty contributors. Licensed under [GPLv3 or later](LICENSE).
Document fonts use the SIL Open Font License, with notices for
[Newsreader](assets/fonts/OFL.txt) and [Source Sans 3](assets/fonts/source-sans-3/OFL.txt). The
[DM Sans brand source](branding/dosty/sources/DMSans-OFL.txt) has its own OFL notice.

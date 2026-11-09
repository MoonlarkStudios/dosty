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

Start with a blank document, open one file, or open a folder as a workspace.
Within a workspace, Files live in the left sidebar; an optional Outline lives on the right.
The folder-name menu manages the workspace. Quick Open (Cmd/Ctrl+P) finds files
by name or relative path, including files in collapsed folders.
An always-visible titlebar offers formatting on demand and a document menu for editing
modes. Writing uses sans-serif text by default, with serif available in Settings. macOS uses native application menus. Configurable word count and reading
time sit below the writing area. Dosty keeps no recent-file history or saved
folder sessions.

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
Document fonts use the [SIL Open Font License](assets/fonts/OFL.txt). The
[DM Sans brand source](branding/dosty/sources/DMSans-OFL.txt) has its own OFL notice.

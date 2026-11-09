# Documentation

Start with the [README](../README.md) to build and run Dosty, the
[bundled guide](../assets/guide.md) for writing, and
[CONTRIBUTING](../CONTRIBUTING.md) for development practices.

## Current behavior and design

| Document | Reader need |
| --- | --- |
| [Architecture](architecture.md) | Ownership, data flow, module boundaries, and editing invariants |
| [Writing workflows](writing-workflows.md) | Files/folders, workspace identity, Quick Open, organization and dragging, native menus, independent sidebars, formatting, links, images, saving, recovery, and export |
| [Collaboration](collaboration.md) | Source coordinates, transactions, offline replicas, proposals, history, and persistence |
| [Outline](outline.md) | Heading navigation, local collapse state, and section moves |
| [Code editing](code-editing.md) | Code entry, copying, rendering, and highlighting limits |
| [Task lists](task-lists.md) | Checkbox and keyboard behavior |
| [Table editing](table-editing.md) | Cell selection, drafts, clipboard, structural commands, and history |
| [Strikethrough](strikethrough.md) | Formatting and source disclosure |
| [Footnotes](footnotes.md) | Syntax, note details, numbering, and navigation |
| [Appearance](design-system.md) | Themes, semantic colors, typography, and layout |
| [macOS Source accessibility](accessibility.md) | Opt-in native text, selection, actions, and prototype limits |
| [Performance](performance.md) | Current scaling limits and profiling methodology |
| [Release readiness](evaluation.md) | Unsupported behavior and platform validation limits |
| [Identity and compatibility](identity-migration.md) | Configuration paths, legacy aliases, and retained file formats |
| [Dependency health](dependency-health.md) | Audit policy and vendored patch maintenance |
| [Releases](releases.md) | Versioning, packaging, draft verification, and publication |
| [Brand kit](../branding/dosty/README.md) | Masters, palette, licenses, and reproducible asset export |

## Evidence

[Performance measurements](performance/README.md) and
[native/visual validation](validation/README.md) retain reproducible evidence for
specific revisions and environments. They are separate from current behavior
documentation. Review summaries and routine passing checks belong in PRs and CI.

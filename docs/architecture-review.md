# Architecture review — 2026-11-04

Reviewed `main` at `5f8d163` in the separate
`codex/rust-architecture-review` worktree. This is a source architecture and
convention review, not a native interaction or performance report. The
[architecture map](architecture.md) describes the resulting current boundaries.

## Decisions and changes

| Area | Finding and disposition |
| --- | --- |
| Cargo organization | Retain one package, a library, and a thin executable. The sole public library export is `run()`. There is no demonstrated consumer or build measurement that warrants another application crate. |
| Authoritative state | `Document` exposed its mutable CRDT and derived block/statistic/revision fields to callers. Make those fields private and provide shared references, slices, and copied counts. Keep all source changes and history application in the existing transaction/cache path. |
| Background IO | `ImageStore` constructed `app::Message`. Return an IO completion value and map it in `App`, retaining the existing task scheduling, bounds, cache, and decoding behavior. |
| Workflow visibility | Narrow workflow modules to their parent and keep recovery private. Continue re-exporting deliberate interaction messages through `app`. |
| Failure contracts | Replace the production table lookup `unwrap` with an `expect` explaining the borrowed-table invariant. External IO failures continue to return errors. |
| Dependencies | Keep the dependency graph and vendored patches unchanged. No new trait, crate, or generic callback is needed for these changes. |

Updating existing callers and their private tests is mechanical. No module files
move, and no source editing, mapping, layout, history, or file format changes are
intended. Rust privacy enforces the new document boundary; existing regression
tests exercise its behavior.

## Rust guidance

The [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html)
distinguishes application code from reusable libraries and recommends direct
application APIs without premature generalization. Its small-crate and flat-file
rules are Chromium-specific. Dosty's responsibility-based modules follow the
documented [project interpretation](../CONTRIBUTING.md#rust-conventions).

This review prioritizes restricted mutation, explicit ownership, borrowed data,
named result fields, and meaningful panic context. Shared immutable snapshots
retain their documented `Arc` ownership. Stable rustfmt and Clippy remain the
automated baseline; application unsafe code remains forbidden.

## Remaining considerations

- `document/selection.rs` adapts Iced cursors and native content;
  `document/code_edit.rs` accepts Iced edit actions; `document/source.rs` owns the
  lazy Iced Read cache. A reusable document crate would require moving these
  adapters and preserving cache invalidation and unchanged-block reuse. The
  current application has no such consumer.
- Editor widgets consume application interaction messages. A reusable editor
  surface would need its own event contract. Introducing generic message plumbing
  solely to remove that dependency would add complexity without a present need.
- `application.rs`, `rich_editor.rs`, `rich_layout.rs`, and `projection.rs`
  contain several responsibilities. Workflow children already limit some of that
  growth. Future work should extract a coherent responsibility in a mechanical
  commit before changing behavior, retaining private state and existing tests.
- Projection fields remain crate-internal data consumed together by layout and
  editing. Their current callers read the snapshots and use projection methods
  to update them. Consider a borrowed view if a future consumer needs a stronger
  independent boundary; do not expose these types as a public library API.

These are constraints on future extraction, not evidence that splitting Cargo
packages today would improve the application.

## Validation

Validation used the mise-pinned Rust 1.95.0 toolchain on macOS/Apple silicon.

| Command | Result |
| --- | --- |
| `mise run check` | Passed formatting, Clippy with warnings denied, and 355 tests; 29 tests remained explicitly ignored. |
| `mise run check-repo` | Passed offline documentation links, unused dependency checks, workflow syntax, release metadata, and 22 policy tests. |
| `mise run release` | Built the optimized application successfully. |

The unchanged private document differential tests still compare character counts
with a fresh parse; the application benchmark checks the accessible word count
and block metadata without adding a test-only character-count accessor.

No dependency or performance change is claimed, so this review does not replace
the existing audit or benchmark evidence. Linux, Windows, native IME, and desktop
interaction testing were not repeated for these visibility and result-routing
changes. No native windows or unsaved documents were used for validation.

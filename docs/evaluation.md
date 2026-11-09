# Current limitations and release readiness

Dosty is an experimental Markdown editor. Automated model, widget, and software
rendering checks cover many editing contracts; they do not establish native
accessibility or complete desktop compatibility.

## Remaining release work

| Area | Current limits |
| --- | --- |
| Accessibility | An opt-in [macOS Source prototype](accessibility.md) exposes native text and selection. Live/Read semantics, application controls, spatial text geometry, and broader VoiceOver workflows remain incomplete; complete screen-reader support is a release blocker. |
| Input and platforms | Real CJK candidate windows, bidirectional editing, alternate keyboard layouts, dragging/autoscroll, and Windows/Linux native workflows need broader validation. |
| Performance | Global Markdown dependencies, uncertain parse boundaries, contiguous buffers, suffix rebasing, outline construction, and very long paragraphs/table rows retain document-size costs. Native input/GPU presentation latency is not established by software profiles. |
| Durability | Recovery can lose edits since the last checkpoint. Undo stacks are session-local. Multiwindow session management and filesystem/power-loss behavior need broader testing. |
| Desktop integration | Native menus, document associations, trusted signing, and macOS notarization are incomplete. |
| File organization | Folder discovery and moves are bounded; there is no filesystem watcher, delete, multi-selection, recent-file history, or persisted folder session. Moves stay on one filesystem and can be blocked by references or metadata conflicts. Native dragging, hover expansion, cancellation, and filesystem failure behavior need broader platform validation. |
| Markdown and export | Mathematics, diagrams, PDF/office export, merged cells, saved table widths, image cropping/resizing, SVG, and animated-image playback are unsupported. GIFs display their first frame. |

CommonMark delimiter rules cannot represent every rich-text join. Some punctuation
joins flatten the affected inline style to preserve displayed text. Source mode
remains available for exact Markdown editing.

The optional Read renderer does not share document-wide reference-style link
definitions with Live. Its indented-code rendering can show text as prose after
an empty code surface; fenced code is the reliable rich-code path in Read and
footnote previews.

Spreadsheet transfers carry visible values. Excel Web can coerce identifiers and
dates during normal paste, while Paste Values can remove embedded tabs. There is
no verified universal lossless Excel Web route for mixed blocks. See
[clipboard behavior](table-editing.md#copy-and-paste).

## Validation scope

The mise checks and CI workflow define automated validation; see
[CONTRIBUTING](../CONTRIBUTING.md#validation-and-performance-evidence).
The [native observations](validation/README.md) record the OS, revision, workflow,
and limitations of manual checks. A result applies to that recorded environment,
not every platform or later revision.

The [performance guide](performance.md) distinguishes profiling methodology from
historical measurements. The [dependency policy](dependency-health.md) tracks
third-party maintenance and audit requirements separately from application tests.

[Release preparation](releases.md) builds draft native packages. Publication
requires inspecting the actual downloaded packages and recording platform tests.

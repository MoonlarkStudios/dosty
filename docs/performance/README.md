# Historical performance measurements

These reports preserve measurements of identified revisions, not current latency
claims. Use the [performance guide](../performance.md) for methodology. Reports
retain fixture sizes, environments, sample counts, raw logs, exclusions, and
interruptions. Compare only matched runs. [History references](../history.md)
resolve commit IDs and application names in older captures.

| Report | Recorded scope |
| --- | --- |
| [Original baseline](baseline.md) | 2026-10-22 full-document editor |
| [Viewport virtualization](virtualization.md) | Resident rich geometry and cold memory |
| [Native initialization](native-buffer.md) | Deferred native shaping |
| [Incremental parsing](incremental-parsing.md) | Certified source windows |
| [Native snapshots](native-snapshots.md) | Local native text extraction |
| [Source/projection updates](source-projection-updates.md) | Buffer and metadata reuse |
| [CRDT integration](crdt-performance.md) | Transaction, position, and retention costs |
| [UTF-16 traversal](utf16-performance.md) | Cached Yrs traversal |
| [Offline imports](offline-imports.md) | Staged admission and independent windows |
| [Writing workflows](writing-workflows.md) | Integrated editing, imports, memory, and retention |
| [Code highlighting](code-editing.md) | Long code and mixed-document interactions |
| [Outline](outline.md) | Queries, section moves, and undo |
| [Column transactions](table-column-performance.md) | Matched large-table command comparison |
| [Table interactions](table-editing.md) | Table commands, drafts, ranges, and bulk paste |
| [Dependency updates](dependencies.md) | Regression smoke measurements around dependency repairs |

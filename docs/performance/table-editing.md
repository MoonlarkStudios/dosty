# Historical table measurements

These are measurements of successive table implementations, not current latency
guarantees. Preserve the fixtures, sample counts, revisions, and exclusions
when comparing results. Raw logs and source digests remain unchanged.

## Multi-cell clipboard validation (2026-11-03)

The focused bulk-paste measurement takes five release samples each at 1,000 and
5,000 rows by three columns, growing a two-row table. Values include Unicode,
edge spaces, pipes, and Markdown punctuation. It measures the keyboard event,
App update, focus operations, and resident layout at 640×560, using the in-memory
clipboard and software text renderer. Setup, correctness checks, undo, drawing,
OS delivery, and display are excluded. Per-sample correctness and single-step
undo are checked outside timing. The raw log records min/median/max; these are
observations of the new operation, without a matched before/after comparison or
a claim about the existing 10k/50k document interaction profiles.

| Pasted rows × columns | Samples | Min / median / max (ms) |
| --- | ---: | ---: |
| 1,000 × 3 | 5 | 6.49 / 6.72 / 7.47 |
| 5,000 × 3 | 5 | 76.10 / 78.68 / 82.73 |

Reproduce with a temporary configuration directory:

```sh
mise run check
mise run check-repo
mise run release
DOSTY_CONFIG_DIR=/tmp/dosty-clipboard-validation mise exec -- cargo test --locked clipboard -- --test-threads=1
mise exec -- cargo test --locked grid_paste -- --test-threads=1
mise exec -- cargo test --locked table_history -- --test-threads=1
DOSTY_CONFIG_DIR=/tmp/dosty-clipboard-validation mise exec -- cargo test --locked table_clipboard_previews -- --ignored --nocapture --test-threads=1
DOSTY_CONFIG_DIR=/tmp/dosty-clipboard-validation mise exec -- cargo test --locked --release bulk_widget_paste_measurements -- --ignored --nocapture --test-threads=1
```

Raw evidence: [focused tests](../validation/table-clipboard/focused-tests.txt),
[checks](../validation/table-clipboard/check.txt),
[repository checks](../validation/table-clipboard/check-repo.txt),
[release build](../validation/table-clipboard/release.txt),
[preview run](../validation/table-clipboard/previews.txt),
[bulk paste measurements](../validation/table-clipboard/bulk-paste.txt), and
[environment/source digest](../validation/table-clipboard/environment.json).

The clipboard tests establish values-based text handling and editor behavior
using representative spreadsheet payloads. They do not establish native
Google Sheets or Excel interoperability on any platform. OS clipboard ownership,
native event delivery, Office HTML, formula/style transfer, accessibility, GPU
presentation, and input-to-display latency remain outside this validation.
Existing windows, unsaved documents, and the system clipboard were untouched.

## Table presentation validation (2026-11-02)

The release table-input harness uses the same 1,000/5,000-row fixtures, bundled
fonts, 30 serial samples, measured phases, and untimed restoration described in
the historical spreadsheet interaction report below. It includes pending draft
input, Tab commit, cell navigation, and a 31×3 range. The mixed-document profiles
use 10k/50k blocks (910,450 / 4,574,450 bytes), 30 samples per repeated interaction,
and the event/layout/drawing/software-raster phases described in
[the profile methodology](../performance.md#reproduce). Cold values
are separate single observations. These observations are not a matched speedup
comparison or a native input-to-display latency guarantee.

| Table interaction, including software rendering | 1,000 rows, ms p50 / p95 / max | 5,000 rows, ms p50 / p95 / max |
| --- | ---: | ---: |
| Space in pending draft | 2.866 / 3.004 / 3.018 | 2.820 / 2.941 / 2.961 |
| Text in pending draft | 2.832 / 3.120 / 3.145 | 2.799 / 2.928 / 3.010 |
| Tab commit and advance | 4.656 / 4.984 / 5.040 | 11.255 / 11.837 / 11.951 |
| Arrow navigation | 2.871 / 3.048 / 3.066 | 2.808 / 2.912 / 2.952 |
| Select 31×3 rectangle | 3.536 / 3.807 / 3.834 | 3.450 / 3.621 / 3.661 |

At 5,000 rows, pending-text median input/update/layout was 0.001 / 0.001 / 0.015 ms; Tab's median update was 8.502 ms, including its source commit. Rasterization dominated draft input. Phase medians must not be summed to reconstruct total percentiles.

| Mixed-document interaction | 10k blocks, ms p50 / p95 / max | 50k blocks, ms p50 / p95 / max |
| --- | ---: | ---: |
| Middle typing | 1.887 / 2.697 / 2.802 | 5.882 / 8.636 / 8.788 |
| Disclosure | 2.978 / 3.236 / 3.784 | 14.805 / 16.674 / 17.093 |
| Half-document selection | 1.258 / 1.366 / 1.373 | 1.626 / 1.769 / 1.894 |
| Scrolling | 1.312 / 1.559 / 1.741 | 1.317 / 1.624 / 1.744 |

Cold initialization was 39.965 / 223.554 ms (10k / 50k), separately from first layout/draw/software rendering at 28.520 / 46.122 ms. Each cold value has one sample. The 50k end-typing run retained a 127.873 ms maximum with a 6.211 ms p95. Structural scaling costs remain visible in the raw profiles.

Raw evidence: [checks](../validation/table-presentation/check.txt),
[repository checks](../validation/table-presentation/check-repo.txt),
[release build](../validation/table-presentation/release.txt),
[preview run](../validation/table-presentation/previews.txt),
[table input and selection](../validation/table-presentation/table-input.txt),
[10k profile](../validation/table-presentation/profile-10k.txt),
[50k profile](../validation/table-presentation/profile-50k.txt), and
[environment and complete source digest](../validation/table-presentation/environment.json).
Measurements use mise's Rust 1.95.0 toolchain, release mode with thin LTO and one
codegen unit, Iced 0.14, bundled Newsreader fonts, and macOS 26.7.1 arm64. CPU
model and physical memory were unavailable in this execution environment. The
working tree includes existing user changes. Benchmark/profile workloads ran
serially; other desktop activity was not controlled. Ordinary regressions contain
no timing gates.

## Spreadsheet interaction validation (2026-11-01)

The focused release profile uses 1,000/5,000-row, three-column tables with Unicode
and inline formatting. Bundled fonts are loaded before measurements. Space and
text update the native pending draft; Tab commits once and selects the next
cell. Reopening the growing first-cell draft is outside timing. Arrow navigation
and a 31×3 rectangle are measured in the middle of the table, with selection reset
outside timing. Each action has 30 serial samples, including event/application
updates, layout, drawing, and software rasterization. Native focus operations can
perform layout inside the update phase. Initial construction, restoration,
background IO, OS event delivery, and GPU/display work are excluded. This changes
the editing contract and harness from the historical document-table profile;
these runs are current observations, not a matched speedup comparison.

| Interaction, including software rendering | 1,000 rows, ms p50 / p95 / max | 5,000 rows, ms p50 / p95 / max |
| --- | ---: | ---: |
| Space in pending draft | 3.054 / 3.172 / 3.173 | 3.042 / 3.146 / 3.169 |
| Text in pending draft | 3.038 / 3.161 / 3.182 | 3.073 / 3.130 / 3.130 |
| Tab commit and advance | 4.788 / 4.948 / 5.168 | 11.216 / 11.480 / 11.741 |
| Arrow navigation | 3.244 / 3.377 / 3.390 | 3.253 / 3.368 / 3.380 |
| Select 31×3 rectangle | 4.070 / 4.148 / 4.408 | 4.047 / 4.169 / 4.399 |

At 5,000 rows, the median draft-text input/update/layout phases were
0.001 / 0.001 / 0.013 ms; rasterization dominated that sample. Tab's median update
was 8.252 ms, including the one source commit. These phase medians are descriptive
and must not be summed to reconstruct total percentiles.

The mixed-document 10k/50k-block profiles also passed with 30 samples per repeated
interaction. They retain continuous selection/dragging, disclosure, scrolling,
IME commit, and undo/redo coverage beyond tables.

| Mixed-document interaction | 10k blocks, ms p50 / p95 / max | 50k blocks, ms p50 / p95 / max |
| --- | ---: | ---: |
| Middle typing | 1.750 / 1.931 / 2.357 | 5.689 / 7.388 / 8.163 |
| Disclosure | 2.995 / 3.551 / 3.649 | 12.113 / 12.767 / 12.798 |
| Half-document selection | 1.228 / 1.308 / 1.340 | 1.605 / 1.703 / 1.720 |
| Scrolling | 1.529 / 2.005 / 2.091 | 1.503 / 1.992 / 2.127 |

Cold initialization was 38.654 / 203.496 ms (10k / 50k), separately from first
layout/draw/software rendering at 28.645 / 43.296 ms. Each cold value is one sample.
The 50k end-typing run retained a 151.082 ms maximum, with a 13.019 ms p95. These
observations retain the document's structural scaling limits and do not establish
a native frame-time guarantee.

Reproduce with mise's Rust 1.95.0 toolchain and release settings (thin LTO, one
codegen unit):

```sh
DOSTY_CONFIG_DIR=/tmp/dosty-table-preview mise exec -- cargo test --locked table_grid_previews -- --ignored --nocapture --test-threads=1
mise exec -- cargo test --locked --release performance_document_table_typing -- --ignored --nocapture --test-threads=1
DOSTY_PROFILE_BLOCKS=10000 DOSTY_PROFILE_SAMPLES=30 mise run profile
DOSTY_PROFILE_BLOCKS=50000 DOSTY_PROFILE_SAMPLES=30 mise run profile
```

Raw evidence: [checks](../validation/table-grid/check.txt),
[repository checks](../validation/table-grid/check-repo.txt),
[release build](../validation/table-grid/release.txt),
[preview run](../validation/table-grid/previews.txt),
[table input and selection profile](../validation/table-grid/table-input.txt),
[10k profile](../validation/table-grid/profile-10k.txt),
[50k profile](../validation/table-grid/profile-50k.txt), and
[environment and complete source digest](../validation/table-grid/environment.json).
The working tree includes existing user changes. macOS 26.7.1 arm64, Iced 0.14,
and the bundled Newsreader fonts were used; CPU model and physical memory were
unavailable in this execution environment. Other desktop activity was not
controlled. Initial queued release runs were stopped before measurements while
final correctness work continued; their [input](../validation/table-grid/table-input-interruption.txt)
and [profile](../validation/table-grid/profile-10k-interruption.txt) logs are retained
and excluded from reported results. Ordinary regressions contain no timing gates.

## Release measurements (2026-10-30)

Historical evidence: this section records the initial controls, before the column
transaction, keyboard feedback, and document editing changes.

The command benchmark uses 1,000 and 5,000 body rows, three columns, Unicode and
inline formatting, and 30 samples per action. Each sample starts from a fresh App;
construction and initial synchronization are outside timing. Opening the dialog,
planning and committing its transaction, projection/native synchronization, and
recovery/search bookkeeping are included. Widget focus effects are scheduled but
not executed. Fresh-parser/native snapshot oracles and Undo run outside timing.
Layout, rasterization, IO, GPU work, and native event/display delivery are excluded.

Reproduce serially with the pinned Rust 1.95.0 toolchain in release mode:

```sh
mise exec -- cargo test --locked --release performance_table_controls -- --ignored --nocapture --test-threads=1
DOSTY_PROFILE_BLOCKS=10000 DOSTY_PROFILE_SAMPLES=30 mise run profile
DOSTY_PROFILE_BLOCKS=50000 DOSTY_PROFILE_SAMPLES=30 mise run profile
```

These are local observations on macOS 26.7.1 (25G241), arm64, with Iced 0.14.
The sandbox did not expose the CPU model or memory through `sysctl`. Other desktop
activity was not controlled. No before/after speedup, native frame-time guarantee,
or timing assertion in ordinary tests is claimed.

| Command | 1,000 rows, ms p50 / p95 / max | 5,000 rows, ms p50 / p95 / max |
| --- | ---: | ---: |
| Insert row below | 1.531 / 1.872 / 1.938 | 8.067 / 8.894 / 8.894 |
| Insert column right | 36.831 / 37.406 / 37.685 | 838.486 / 844.333 / 869.331 |
| Delete column | 37.942 / 38.203 / 38.366 | 868.671 / 893.018 / 906.257 |
| Align right | 1.575 / 1.737 / 1.834 | 8.051 / 8.178 / 8.284 |

These initial column commands touched every physical row and restarted CRDT
position searches for every source edit. The approximately 0.84–0.87 second
medians at 5,000 rows describe that historical implementation. The later
[batch transaction optimization](table-column-performance.md) retains the source
edits and undo behavior while advancing one CRDT cursor through the batch.

The existing mixed-content interaction profile passed at 10k and 50k blocks with
30 samples per repeated interaction. It covers typing, selection/dragging,
disclosure, scrolling, IME, and Undo/Redo; it does not invoke the new dialog.
See the [profile methodology](../performance.md#reproduce) for its phases and limits.

| Interaction | 10k blocks, ms p50 / p95 / max | 50k blocks, ms p50 / p95 / max |
| --- | ---: | ---: |
| Middle typing | 2.533 / 3.065 / 3.251 | 6.219 / 8.045 / 10.526 |
| Disclosure | 3.521 / 3.864 / 3.967 | 16.543 / 18.013 / 18.279 |
| Half-document selection | 1.735 / 1.826 / 1.934 | 2.925 / 3.113 / 3.163 |
| Scrolling | 1.503 / 1.922 / 2.007 | 1.525 / 1.868 / 1.911 |

Cold App/source initialization was 42.08 / 216.51 ms (10k / 50k), with first
layout/draw/software rasterization at 36.42 / 46.59 ms; each is one sample,
separate from repeated interactions. End typing retained a 125.72 ms maximum at
50k, so these observations do not remove existing structural scaling limits.

Raw evidence: [checks](../validation/table-controls/check.txt),
[sandbox limitation](../validation/table-controls/check-sandboxed.txt),
[repository checks](../validation/table-controls/check-repo.txt),
[release](../validation/table-controls/release.txt),
[table benchmark](../validation/table-controls/table-benchmark.txt),
[10k profile](../validation/table-controls/profile-10k.txt),
[50k profile](../validation/table-controls/profile-50k.txt), and
[environment/source hashes](../validation/table-controls/environment.json).
Representative previews: [Paper](../validation/table-controls/paper.png),
[Paper narrow](../validation/table-controls/paper-narrow.png),
[Midnight narrow](../validation/table-controls/midnight-narrow.png), and
[Sage narrow](../validation/table-controls/sage-narrow.png).

## Space feedback validation (2026-10-30)

The same command benchmark and mixed-document profiles above ran serially with
30 samples, Rust 1.95.0, thin LTO and one codegen unit on an Apple M2 Max with
32 GB memory, macOS 26.7.1 (25G241). Other desktop activity was not controlled.
These runs check broad scaling; they do not specifically time a table-space
keypress or establish native input-to-display latency. The command benchmark
excludes layout; the document profiles include software rasterization.

| Interaction | Size | ms p50 / p95 / max |
| --- | --- | ---: |
| Insert column right | 5,000 rows | 69.442 / 71.501 / 71.878 |
| Delete column | 5,000 rows | 122.258 / 126.113 / 127.105 |
| Middle typing | 10k blocks | 2.198 / 2.381 / 2.728 |
| Middle typing | 50k blocks | 7.053 / 9.965 / 10.987 |
| Disclosure | 10k blocks | 3.447 / 4.163 / 4.403 |
| Disclosure | 50k blocks | 14.733 / 16.134 / 16.531 |
| Scrolling | 10k blocks | 1.540 / 1.824 / 1.825 |
| Scrolling | 50k blocks | 1.485 / 1.771 / 1.781 |

Cold initialization was 40.52 / 210.69 ms (10k / 50k); first layout, draw and
software rasterization took 29.34 / 44.79 ms. Each is a separate single sample.
No native windows or user documents were opened or changed.

Raw evidence: [checks](../validation/table-keyboard/check.txt),
[repository checks](../validation/table-keyboard/check-repo.txt),
[release](../validation/table-keyboard/release.txt),
[command benchmark](../validation/table-keyboard/benchmark.txt),
[10k profile](../validation/table-keyboard/profile-10k.txt),
[50k profile](../validation/table-keyboard/profile-50k.txt), and
[environment/source hash](../validation/table-keyboard/environment.txt).

## Document table interaction validation (2026-10-30)

The release runs below used Rust 1.95.0, Iced 0.14, thin LTO, one codegen unit,
macOS 26.7.1 arm64, and 30 serial samples per action. The sandbox did not expose CPU
model or physical memory. Other desktop activity was uncontrolled. Mixed-document
tail variation is retained; these are observations, not a speedup claim or native
latency guarantee. Ordinary tests contain no machine-dependent timing assertions.

| Interaction | Size | ms p50 / p95 / max |
| --- | --- | ---: |
| Space, real widget and software rendering | 1,000 rows | 4.373 / 4.693 / 5.251 |
| Space, real widget and software rendering | 5,000 rows | 10.887 / 11.256 / 11.406 |
| Text, real widget and software rendering | 5,000 rows | 10.302 / 10.725 / 10.893 |
| Tab, real widget and software rendering | 5,000 rows | 2.492 / 2.674 / 2.716 |
| Insert column right, no rendering | 5,000 rows | 71.177 / 77.820 / 80.477 |
| Delete column, no rendering | 5,000 rows | 123.425 / 125.684 / 130.040 |
| Middle typing, mixed document | 10k blocks | 2.750 / 6.565 / 7.176 |
| Middle typing, mixed document | 50k blocks | 7.545 / 9.568 / 9.930 |
| Disclosure, mixed document | 10k blocks | 3.477 / 3.998 / 4.092 |
| Disclosure, mixed document | 50k blocks | 14.813 / 16.022 / 16.042 |

The new table input profile warms the real widget at 720×800, then types Space
and `x`, Tabs to the next cell, and restores the first cell's caret outside timing.
It repeats this growing value 30 times in 1,000/5,000-row, three-column tables with
Unicode and inline formatting. Input, update, layout, drawing, and software raster
costs are recorded separately. The column profile uses independent documents and
excludes setup, validation, undo, and rendering, as described in the historical
report. All runs exclude OS event delivery and GPU/display presentation.

```sh
mise exec -- cargo test --locked --release performance_table_controls -- --ignored --nocapture --test-threads=1
mise exec -- cargo test --locked --release performance_document_table_typing -- --ignored --nocapture --test-threads=1
DOSTY_PROFILE_BLOCKS=10000 DOSTY_PROFILE_SAMPLES=30 mise run profile
DOSTY_PROFILE_BLOCKS=50000 DOSTY_PROFILE_SAMPLES=30 mise run profile
```

[Raw checks](../validation/document-tables/check.txt),
[repository checks](../validation/document-tables/check-repo.txt),
[release build](../validation/document-tables/release.txt),
[column benchmark](../validation/document-tables/column-benchmark.txt),
[table input profile](../validation/document-tables/table-typing.txt),
[10k profile](../validation/document-tables/profile-10k.txt),
[50k profile](../validation/document-tables/profile-50k.txt), and
[environment/source digest](../validation/document-tables/environment.txt) retain
reproduction evidence. An initial resource-reporting wrapper failed on the
sandbox's `kern.clockrate` access after its benchmark passed; the
[interruption log](../validation/document-tables/column-wrapper-interruption.txt)
is retained, and reported numbers use the successful unwrapped rerun.

Representative previews:
[Paper at minimum size](../validation/document-tables/paper-document-table-narrow.png),
[Midnight Live](../validation/document-tables/midnight-document-table.png), and
[Sage Read](../validation/document-tables/sage-document-table-read.png).

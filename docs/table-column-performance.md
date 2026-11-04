# Large-table column transaction performance

On 2026-10-30, column insertion and deletion were changed to reuse one advancing
Yrs cursor for a validated batch. Table controls still produce the same granular
source edits. Surrounding Markdown and untouched cells retain their exact bytes
and CRDT identities; the command remains one human transaction and one undo step.
See [table behavior](table-editing.md) and [transaction ownership](collaboration.md#transactions-and-ownership).

## Cause and change

The previous mutation helper independently located every insertion/deletion in
reverse source order. Column commands edit every physical row, so these searches
repeatedly traversed the growing CRDT. At 5,000 body rows, the CRDT component
accounted for roughly 98% of the measured command time.

For multiple edits, the application now converts the original UTF-8 offsets to
UTF-16 retain/delete/insert deltas and calls Yrs 0.28's existing `Text::apply_delta`.
The delta retains refer to unchanged spans between original edit ranges. One
cursor advances across those spans without replacing or reserializing them.
Admission staging and the active document use the same helper. Single edits
retain the direct positional path; validation, resource limits, origin tracking,
observer-driven cache updates, and history metadata remain active. No dependency
or vendored code changed.

## Matched command benchmark

Measurements used Apple M2 Max, 32 GB memory, macOS 26.7.1 (25G241), arm64,
mise Rust 1.95.0, Iced 0.14, release thin LTO and one codegen unit. Before and
after use the same three-column fixture, Unicode/inline formatting, selected
first body cell, Live mode, and **30 samples per command** at each size.
The baseline executable was retained before changing the mutation helper.
New correctness tests do not change the timed benchmark body.

Each sample constructs and synchronizes a fresh App outside timing. Timing covers
dialog opening, command planning, CRDT admission/commit, source/projection and
Iced buffer synchronization, and search/recovery bookkeeping. Focus effects are
scheduled but not executed. Fresh parser/native snapshot comparisons and Undo
run outside timing. Rendering, OS input delivery, file IO, and GPU/display
presentation are excluded. CRDT/source phase medians are reported independently;
they must not be added to estimate total percentiles.

| Body rows | Command | Before ms p50 / p95 / max | After ms p50 / p95 / max | Median speedup |
| ---: | --- | ---: | ---: | ---: |
| 1,000 | Insert column right | 36.052 / 42.981 / 59.348 | 5.302 / 5.683 / 5.771 | 6.8× |
| 1,000 | Delete column | 36.109 / 37.598 / 38.929 | 5.082 / 5.412 / 5.450 | 7.1× |
| 5,000 | Insert column right | 807.501 / 825.754 / 834.473 | 68.951 / 71.353 / 71.390 | 11.7× |
| 5,000 | Delete column | 837.186 / 850.063 / 850.653 | 122.359 / 129.283 / 138.623 | 6.8× |

At 5,000 rows, CRDT medians fell **790.816 → 52.505 ms** for insertion and
**824.180 → 109.931 ms** for deletion. Source/cache medians stayed approximately
8.15/8.19 ms and 5.57/5.60 ms respectively. Row insertion and alignment stayed near
7.7–7.8 ms in both runs; no improvement is claimed for their single-edit path.

Column commands still touch every row. CRDT string splitting/copying, admission,
source/cache work and projection rebuilding remain costs. These measurements do
not establish linear scaling, a frame-time guarantee, or fast Undo on every
document. Other desktop activity was not controlled, and 30 samples give limited
tail estimates. Native platform and GPU latency were not measured.

## Large-document regression profile

The unchanged mixed-content profile passed serially at 10k and 50k blocks before
and after, with 30 samples per repeated interaction. It includes widget input,
App updates, layout, draw submission and software rasterization; it excludes
the dialog and native presentation. See [methodology](performance.md#reproduce).

| 50k interaction | Before ms p50 / p95 / max | After ms p50 / p95 / max |
| --- | ---: | ---: |
| Middle typing | 5.812 / 7.316 / 9.014 | 5.757 / 6.964 / 9.022 |
| Disclosure | 15.041 / 16.125 / 16.672 | 15.081 / 20.306 / 29.464 |
| Scrolling | 1.465 / 1.796 / 1.859 | 1.451 / 1.781 / 1.808 |

These medians stayed similar; the higher disclosure tail is retained rather than
excluded. The small uncontrolled sample is not evidence of improved general
editor latency. End typing still reached 113.132 ms after the change at 50k.
Cold App/source initialization was 47.74 → 41.64 ms at 10k and 203.51 → 200.23 ms
at 50k, each one sample. Separate first-layout/draw/raster samples were
37.28 → 28.94 ms and 42.18 → 43.57 ms respectively. No cold-load speedup is claimed.

## Correctness and reproduction

The new differential regression compares the batch cursor with the previous
positional implementation on fragmented Unicode, RTL, combining characters,
joined emoji, CRLF, adjacent replacements, insertions and deletions. It checks
exact observer deltas, materialized text, the UTF-16 index, and sticky positions
with both affinities. Large column regressions cross the staged-admission threshold
in Live and Source, preserve reverse selections and untouched cell identities,
compare fresh source/display mappings, restore source/selection in one Undo, and
retain a concurrent edit after undo/redo. Existing rich-widget, IME, viewport,
selection, replace-all, origin-history, and replica tests also pass.

Final validation passed **237 ordinary tests** (22 optional tests ignored), strict
Clippy, formatting, repository checks, the release build, the 30-sample table
benchmark, and both large-document profiles. Existing native windows and unsaved
documents were preserved; no native UI session was opened for this change.

```sh
mise run check
mise run check-repo
mise run release
mise exec -- cargo test --locked --release performance_table_controls -- --ignored --nocapture --test-threads=1
DOSTY_PROFILE_BLOCKS=10000 DOSTY_PROFILE_SAMPLES=30 mise run profile
DOSTY_PROFILE_BLOCKS=50000 DOSTY_PROFILE_SAMPLES=30 mise run profile
```

The benchmark defaults to 30 samples. `DOSTY_TABLE_SAMPLES=1` is an optional
diagnostic probe, not the comparison above. The existing HTTP image regression
requires localhost binding permission when running the full suite in a sandbox.

Raw evidence: [before benchmark](validation/table-columns/before.txt),
[after benchmark](validation/table-columns/after.txt),
[initial one-sample phase probe](validation/table-columns/phase-probe.txt),
[before 10k](validation/table-columns/before-10k.txt),
[before 50k](validation/table-columns/before-50k.txt),
[after 10k](validation/table-columns/after-10k.txt),
[after 50k](validation/table-columns/after-50k.txt),
[checks](validation/table-columns/check.txt),
[repository checks](validation/table-columns/check-repo.txt),
[release](validation/table-columns/release.txt), and
[environment/source and binary hashes](validation/table-columns/environment.json).

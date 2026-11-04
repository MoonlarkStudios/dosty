# Performance

The editor uses conservative local source/projection updates, tracked native
snapshots, and viewport-limited rich shaping. See [architecture](architecture.md#parsing-and-performance)
for ownership and fallback boundaries; implementation details live in the owning
modules and their tests.

## Current limits

Global Markdown dependencies and uncertain boundaries can require full parsing.
Length-changing edits shift contiguous buffers and rebase suffix offsets.
Outline construction, unusually long paragraphs/table rows, and retained native
shapes can add cost. Offline admission stages complete compatible state and has
document-size-dependent work. There is no universal frame-time guarantee.

## Reproduce

Use the mise-pinned toolchain and locked dependencies. Run profiles serially:

```sh
mise run bench
DOSTY_PROFILE_BLOCKS=10000 DOSTY_PROFILE_SAMPLES=60 mise run profile
DOSTY_PROFILE_BLOCKS=50000 DOSTY_PROFILE_SAMPLES=30 mise run profile
```

The ignored application harness in [app/performance.rs](../src/app/performance.rs)
drives a persistent widget tree and 720×800 software viewport with mixed Markdown
fixtures. It records event dispatch, App update, layout, drawing, software
rasterization, and per-interaction totals. Cold load and first frame are separate
observations. Image tasks are dropped; OS event delivery, window chrome, file IO,
decoded-image memory, GPU execution, and display presentation are excluded.

Component benchmarks measure narrower paths and cannot substitute for integrated
interaction timings. Specialized table and outline harnesses have their own
fixtures and exclusions. Inspect the relevant ignored test before interpreting
a result.

Compare identical fixtures, source revisions, toolchains, build settings, sample
counts, and renderer paths. Record hardware/platform, cold and warm cases,
median/tail values, memory scope, and interruptions. Do not sum phase percentiles
to infer a total percentile or add machine-dependent timing assertions to ordinary
tests. The [contribution guide](../CONTRIBUTING.md#validation-and-performance-evidence)
defines the required correctness and profiling checks for performance changes.

## Retained measurements

The [measurement index](performance/README.md) identifies historical baselines,
comparisons, raw logs, source hashes, and limitations. Those numbers describe the
recorded implementations; they are not continuously updated current benchmarks.

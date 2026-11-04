# Writing workflows measurements

Historical measurements; results apply to the recorded revisions and environments.

## Reproducible performance

Measured 2026-10-23, Apple M2 Max / 32 GiB, macOS 26.7.1, mise Rust 1.95.0, Iced 0.14, release / thin LTO / one codegen unit. The clean baseline is **34d6cc3**, which already contains the completed bounded offline-import optimization. Its preserved executable was used for fresh interaction profiles before editing and for a paired import/memory follow-up after final builds. [Environment](writing-workflows/environment.txt), [source hashes](writing-workflows/source-sha256.txt), and [executable hashes](writing-workflows/executables-sha256.txt) identify the runs.

All final builds and measurements ran serially, with the isolated native test process closed. Existing 910,450 / 4,574,450-byte fixtures, 720×800 viewport and 30 warm samples are unchanged. These integrated editor profiles include CRDT, source/projection/native adapters, layout, draw and software rasterization. They exclude window chrome, dialog/asset IO, GPU presentation and OS event delivery. The sample photograph is not decoded in this fixture. They are not a claim about end-to-end OS input latency. Full source/projection oracles remain outside timed paths.

Times below are total **p50 / p95 milliseconds**, before → after. Cold initialization and IME/history each have one sample; do not interpret them as distributions or speedup evidence. Raw logs retain all phases and maxima. The denser paragraph spacing deliberately raises initial resident fragments/rows from **44/52 to 52/60**; this is a visual change with more text on screen, rather than a performance optimization.

### 10k blocks

| Interaction | Before | After |
| --- | ---: | ---: |
| Cold source/App initialization, ms | 167.0 | 148.2 |
| Typing at start | 1.895 / 2.851 | 2.120 / 2.714 |
| Typing in middle | 1.728 / 2.205 | 2.176 / 2.809 |
| Typing at end | 1.527 / 1.912 | 1.805 / 2.212 |
| Disclosure | 2.913 / 3.399 | 3.084 / 3.622 |
| Cross-block selection | 1.282 / 1.332 | 1.586 / 1.802 |
| Scroll | 1.312 / 1.732 | 1.571 / 1.916 |
| IME commit (one sample) | 7.812 / 7.812 | 3.420 / 3.420 |
| Undo IME (one sample) | 2.020 / 2.020 | 2.823 / 2.823 |
| Redo IME (one sample) | 1.893 / 1.893 | 2.087 / 2.087 |

[Before](writing-workflows/before-10k.txt), [after](writing-workflows/after-10k.txt).

### 50k blocks

| Interaction | Before | After |
| --- | ---: | ---: |
| Cold source/App initialization, ms | 838.5 | 700.5 |
| Typing at start | 6.207 / 8.146 | 6.754 / 10.248 |
| Typing in middle | 4.933 / 6.840 | 5.784 / 7.670 |
| Typing at end | 5.358 / 6.649 | 5.065 / 5.757 |
| Disclosure | 14.809 / 18.672 | 14.858 / 16.006 |
| Cross-block selection | 2.640 / 3.249 | 2.652 / 2.888 |
| Scroll | 1.520 / 1.831 | 1.551 / 1.826 |
| IME commit (one sample) | 6.640 / 6.640 | 8.102 / 8.102 |
| Undo IME (one sample) | 5.209 / 5.209 | 4.813 / 4.813 |
| Redo IME (one sample) | 5.857 / 5.857 | 5.037 / 5.037 |

[Before](writing-workflows/before-50k.txt), [after](writing-workflows/after-50k.txt).

Several 10k medians increase by a fraction of a millisecond; rasterization does more work with the denser layout. No speedup is claimed for these workflow changes. At 50k, typing medians span 5.1–6.8 ms, scroll is 1.55 ms, and disclosure is 14.86 ms. Some tails remain outside a frame budget: the first end-of-document typing/capacity-growth event is **113.5 → 111.7 ms**. This remains a concrete limit of contiguous source/projection/native storage. Single IME/undo samples vary and do not establish a regression or improvement.

### Offline imports, retained state and memory

The bounded import work is retained: 32 certified independent windows at most 64 KiB each; larger/uncertain batches still use the full scanner without changing or dropping CRDT operations. Paired 30-sample, eight-edit imports measured:

| Blocks | Before p50 / p95, ms | After p50 / p95, ms |
| --- | ---: | ---: |
| 10,000 | 9.090 / 11.015 | 9.340 / 11.508 |
| 50,000 | 38.238 / 40.781 | 38.409 / 43.059 |

The paired baseline/final processes peaked at **563.8 / 569.1 MiB RSS**, with macOS peak memory footprints **512.8 / 517.7 MiB**. Current RSS after 50k churn was **487.5 / 485.8 MiB**; allocation retention varies and is not a live-object measurement. Earlier diagnostic final runs showed higher memory than the previous report, so a current paired baseline was run instead of attributing that difference to the feature. Both binaries now show similar process high-water marks. [Paired baseline](writing-workflows/before-collaboration.txt), [final](writing-workflows/collaboration.txt), and [earlier diagnostics](writing-workflows/earlier/README.md) are retained. No decoded-photo/cache allowance is included in those figures. Large text documents still have substantial transient memory costs.

After 200 insertion/undo pairs, complete checkpoint sizes were identical before/after: **916,791 → 921,029 bytes at 10k**, **4,580,919 → 4,585,159 bytes at 50k**. The 2,000-transaction component churn/reopen benchmark also passed; [benchmark output](writing-workflows/bench.txt) separates CRDT and source costs. Retention/undo policy has not been compacted away to obtain these numbers.

[Extended imports](writing-workflows/import-shapes.txt) run ten batches per shape and size, checking source/projection/native equivalence. The certified 32-window and structural paths pass 10/10; 128 edits deliberately fall back 10/10. A first large fallback remains a conspicuous offline-import stall (about 0.85 seconds in the final 50k run); this is not a real-time multiplayer guarantee. These extended cases are validation, not a matched speedup comparison.

Reproduce from the repository root:

```sh
mise run check
mise run release
RUST_TEST_THREADS=1 mise run bench
DOSTY_PROFILE_BLOCKS=10000 DOSTY_PROFILE_SAMPLES=30 mise run profile
DOSTY_PROFILE_BLOCKS=50000 DOSTY_PROFILE_SAMPLES=30 mise run profile
DOSTY_PROFILE_SAMPLES=30 mise exec -- cargo test --locked --release large_document_collaboration -- --ignored --nocapture --test-threads=1
DOSTY_PROFILE_SAMPLES=10 mise exec -- cargo test --locked --release large_offline_import_shapes -- --ignored --nocapture --test-threads=1
```

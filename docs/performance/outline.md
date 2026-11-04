# Outline measurements

Historical measurements; results apply to the recorded revisions and environments.

## Release measurements

Measured on 2026-10-25 on an Apple M2 Max, 32 GiB RAM, macOS 26.7.1 (25G241), aarch64, mise Rust 1.95.0, release/thin LTO/codegen-units=1. [Environment](outline/environment.txt) and [source checksums](outline/source-sha256.txt) identify this run. This is a feature baseline, not a matched before/after performance comparison.

```sh
mise exec -- cargo test --locked --release performance_outline_sections -- --ignored --nocapture --test-threads=1
DOSTY_PROFILE_SAMPLES=10 mise run profile
```

The dedicated fixture has ten blocks per chapter: a parent heading, introductory paragraph, child heading, and seven paragraphs containing formatting, a link, and Unicode. The 10k/50k cases contain 2,000/10,000 headings and 926,010/4,670,010 source bytes. Query timings use ten distinct queries. Move/undo timings use five repetitions moving the first complete chapter to the end, restoring exact source after each undo. These times include the application command, CRDT transaction, source parsing, outline refresh, projection, and native buffer synchronization; they exclude widget construction/layout, drawing, OS/GPU delivery, and file IO.

| Operation | 10k blocks, median / maximum ms | 50k blocks, median / maximum ms |
| --- | ---: | ---: |
| Heading query (n=10) | 0.276 / 0.397 | 1.407 / 1.478 |
| Move whole section (n=5) | 20.924 / 42.302 | 127.454 / 138.560 |
| Undo section move (n=5) | 17.227 / 19.348 | 108.950 / 111.230 |

Collapse-all and expand-all were single observations: 0.538/0.012 ms at 10k, and 2.276/0.060 ms at 50k. Application construction/load was 154.756 ms for the first 10k case and 173.374 ms for the subsequent 50k case. Those single loads include application initialization and different process-global warm state, so they do not establish cold file-open latency.

The existing large-document interaction profile passed at both sizes with ten samples per repeated interaction, including selection, disclosure, scrolling, IME commit, and undo/redo. It uses the separate mixed-content fixture documented in [performance.md](../performance.md), includes editor layout and software rasterization, and excludes the sidebar/window shell. Middle typing measured 2.129/2.941 ms median/maximum at 10k and 6.012/9.431 ms at 50k. End typing had a 118.659 ms maximum at 50k; document-wide structural work remains a limit. These small sample counts and CPU/software paths are not native frame-time or worst-case guarantees.

Raw evidence: [outline benchmark](outline/benchmark.txt), [interaction profiles](outline/profile.txt), [full checks](outline/check.txt), [release build](outline/release.txt), and [preview generation](outline/previews.txt). No timing assertions were added to ordinary tests.

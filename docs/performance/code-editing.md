# Code editing measurements

Historical measurements; results apply to the recorded revisions and environments.

## Measurements

Apple M2 Max, 32 GiB RAM, macOS 26.7.1 (25G241), aarch64, mise Rust 1.95.0, Iced 0.14, release optimization with thin LTO and one codegen unit. The 720×800 software renderer includes input dispatch, application update, layout, drawing, and rasterization; OS delivery, GPU/display presentation, file IO, and image decoding are excluded. Timings are observations, never regression-test assertions.

The dedicated benchmark types 20 characters in the middle of a 2,000-line Rust block. The comparison below isolates the initially implemented eager highlighting from the final lazy Read cache and incremental Live grammar cache. **The eager column is an intermediate implementation of this feature, not the original repository baseline.** Both runs used the same fixture and standalone serial test process.

| Phase | Eager median | Final median | Final p95 / max |
| --- | ---: | ---: | ---: |
| Application update | 147.47 ms | 0.41 ms | 0.55 / 1.04 ms |
| Layout | 145.68 ms | 5.59 ms | 6.99 / 7.13 ms |
| Total, including rasterization | 295.82 ms | 7.98 ms | 10.09 / 11.18 ms |

The final viewport retained 111 fragments and 111 visual rows. Syntax state may cover preceding lines to establish lexical context; the offscreen suffix is deferred.

The standard mixed-document profiles compare the repository state before this change with the final implementation, using identical fixtures and ten samples per interaction. Values are total median milliseconds unless marked as single cold samples.

| Measurement | 10k before | 10k final | 50k before | 50k final |
| --- | ---: | ---: | ---: | ---: |
| Document load, n=1 | 155.10 | 42.00 | 689.09 | 216.16 |
| First frame, n=1 | 11.53 | 29.52 | 24.38 | 43.15 |
| Typing/middle | 2.21 | 2.31 | 6.77 | 5.79 |
| Typing/end | 1.88 | 1.99 | 5.62 | 5.43 |
| Scroll | 1.48 | 1.55 | 1.56 | 1.42 |

The first frame now includes initialization of Live syntax highlighting; Read syntax initialization previously occurred during document load. The warm differences are small and mixed, so these samples do not establish a general throughput improvement. Ten samples give weak tail estimates: the 50k end-of-document structural sample still reached 114.25 ms, compared with 109.28 ms before. That existing fallback remains outside a universal frame-time guarantee. Cold values are single observations; process RSS was unavailable inside the profile sandbox. The general benchmark task runs its ignored tests concurrently; the standalone code report above is the timing comparison for long code.

Reproduce:

```sh
mise run check
mise run release
mise run bench
mise exec -- cargo test --locked --release performance_code_editor -- --ignored --nocapture --test-threads=1
DOSTY_PROFILE_BLOCKS=10000 DOSTY_PROFILE_SAMPLES=10 mise run profile
DOSTY_PROFILE_BLOCKS=50000 DOSTY_PROFILE_SAMPLES=10 mise run profile
mise exec -- cargo test --locked --lib code_surface_previews -- --ignored --nocapture --test-threads=1
```

Saved logs normalize trailing whitespace.

Raw evidence: [code eager](code-editing/code-eager.txt), [code incremental](code-editing/code-incremental.txt), [10k before](code-editing/before-10k.txt), [10k final](code-editing/after-10k.txt), [50k before](code-editing/before-50k.txt), [50k final](code-editing/after-50k.txt), [benchmarks](code-editing/bench.txt), [checks](code-editing/check.txt), [release](code-editing/release.txt), [previews](code-editing/previews.txt), [source hashes](code-editing/source-sha256.txt), [executable hashes](code-editing/executables-sha256.txt).

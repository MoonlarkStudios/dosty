# Dependency-update regression measurements — 2026-10-25

Historical smoke measurements for the Yrs retry repair, RFD update, and
manifest backports. These are not current dependency audit results.

Raw outputs: [before interactions](../validation/dependencies/before-profile.txt),
[after interactions](../validation/dependencies/after-profile.txt),
[before collaboration](../validation/dependencies/before-collaboration.txt),
[after collaboration](../validation/dependencies/after-collaboration.txt), and
[CRDT benchmark](../validation/dependencies/after-bench.txt).

Measurements used Apple M2 Max / 32 GiB, macOS 26.7.1, Rust 1.95.0, release thin LTO,
one codegen unit, and the same fixtures with 10 interaction samples per size
(400 churn steps). Baseline was `15d8baf`; the successor includes the Yrs retry fix,
RFD update and two manifest overrides. No application or fixture code changed.

| 50k-block operation | Before p50 / p95 (ms) | After p50 / p95 (ms) |
| --- | --- | --- |
| Middle typing, total | 6.112 / 9.577 | 6.732 / 9.852 |
| Distant jump, total | 16.213 / 16.989 | 14.992 / 19.172 |
| Remote eight-edit batch, total | 45.542 / 93.917 | 40.463 / 86.311 |
| Application churn, total | 5.667 / 11.422 | 4.929 / 5.692 |

Cold 50k load was 203.2 → 206.5 ms (one sample each). The 2,000-transaction CRDT
benchmark completed in 426.2 ms at 50k, with 89.9 ms reopen. These small, mixed
samples are regression smoke checks, not evidence of a general speedup. Ten samples
provide weak tail estimates; background host work and allocator history vary.
The profile uses software rasterization and does not measure the upgraded GPU
cache, OS input latency or presentation. The native smoke check exercises rendering
but is not a GPU performance benchmark.

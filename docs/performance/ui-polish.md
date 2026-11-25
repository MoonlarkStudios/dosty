# Typeface comparison for the UI refresh

Measured on November 25, 2026 on an Apple M2 Max with 32 GB RAM, macOS
27.2.1 (26B131), and mise Rust 1.95.0. The working tree was based on
`f2f04d2` on `codex/ui-polish`. Both typefaces ran from the same optimized
test binary, with thin LTO and one codegen unit.
The binary SHA-256 was
`e33b246e02666554946280f7af9ddab69eb652682152d5cc7fd465fadc81ed60`.

The 720×800 software renderer uses the same mixed Markdown fixture for each
typeface. It measures input dispatch, application update, layout, drawing, and
rasterization. Font registration precedes the source-load timer. Window chrome,
OS event delivery, document file IO, image decoding, GPU work, and display presentation
are outside this harness.

Run each case serially with an isolated configuration:

```sh
export DOSTY_CONFIG_DIR=$(mktemp -d)
for font in system-sans source-sans-3
do
    DOSTY_PROFILE_FONT="$font" DOSTY_PROFILE_BLOCKS=10000 DOSTY_PROFILE_SAMPLES=60 mise run profile
    DOSTY_PROFILE_FONT="$font" DOSTY_PROFILE_BLOCKS=50000 DOSTY_PROFILE_SAMPLES=30 mise run profile
done
```

## Results

Load and first frame are single observations. Warm cells show whole-interaction
p50 / p95 in milliseconds. The fixtures contain 910,450 and 4,574,450 bytes.

| Blocks | Typeface | Source load | First frame | Typing in the middle | Disclosure | Scrolling |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 10,000 | System Sans | 45.79 | 26.51 | 1.84 / 2.63 | 3.30 / 3.62 | 1.36 / 1.90 |
| 10,000 | Source Sans 3 | 41.92 | 26.47 | 2.32 / 2.93 | 3.66 / 3.98 | 1.75 / 2.24 |
| 50,000 | System Sans | 210.57 | 39.38 | 4.80 / 5.11 | 13.87 / 14.54 | 1.27 / 1.77 |
| 50,000 | Source Sans 3 | 210.45 | 41.90 | 5.75 / 8.01 | 15.60 / 16.27 | 1.73 / 2.28 |

Source Sans 3 had higher median times for typing, disclosure, and scrolling in
the first pair. Repeating the 50,000-block case with Source Sans 3 first gave
typing p50 / p95 of 5.37 / 5.55 ms for Source Sans 3 and 5.05 / 5.82 ms for
System Sans. Tail results varied across runs. The first pair also included
end-of-document typing samples above 110 ms with both faces. RSS was unavailable
in the profile sandbox. The results describe these fixtures and runs.

Recorded test output for the first pair:
[System Sans 10k](ui-polish/system-sans-10000.txt),
[Source Sans 3 10k](ui-polish/source-sans-3-10000.txt),
[System Sans 50k](ui-polish/system-sans-50000.txt), and
[Source Sans 3 50k](ui-polish/source-sans-3-50000.txt).
The reverse-order pair used the same command and 30 samples:
[Source Sans 3 50k](ui-polish/source-sans-3-50000-repeat.txt) and
[System Sans 50k](ui-polish/system-sans-50000-repeat.txt).

## Scrollbar placement follow-up

The pane-edge scrollbar was profiled on November 25, 2026 with Source Sans 3
using the same machine, toolchain, fixture, and sample counts. The test binary
SHA-256 was `51173c7e84e73a04ce799c5001bf29fa777d2b1959e87c291a62b17ffcc14619`.
The fixed-width harness exercises the editor's paint and input paths. Pane
margins and the docked Outline are covered by full-window software previews.

| Blocks | Source load, ms | First frame, ms | Typing in the middle, p50 / p95 ms | Scrolling, p50 / p95 ms |
| ---: | ---: | ---: | ---: | ---: |
| 10,000 | 39.04 | 25.13 | 2.34 / 2.90 | 1.75 / 2.12 |
| 50,000 | 211.58 | 40.78 | 5.56 / 7.15 | 1.72 / 2.25 |

The median wheel-scroll times were within 0.01 ms of the first Source Sans 3
run. These are individual runs of the software harness. Recorded output is
available for [10k blocks](ui-polish/scrollbar-10000.txt) and
[50k blocks](ui-polish/scrollbar-50000.txt).

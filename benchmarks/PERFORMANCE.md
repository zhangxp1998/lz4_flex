# Safe decompression optimization — 2026-11-02

The objective is **decompression throughput**, not compression speed.
Baseline: `0c027fd21e7e3545c5b409f11699ac0c6a680111` (lz4_flex 0.14.0).

## Implementation

- In `SliceSink::extend_from_within_overlapping`, keep the byte loop for matches
  up to 32 bytes. For longer matches, copy an already initialized prefix into a
  disjoint destination slice, doubling the available prefix on each iteration.
  This removes the serial byte-by-byte dependency in long repeating matches.
- Keep the bulk-copy helper cold and force-inline `duplicate_overlapping_slice`
  so offset validation and the existing offset-1 fill stay in the hot path.
- No new unsafe code, allocation, format change, public API change or feature
  change. `src/block/compress.rs` is identical to the baseline. The optimization
  targets `safe-decode`, which is enabled by default; it does not optimize the
  separate unsafe decoder.
- Add a regression test covering 1,176 combinations of prefix, offset, match
  length, exact output capacity and dirty suffix. Expected bytes come from the
  simple bytewise LZ4 recurrence; both output position and untouched tail are
  checked.

## Results

Median **decompression MiB/s**. Improvement is throughput change, not the
percentage reduction in elapsed time.

| Input | Mode | Baseline | Optimized | Change |
|---|---|---:|---:|---:|
| Android kernel Image | 4 KiB blocks | 4399.8 | 4542.7 | +3.25% |
| Android kernel Image | 64 KiB blocks | 4628.7 | 5086.0 | +9.88% |
| Android kernel Image | 256 KiB blocks | 4481.3 | 5060.4 | +12.92% |
| Android kernel Image | Default frame | 3628.9 | 4080.6 | +12.45% |
| Android source subset | 4 KiB blocks | 3992.7 | 4064.0 | +1.79% |
| Android source subset | 64 KiB blocks | 4526.4 | 4547.3 | +0.46% |
| Android source subset | 256 KiB blocks | 4557.0 | 4579.5 | +0.49% |
| Android source subset | Default frame | 4107.7 | 4099.0 | -0.21% |
| compression_66k_JSON.txt | 64 KiB blocks | 12119.4 | 12123.3 | +0.03% |
| dickens.txt | 64 KiB blocks | 5166.7 | 5255.5 | +1.72% |
| xml_collection.xml | 64 KiB blocks | 5005.5 | 5009.4 | +0.08% |
| reymont.pdf | 64 KiB blocks | 4706.8 | 4773.0 | +1.41% |
| logo.jpg | 64 KiB blocks | 102483.6 | 102821.9 | +0.33% |

The kernel **binary** is the clear beneficiary. Text/source data are largely
unchanged, with a noise-sized negative result for source frames. JPEG throughput
mostly measures literal copies in warm memory, not useful compression.
These are x86_64 host measurements using an Android kernel as input, **not ARM
execution or Android boot-time measurements**.

The earlier combined run also recorded some compression throughput reductions
(e.g. Image 64 KiB -3.88%, Dickens -5.25%). Compression source and compressed
size/fingerprint are unchanged. A code-layout explanation is plausible but not
proven; compression investigation was stopped when the objective was clarified.
Temporary compressor edits were reverted.

## Method and inputs

- AMD Ryzen 9 9950X; Linux x86_64; pinned to CPU 4. Existing powersave governor
  and boost settings were left unchanged. No fixed-frequency claim is made.
- rustc 1.97.1 (`8bab26f4f`, LLVM 22.1.6), release opt-level 3,
  `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1`, `CARGO_PROFILE_RELEASE_DEBUG=1`.
  Default features; no target-cpu/native flags or unsafe-decoder shortcut.
- `kernel_bench` reads inputs and prepares compressed data before timing.
  Every input is round-trip checked. Block timing reuses allocated output;
  frame timing uses the default frame API and includes decoder-internal
  allocation, while reusing the caller's output Vec. No disk I/O is timed.
- Baseline/optimized process order ABBA; three samples of at least 0.2 seconds
  per process, six samples per variant and case. Image 64 KiB and frame repeat
  ABBA twice, giving twelve samples per variant. One warm-up per operation.
- All 13 cases have the same compressed length and FNV64 fingerprint between
  variants. Full source/binary SHA-256 hashes and raw samples are retained.
- Image: 42,928,640 bytes, from a local Android common 6.18 kernel build.
  SHA-256: `c44f520ea37cea41438e91cd84ea224a82ae8932ded88514caa7de4642454974`.
- Source subset: 16,875,523 bytes, concatenated selected tracked `.c`, `.h`,
  `.S` files from the clean Android common checkout at
  `f37da0928b116686bb09737e622a22a54180577d`. Selection list is in the manifest;
  this is a subset, not a claim of full-source coverage.
  SHA-256: `4820eecf52b215278a22d3094d2b0056ac65333ad54670aa6ae7d9118c28c63a`.

## Validation

| Check | Result |
|---|---|
| Default `cargo test`, `PROPTEST_CASES=1024` | 77 passed, 1 ignored |
| `--no-default-features --features frame,checked-decode` | 80 passed, 1 ignored |
| `--no-default-features --features frame,safe-decode,checked-decode` | 81 passed, 1 ignored |
| `--no-default-features --features frame,safe-encode,checked-decode` | 81 passed, 1 ignored |
| `no_alloc` integration tests, safe and unsafe variants | 1 passed each |
| no_std checks: no features, safe-encode, safe-decode, both, alloc | All passed |
| Final release library and benchmark build; formatting and diff checks | Passed |

The suites include C LZ4 bidirectional interoperability and independent/linked
frame coverage. The final rebuilt benchmark is byte-identical to the measured
optimized executable, SHA-256
`3fc9b181e8af91ef2cf05a14804f07501f389bde3a6f491d2cd83afdfd87697c`.

Known unrelated limitation: with only `std,safe-decode` enabled and `frame`
disabled, all 62 unit/integration tests pass (1 ignored), but two existing
unconditional frame examples in `src/lib.rs` fail doctests. That file is
unchanged. The mixed safe-decode configuration with frame enabled passes above.
Nightly/Miri and ARM performance were not tested.

## Reproduce locally

From the repository root:

```sh
cargo build --release
CARGO_PROFILE_RELEASE_DEBUG=1 CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
  cargo build --release --manifest-path benchmarks/Cargo.toml --bin kernel_bench

# Decompression only; the original measured binaries are retained locally.
for variant in baseline optimized optimized baseline; do
  taskset -c 4 "benchmarks/target/perf/$variant" \
    benchmarks/target/perf/android-Image block:65536 0.2 3 d
done

# Run the current build; substitute block:4096, block:262144, or frame.
taskset -c 4 benchmarks/target/release/kernel_bench \
  benchmarks/target/perf/android-Image frame 0.2 3 d
```

`kernel_bench FILE block:SIZE|frame SECONDS SAMPLES [c|d|both]` also accepts
other nonempty local files. CSV output contains input/compressed sizes,
fingerprint, iterations, seconds and MiB/s.

Local ignored artifacts under `benchmarks/target/perf/` (not uploaded to GitHub):

- `manifest.json`: revisions, selected source files, toolchain, hashes/settings.
- `final-results.json` and `final-raw.csv`: all 13 cases and original samples.
- `test-results.json` and `final-test-*.log`: exact commands and test outcomes.
- `baseline`, `optimized`, `android-Image`, `android-source`: original binaries
  and copied inputs. Kernel sources/images were not modified.
- `run_final.py`: original combined comparison driver. Rerunning it overwrites
  its local results files; the decompression-only commands above do not.

The compiled library is `target/release/liblz4_flex.rlib`.

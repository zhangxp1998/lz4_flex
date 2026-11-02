//! Reproducible, in-memory benchmark for caller-provided data.
//! Usage: kernel_bench FILE block:65536|frame SECONDS SAMPLES [c|d|both]
use std::hint::black_box;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

fn fingerprint(bytes: &[u8], mut hash: u64) -> u64 {
    for &byte in bytes {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    hash
}

fn measure(mut f: impl FnMut(), bytes: usize, seconds: f64, samples: usize, name: &str) {
    f();
    for sample in 0..samples {
        let start = Instant::now();
        let mut iterations = 0;
        while start.elapsed() < Duration::from_secs_f64(seconds) || iterations == 0 {
            f();
            iterations += 1;
        }
        let elapsed = start.elapsed().as_secs_f64();
        println!(
            "sample,{name},{sample},{iterations},{elapsed:.9},{:.3}",
            bytes as f64 * iterations as f64 / elapsed / 1048576.0
        );
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        args.len() >= 5,
        "FILE block:SIZE|frame SECONDS SAMPLES [c|d|both]"
    );
    let input = std::fs::read(&args[1]).unwrap();
    assert!(!input.is_empty());
    let mode = &args[2];
    let seconds: f64 = args[3].parse().unwrap();
    let samples: usize = args[4].parse().unwrap();
    let operation = args.get(5).map(String::as_str).unwrap_or("both");
    assert!(matches!(operation, "c" | "d" | "both"));
    let mut hash = 0xcbf29ce484222325;
    if let Some(chunk) = mode.strip_prefix("block:") {
        let chunk: usize = chunk.parse().unwrap();
        assert_ne!(chunk, 0);
        let compressed: Vec<_> = input.chunks(chunk).map(lz4_flex::block::compress).collect();
        let mut decoded = vec![0; chunk.min(input.len())];
        for (src, packed) in input.chunks(chunk).zip(&compressed) {
            let size = lz4_flex::block::decompress_into(packed, &mut decoded).unwrap();
            assert_eq!(&decoded[..size], src);
            hash = fingerprint(packed, hash);
        }
        let compressed_size: usize = compressed.iter().map(Vec::len).sum();
        println!(
            "data,{},{mode},{},{compressed_size},{hash:016x}",
            args[1],
            input.len()
        );
        let mut output = vec![0; lz4_flex::block::get_maximum_output_size(chunk.min(input.len()))];
        if operation != "d" {
            measure(
                || {
                    for src in input.chunks(chunk) {
                        let size =
                            lz4_flex::block::compress_into(black_box(src), &mut output).unwrap();
                        black_box(&output[..size]);
                    }
                },
                input.len(),
                seconds,
                samples,
                "compress",
            );
        }
        if operation != "c" {
            measure(
                || {
                    for packed in &compressed {
                        let size =
                            lz4_flex::block::decompress_into(black_box(packed), &mut decoded)
                                .unwrap();
                        black_box(&decoded[..size]);
                    }
                },
                input.len(),
                seconds,
                samples,
                "decompress",
            );
        }
    } else {
        assert_eq!(mode, "frame");
        let mut encoder = lz4_flex::frame::FrameEncoder::new(Vec::new());
        encoder.write_all(&input).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut decoded = Vec::with_capacity(input.len());
        lz4_flex::frame::FrameDecoder::new(compressed.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, input);
        hash = fingerprint(&compressed, hash);
        println!(
            "data,{},{mode},{},{},{hash:016x}",
            args[1],
            input.len(),
            compressed.len()
        );
        let mut output = Vec::with_capacity(compressed.len());
        if operation != "d" {
            measure(
                || {
                    output.clear();
                    let mut encoder = lz4_flex::frame::FrameEncoder::new(&mut output);
                    encoder.write_all(black_box(&input)).unwrap();
                    encoder.finish().unwrap();
                    black_box(&output);
                },
                input.len(),
                seconds,
                samples,
                "compress",
            );
        }
        if operation != "c" {
            measure(
                || {
                    decoded.clear();
                    lz4_flex::frame::FrameDecoder::new(black_box(compressed.as_slice()))
                        .read_to_end(&mut decoded)
                        .unwrap();
                    black_box(&decoded);
                },
                input.len(),
                seconds,
                samples,
                "decompress",
            );
        }
    }
}

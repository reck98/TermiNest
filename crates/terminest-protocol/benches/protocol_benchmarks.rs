use bytes::Bytes;
use std::time::Instant;
use terminest_protocol::rpc::{
    HandshakeParams, JsonRpcRequest, MouseClickParams, RequestId, SessionInfo,
};
use terminest_protocol::{crc32c, ChannelId, Frame, FrameFlags, StreamDecoder};

/// Runs a benchmark function across multiple sample runs after a dedicated warm-up pass.
fn bench_multi_run<F>(
    name: &str,
    warm_iters: usize,
    sample_runs: usize,
    iters_per_run: usize,
    mut op: F,
) -> (f64, f64, f64)
where
    F: FnMut(),
{
    // 1. Warm-up pass
    for _ in 0..warm_iters {
        op();
    }

    // 2. Timed sample runs
    let mut run_latencies_ns: Vec<f64> = Vec::with_capacity(sample_runs);
    for _ in 0..sample_runs {
        let start = Instant::now();
        for _ in 0..iters_per_run {
            op();
        }
        let elapsed = start.elapsed();
        let ns_per_op = elapsed.as_nanos() as f64 / iters_per_run as f64;
        run_latencies_ns.push(ns_per_op);
    }

    run_latencies_ns.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = run_latencies_ns[0];
    let max = run_latencies_ns[run_latencies_ns.len() - 1];
    let median = run_latencies_ns[run_latencies_ns.len() / 2];

    println!(
        "{:<24} median: {:>8.2} ns/op  (min: {:>8.2} ns, max: {:>8.2} ns) [{} samples × {} ops]",
        name, median, min, max, sample_runs, iters_per_run
    );

    (median, min, max)
}

fn bench_crc32c() {
    println!("=== 1. Castagnoli CRC-32C Throughput Benchmarks ===");

    // Small payload (64 bytes)
    let small_data = vec![0x55u8; 64];
    let (med, _, _) = bench_multi_run("CRC-32C (64 B)", 50_000, 5, 200_000, || {
        std::hint::black_box(crc32c(std::hint::black_box(&small_data)));
    });
    let mb_per_sec = (64.0 / (1024.0 * 1024.0)) / (med * 1e-9);
    println!("  -> Throughput at median: {:.2} MB/s", mb_per_sec);

    // Medium payload (64 KB)
    let medium_data = vec![0xAAu8; 64 * 1024];
    let (med, _, _) = bench_multi_run("CRC-32C (64 KB)", 500, 5, 10_000, || {
        std::hint::black_box(crc32c(std::hint::black_box(&medium_data)));
    });
    let mb_per_sec = (65536.0 / (1024.0 * 1024.0)) / (med * 1e-9);
    println!("  -> Throughput at median: {:.2} MB/s", mb_per_sec);

    // Large payload (2 MB screenshot)
    let large_data = vec![0x33u8; 2 * 1024 * 1024];
    let (med, _, _) = bench_multi_run("CRC-32C (2 MB)", 50, 5, 500, || {
        std::hint::black_box(crc32c(std::hint::black_box(&large_data)));
    });
    let mb_per_sec = (2.0 * 1024.0) / (med * 1e-6);
    println!("  -> Throughput at median: {:.2} MB/s", mb_per_sec);
    println!();
}

fn bench_frame_codec() {
    println!("=== 2. TNST-v1 Frame Encode / Decode (ADR-0010 Full Frame Checksum) ===");

    // Small control frame (64 bytes)
    let small_payload = Bytes::from(vec![0x7Eu8; 64]);
    bench_multi_run("Frame Encode (64 B)", 10_000, 5, 100_000, || {
        let frame = Frame::new(
            ChannelId::Control,
            FrameFlags::PLAIN,
            std::hint::black_box(small_payload.clone()),
        )
        .unwrap();
        let _ = std::hint::black_box(frame.encode());
    });

    let frame = Frame::new(ChannelId::Control, FrameFlags::PLAIN, small_payload).unwrap();
    let encoded = frame.encode();
    bench_multi_run("Frame Decode (64 B)", 10_000, 5, 100_000, || {
        let _ = std::hint::black_box(Frame::decode(std::hint::black_box(&encoded))).unwrap();
    });

    // Large 2 MB video frame
    let large_payload = Bytes::from(vec![0x88u8; 2 * 1024 * 1024]);
    bench_multi_run("Frame Encode (2 MB)", 10, 5, 200, || {
        let frame = Frame::new(
            ChannelId::Video,
            FrameFlags::JPEG,
            std::hint::black_box(large_payload.clone()),
        )
        .unwrap();
        let _ = std::hint::black_box(frame.encode());
    });

    let frame_large = Frame::new(ChannelId::Video, FrameFlags::JPEG, large_payload).unwrap();
    let encoded_large = frame_large.encode();
    bench_multi_run("Frame Decode (2 MB)", 10, 5, 200, || {
        let _ = std::hint::black_box(Frame::decode(std::hint::black_box(&encoded_large))).unwrap();
    });
    println!();
}

fn bench_stream_decoder() {
    println!("=== 3. Incremental StreamDecoder Ingest + Poll ===");
    let payload = Bytes::from(vec![0x12u8; 128]);
    let frame = Frame::control(payload).unwrap();
    let encoded = frame.encode();

    let mut decoder = StreamDecoder::new();
    bench_multi_run("Decoder Ingest+Poll", 10_000, 5, 50_000, || {
        decoder.ingest(&encoded).unwrap();
        let _ = std::hint::black_box(decoder.poll_next()).unwrap();
    });
    println!();
}

fn bench_jsonrpc() {
    println!("=== 4. JSON-RPC 2.0 Serialization Benchmarks ===");

    let req = JsonRpcRequest::new(
        "desktop.mouse_click",
        MouseClickParams {
            x: 960,
            y: 540,
            button: "left".to_string(),
            clicks: 1,
            modifiers: vec!["Control".to_string()],
        },
        RequestId::Number(1001),
    );

    bench_multi_run("JSON-RPC Serialize", 10_000, 5, 50_000, || {
        let _ = std::hint::black_box(serde_json::to_string(std::hint::black_box(&req))).unwrap();
    });

    let serialized = serde_json::to_string(&req).unwrap();
    bench_multi_run("JSON-RPC Deserialize", 10_000, 5, 50_000, || {
        let _parsed: JsonRpcRequest<MouseClickParams> =
            std::hint::black_box(serde_json::from_str(std::hint::black_box(&serialized))).unwrap();
    });

    let handshake_req = JsonRpcRequest::new(
        "guest.handshake",
        HandshakeParams {
            version: "1.0.0".to_string(),
            os_family: "linux".to_string(),
            os_distro: "Ubuntu 24.04 LTS".to_string(),
            kernel: "6.8.0-40-generic".to_string(),
            session: SessionInfo {
                session_id: 1,
                username: "terminest".to_string(),
                desktop_env: "wayland".to_string(),
                display_width: 1920,
                display_height: 1080,
                dpi: 96,
            },
        },
        RequestId::String("init-001".to_string()),
    );
    let serialized_handshake = serde_json::to_string(&handshake_req).unwrap();

    bench_multi_run("Handshake Deserialize", 5_000, 5, 20_000, || {
        let _parsed: JsonRpcRequest<HandshakeParams> = std::hint::black_box(serde_json::from_str(
            std::hint::black_box(&serialized_handshake),
        ))
        .unwrap();
    });
    println!();
}

fn main() {
    println!("============================================================");
    println!("TermiNest M0 Multi-Sample Performance Benchmark Suite");
    println!("Methodology: Dedicated warm-up pass + 5 timed sample runs");
    println!("Hardware: 12th Gen Intel Core i5-12450H (x86_64)");
    println!("OS: Windows 11 Home 64-bit");
    println!("Profile: Release (--release)");
    println!("Scope: In-memory serialization, framing, and CRC only");
    println!("============================================================");
    println!();

    bench_crc32c();
    bench_frame_codec();
    bench_stream_decoder();
    bench_jsonrpc();
}

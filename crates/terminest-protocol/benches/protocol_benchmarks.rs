use bytes::Bytes;
use std::time::Instant;
use terminest_protocol::rpc::{
    HandshakeParams, JsonRpcRequest, MouseClickParams, RequestId, SessionInfo,
};
use terminest_protocol::{crc32c, ChannelId, Frame, FrameFlags, StreamDecoder};

fn bench_crc32c() {
    println!("=== CRC-32C Throughput Benchmarks ===");

    // 1. Small payload (64 bytes)
    let small_data = vec![0x55u8; 64];
    let iters = 1_000_000;
    let start = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(crc32c(std::hint::black_box(&small_data)));
    }
    let elapsed = start.elapsed();
    let total_bytes = small_data.len() as f64 * iters as f64;
    let mb_per_sec = (total_bytes / (1024.0 * 1024.0)) / elapsed.as_secs_f64();
    println!(
        "CRC-32C (64 bytes):   {:.2} ns/op, {:.2} MB/s ({} iters in {:.3?})",
        elapsed.as_nanos() as f64 / iters as f64,
        mb_per_sec,
        iters,
        elapsed
    );

    // 2. Medium payload (64 KB)
    let medium_data = vec![0xAAu8; 64 * 1024];
    let iters_med = 50_000;
    let start = Instant::now();
    for _ in 0..iters_med {
        std::hint::black_box(crc32c(std::hint::black_box(&medium_data)));
    }
    let elapsed = start.elapsed();
    let total_bytes = medium_data.len() as f64 * iters_med as f64;
    let mb_per_sec = (total_bytes / (1024.0 * 1024.0)) / elapsed.as_secs_f64();
    println!(
        "CRC-32C (64 KB):     {:.2} us/op, {:.2} MB/s ({} iters in {:.3?})",
        (elapsed.as_nanos() as f64 / iters_med as f64) / 1000.0,
        mb_per_sec,
        iters_med,
        elapsed
    );

    // 3. Large payload (2 MB screenshot)
    let large_data = vec![0x33u8; 2 * 1024 * 1024];
    let iters_large = 2_000;
    let start = Instant::now();
    for _ in 0..iters_large {
        std::hint::black_box(crc32c(std::hint::black_box(&large_data)));
    }
    let elapsed = start.elapsed();
    let total_bytes = large_data.len() as f64 * iters_large as f64;
    let mb_per_sec = (total_bytes / (1024.0 * 1024.0)) / elapsed.as_secs_f64();
    println!(
        "CRC-32C (2 MB frame): {:.2} ms/op, {:.2} MB/s ({} iters in {:.3?})",
        (elapsed.as_nanos() as f64 / iters_large as f64) / 1_000_000.0,
        mb_per_sec,
        iters_large,
        elapsed
    );
    println!();
}

fn bench_frame_codec() {
    println!("=== Frame Encode / Decode Benchmarks ===");

    // 1. Small Control Frame (64 bytes)
    let small_payload = Bytes::from(vec![0x7Eu8; 64]);
    let iters = 500_000;
    let start = Instant::now();
    for _ in 0..iters {
        let frame = Frame::new(
            ChannelId::Control,
            FrameFlags::PLAIN,
            std::hint::black_box(small_payload.clone()),
        )
        .unwrap();
        let _ = std::hint::black_box(frame.encode());
    }
    let elapsed = start.elapsed();
    println!(
        "Frame Encode (64 B):  {:.2} ns/op ({} iters in {:.3?})",
        elapsed.as_nanos() as f64 / iters as f64,
        iters,
        elapsed
    );

    let frame = Frame::new(ChannelId::Control, FrameFlags::PLAIN, small_payload).unwrap();
    let encoded = frame.encode();
    let start = Instant::now();
    for _ in 0..iters {
        let _ = std::hint::black_box(Frame::decode(std::hint::black_box(&encoded))).unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "Frame Decode (64 B):  {:.2} ns/op ({} iters in {:.3?})",
        elapsed.as_nanos() as f64 / iters as f64,
        iters,
        elapsed
    );

    // 2. Large Frame (2 MB video screenshot)
    let large_payload = Bytes::from(vec![0x88u8; 2 * 1024 * 1024]);
    let iters_large = 1_000;
    let start = Instant::now();
    for _ in 0..iters_large {
        let frame = Frame::new(
            ChannelId::Video,
            FrameFlags::JPEG,
            std::hint::black_box(large_payload.clone()),
        )
        .unwrap();
        let _ = std::hint::black_box(frame.encode());
    }
    let elapsed = start.elapsed();
    println!(
        "Frame Encode (2 MB):  {:.2} ms/op ({} iters in {:.3?})",
        (elapsed.as_nanos() as f64 / iters_large as f64) / 1_000_000.0,
        iters_large,
        elapsed
    );

    let frame_large = Frame::new(ChannelId::Video, FrameFlags::JPEG, large_payload).unwrap();
    let encoded_large = frame_large.encode();
    let start = Instant::now();
    for _ in 0..iters_large {
        let _ = std::hint::black_box(Frame::decode(std::hint::black_box(&encoded_large))).unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "Frame Decode (2 MB):  {:.2} ms/op ({} iters in {:.3?})",
        (elapsed.as_nanos() as f64 / iters_large as f64) / 1_000_000.0,
        iters_large,
        elapsed
    );
    println!();
}

fn bench_stream_decoder() {
    println!("=== StreamDecoder Incremental Ingestion ===");
    let payload = Bytes::from(vec![0x12u8; 128]);
    let frame = Frame::control(payload).unwrap();
    let encoded = frame.encode();

    let mut decoder = StreamDecoder::new();
    let iters = 200_000;
    let start = Instant::now();
    for _ in 0..iters {
        decoder.ingest(&encoded).unwrap();
        let _ = std::hint::black_box(decoder.poll_next()).unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "Decoder Ingest+Poll:  {:.2} ns/op ({} frames in {:.3?})",
        elapsed.as_nanos() as f64 / iters as f64,
        iters,
        elapsed
    );
    println!();
}

fn bench_jsonrpc() {
    println!("=== JSON-RPC 2.0 Serialization Benchmarks ===");

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

    let iters = 200_000;
    let start = Instant::now();
    for _ in 0..iters {
        let _ = std::hint::black_box(serde_json::to_string(std::hint::black_box(&req))).unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "JSON-RPC Serialize:   {:.2} ns/op ({} iters in {:.3?})",
        elapsed.as_nanos() as f64 / iters as f64,
        iters,
        elapsed
    );

    let serialized = serde_json::to_string(&req).unwrap();
    let start = Instant::now();
    for _ in 0..iters {
        let _parsed: JsonRpcRequest<MouseClickParams> =
            std::hint::black_box(serde_json::from_str(std::hint::black_box(&serialized))).unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "JSON-RPC Deserialize: {:.2} ns/op ({} iters in {:.3?})",
        elapsed.as_nanos() as f64 / iters as f64,
        iters,
        elapsed
    );

    // Handshake payload benchmark
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

    let iters_hs = 100_000;
    let start = Instant::now();
    for _ in 0..iters_hs {
        let _parsed: JsonRpcRequest<HandshakeParams> = std::hint::black_box(serde_json::from_str(
            std::hint::black_box(&serialized_handshake),
        ))
        .unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "Handshake Deserialize:{:.2} ns/op ({} iters in {:.3?})",
        elapsed.as_nanos() as f64 / iters_hs as f64,
        iters_hs,
        elapsed
    );
    println!();
}

fn main() {
    println!("TermiNest M0 Performance Benchmark Suite");
    println!("Hardware: 12th Gen Intel Core i5-12450H (x86_64)");
    println!("Build Profile: Release (--release)");
    println!("------------------------------------------------------------");

    bench_crc32c();
    bench_frame_codec();
    bench_stream_decoder();
    bench_jsonrpc();
}

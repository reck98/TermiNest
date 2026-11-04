use bytes::Bytes;
use std::time::Duration;
use terminest_protocol::{ChannelId, Frame, FrameFlags};
use terminest_transport::{FramedTransport, InMemoryTransport, Transport};
use tokio::io::duplex;
use tokio::time::timeout;

#[tokio::test]
async fn test_in_memory_concurrent_ping_pong() {
    let (mut host, mut guest) = InMemoryTransport::pair(64);

    let host_task = tokio::spawn(async move {
        for i in 0..100 {
            let msg = format!("host-msg-{}", i);
            host.send(Frame::control(Bytes::from(msg)).unwrap())
                .await
                .unwrap();

            let reply = host.recv().await.unwrap().expect("Expected guest reply");
            assert!(reply.payload.starts_with(b"guest-echo-"));
        }
        host.close().await.unwrap();
    });

    let guest_task = tokio::spawn(async move {
        while let Some(frame) = guest.recv().await.unwrap() {
            let req_str = std::str::from_utf8(&frame.payload).unwrap();
            let echo = format!("guest-echo-{}", req_str);
            guest
                .send(Frame::control(Bytes::from(echo)).unwrap())
                .await
                .unwrap();
        }
    });

    let (r1, r2) = tokio::join!(host_task, guest_task);
    r1.unwrap();
    r2.unwrap();
}

#[tokio::test]
async fn test_framed_transport_timeout() {
    let (_client_io, server_io) = duplex(1024);
    let mut server = FramedTransport::new(server_io);

    // No client data sent; recv should block until timeout
    let result = timeout(Duration::from_millis(50), server.recv()).await;
    assert!(result.is_err(), "Expected timeout");
}

#[tokio::test]
async fn test_framed_transport_large_video_frame() {
    let (client_io, server_io) = duplex(65536);
    let mut client = FramedTransport::new(client_io);
    let mut server = FramedTransport::new(server_io);

    // 1 MB fake video frame payload
    let large_payload = vec![0x42u8; 1024 * 1024];
    let video_frame = Frame::new(
        ChannelId::Video,
        FrameFlags::JPEG,
        Bytes::from(large_payload.clone()),
    )
    .unwrap();

    let client_task = tokio::spawn(async move {
        client.send(video_frame).await.unwrap();
    });

    let received = server.recv().await.unwrap().expect("Expected video frame");
    client_task.await.unwrap();

    assert_eq!(received.header.channel_id, ChannelId::Video);
    assert!(received.header.flags.is_jpeg());
    assert_eq!(received.payload.len(), 1024 * 1024);
    assert_eq!(received.payload[0], 0x42);
}

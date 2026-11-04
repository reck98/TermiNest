pub mod channel;
pub mod crc;
pub mod decoder;
pub mod error;
pub mod frame;
pub mod rpc;

pub use channel::ChannelId;
pub use crc::crc32c;
pub use decoder::StreamDecoder;
pub use error::ProtocolError;
pub use frame::{Frame, FrameFlags, FrameHeader, HEADER_SIZE, MAGIC, MAX_PAYLOAD_SIZE};

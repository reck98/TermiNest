use crate::error::ProtocolError;

/// Logical multiplexed channels defined in TNST-v1 specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ChannelId {
    /// Channel 0: Control Channel (JSON-RPC 2.0 messages).
    Control = 0x0000,
    /// Channel 1: Video Stream Channel (Screen capture frames).
    Video = 0x0001,
    /// Channel 2: File Transfer Channel (Chunked file streams).
    File = 0x0002,
    /// Channel 3: Telemetry & Metrics Channel (Guest health, CPU, RAM).
    Telemetry = 0x0003,
}

impl ChannelId {
    /// Attempts to convert a raw u16 wire identifier into a known `ChannelId`.
    pub fn from_raw(raw: u16) -> Result<Self, ProtocolError> {
        match raw {
            0x0000 => Ok(ChannelId::Control),
            0x0001 => Ok(ChannelId::Video),
            0x0002 => Ok(ChannelId::File),
            0x0003 => Ok(ChannelId::Telemetry),
            other => Err(ProtocolError::UnknownChannel(other)),
        }
    }

    /// Returns the raw u16 wire identifier.
    #[inline]
    pub const fn to_raw(self) -> u16 {
        self as u16
    }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelId::Control => write!(f, "Control (0x0000)"),
            ChannelId::Video => write!(f, "Video (0x0001)"),
            ChannelId::File => write!(f, "File (0x0002)"),
            ChannelId::Telemetry => write!(f, "Telemetry (0x0003)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_channel_conversions() {
        assert_eq!(ChannelId::from_raw(0x0000).unwrap(), ChannelId::Control);
        assert_eq!(ChannelId::from_raw(0x0001).unwrap(), ChannelId::Video);
        assert_eq!(ChannelId::from_raw(0x0002).unwrap(), ChannelId::File);
        assert_eq!(ChannelId::from_raw(0x0003).unwrap(), ChannelId::Telemetry);
    }

    #[test]
    fn test_invalid_channel_rejected() {
        assert!(matches!(
            ChannelId::from_raw(0x0004),
            Err(ProtocolError::UnknownChannel(0x0004))
        ));
        assert!(matches!(
            ChannelId::from_raw(0xFFFF),
            Err(ProtocolError::UnknownChannel(0xFFFF))
        ));
    }
}

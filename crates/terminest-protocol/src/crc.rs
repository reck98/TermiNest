//! Castagnoli CRC-32C checksum calculation (polynomial 0x1EDC6F41, reflected 0x82F63B78).
//!
//! Provides verified hardware/software Castagnoli CRC-32C for frame integrity verification.

/// Precomputed 256-entry lookup table for Castagnoli CRC-32C.
const CRC32C_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0x82F63B78;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// Computes the Castagnoli CRC-32C checksum of the given byte slice.
///
/// An empty slice returns `0x00000000`.
/// The canonical test vector `b"123456789"` returns `0xE3069283`.
#[inline]
pub fn crc32c(data: &[u8]) -> u32 {
    if data.is_empty() {
        return 0;
    }
    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        let index = ((crc ^ (byte as u32)) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32C_TABLE[index];
    }
    crc ^ 0xFFFFFFFFu32
}

/// Updates an existing CRC-32C checksum with additional data.
#[inline]
pub fn crc32c_update(previous_crc: u32, data: &[u8]) -> u32 {
    if data.is_empty() {
        return previous_crc;
    }
    // Invert final XOR of previous_crc to continue accumulation
    let mut crc = if previous_crc == 0 {
        0xFFFFFFFFu32
    } else {
        previous_crc ^ 0xFFFFFFFFu32
    };
    for &byte in data {
        let index = ((crc ^ (byte as u32)) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32C_TABLE[index];
    }
    crc ^ 0xFFFFFFFFu32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_vector_123456789() {
        let input = b"123456789";
        let expected = 0xE3069283;
        assert_eq!(
            crc32c(input),
            expected,
            "CRC-32C for 123456789 must equal 0xE3069283"
        );
    }

    #[test]
    fn test_empty_slice() {
        assert_eq!(crc32c(b""), 0x00000000);
    }

    #[test]
    fn test_incremental_crc_matches_all_at_once() {
        let data = b"The quick brown fox jumps over the lazy dog";
        let full = crc32c(data);

        // Split into chunks
        let mut running = 0;
        for chunk in data.chunks(4) {
            running = crc32c_update(running, chunk);
        }
        assert_eq!(running, full);
    }
}

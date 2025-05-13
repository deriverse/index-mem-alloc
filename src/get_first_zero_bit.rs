use crate::MemoryMapError;

pub(crate) fn get_first_zero_bit(pattern: u64, bits: usize) -> Result<usize, MemoryMapError> {
    if bits < 33 {
        for j in 0..bits {
            if pattern & (1 << j) == 0 {
                return Ok(j);
            }
        }
    } else if pattern & 0xffffffff == 0xffffffff {
        if pattern & 0xffff00000000 == 0xffff00000000 {
            for j in 48..bits {
                if pattern & (1 << j) == 0 {
                    return Ok(j);
                }
            }
        } else {
            for j in 32..bits.min(48) {
                if pattern & (1 << j) == 0 {
                    return Ok(j);
                }
            }
        }
    } else if pattern & 0xffff == 0xffff {
        for j in 16..32 {
            if pattern & (1 << j) == 0 {
                return Ok(j);
            }
        }
    } else {
        for j in 0..16 {
            if pattern & (1 << j) == 0 {
                return Ok(j);
            }
        }
    }

    Err(MemoryMapError::NoAvailableSlots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_first_zero_bit_empty_pattern() {
        // Test with all bits zero (should return the first bit)
        let result = get_first_zero_bit(0, 64);
        assert_eq!(
            result.unwrap(),
            0,
            "First zero bit in empty pattern should be 0"
        );
    }

    #[test]
    fn test_get_first_zero_bit_specific_patterns() {
        // Test with specific patterns to check all branches

        // Pattern with first few bits set
        let pattern = 0b1111; // First 4 bits set
        let result = get_first_zero_bit(pattern, 64);
        assert_eq!(result.unwrap(), 4, "First zero bit should be at position 4");

        // Pattern with all bits in first 16 bits set
        let pattern = 0xFFFF; // First 16 bits set
        let result = get_first_zero_bit(pattern, 64);
        assert_eq!(
            result.unwrap(),
            16,
            "First zero bit should be at position 16"
        );

        // Pattern with all bits in first 32 bits set
        let pattern = 0xFFFFFFFF; // First 32 bits set
        let result = get_first_zero_bit(pattern, 64);
        assert_eq!(
            result.unwrap(),
            32,
            "First zero bit should be at position 32"
        );

        // Pattern with bits set up to position 48
        let pattern = 0xFFFFFFFFFFFF; // First 48 bits set
        let result = get_first_zero_bit(pattern, 64);
        assert_eq!(
            result.unwrap(),
            48,
            "First zero bit should be at position 48"
        );
    }

    #[test]
    fn test_get_first_zero_bit_limited_search_range() {
        // Test with limited search range
        let pattern = 0xFFFF; // First 16 bits set

        // Limit search to first 8 bits (should fail)
        let result = get_first_zero_bit(pattern, 8);
        assert!(
            matches!(result, Err(MemoryMapError::NoAvailableSlots)),
            "Should return error when no zero bits in range"
        );

        // Limit search to first 32 bits (should find bit 16)
        let result = get_first_zero_bit(pattern, 32);
        assert_eq!(result.unwrap(), 16, "Should find zero bit at position 16");
    }

    #[test]
    fn test_get_first_zero_bit_all_bits_set() {
        // Test with all bits set (should return error)
        let pattern = 0xFFFFFFFFFFFFFFFF; // All 64 bits set
        let result = get_first_zero_bit(pattern, 64);
        assert!(
            matches!(result, Err(MemoryMapError::NoAvailableSlots)),
            "Should return error when all bits are set"
        );
    }

    #[test]
    fn test_get_first_zero_bit_alternating_pattern() {
        // Test with alternating bit pattern
        let pattern = 0xAAAAAAAAAAAAAAAA; // 10101010... pattern
        let result = get_first_zero_bit(pattern, 64);
        assert_eq!(result.unwrap(), 0, "Should find zero bit at position 0");

        let pattern = 0x5555555555555555; // 01010101... pattern
        let result = get_first_zero_bit(pattern, 64);
        assert_eq!(result.unwrap(), 1, "Should find zero bit at position 1");
    }
}

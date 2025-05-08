use crate::MemoryMapError;

pub(crate) fn get_first_zero_bit(pattern: u64, bits: isize) -> Result<isize, MemoryMapError> {
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
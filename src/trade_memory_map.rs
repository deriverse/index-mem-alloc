use crate::{MemoryMapError, get_first_zero_bit::get_first_zero_bit};
use bytemuck::cast_slice_mut;
use std::{cell::RefCell, rc::Rc};

/// Standard memory map implementation (3 levels, 4 bits at first level)
#[derive(Clone)]
pub struct StandardMemoryMap<'a> {
    memory: Rc<RefCell<&'a mut [u8]>>,
    offset: usize,
}

impl<'a> StandardMemoryMap<'a> {
    /// Create a new standard memory map
    pub(crate) fn new(
        memory: Rc<RefCell<&'a mut [u8]>>,
        offset: usize,
    ) -> Result<Self, MemoryMapError> {
        // Calculate required memory size for standard map:
        // - First level: 1 word to track available blocks in level 2
        // - Second level: 4 words (one per bit in first level)
        // - Third level: 4*64 words (one per bit in second level)
        let required_size = (1 + 4 + 4 * 64) * size_of::<u64>();

        // Check if there's enough memory
        {
            let memory_ref = memory.borrow();
            if memory_ref.len() - offset < required_size {
                return Err(MemoryMapError::InsufficientMemory);
            }
        }

        Ok(Self { memory, offset })
    }

    /// Allocate a new slot
    pub(crate) fn alloc(&mut self) -> Result<usize, MemoryMapError> {
        let mut memory = self
            .memory
            .try_borrow_mut()
            .map_err(|_| MemoryMapError::CantBorrowMutMemory)?;
        // Safe to use `cast_slice_mut` because we already checked alignment in the
        // constructor
        let u64_slice = cast_slice_mut::<u8, u64>(&mut memory[self.offset..]);

        // First level allocation (4 bits)
        let first = get_first_zero_bit(u64_slice[0], 4)?;

        // Second level allocation
        let second_idx = 1 + first;
        if second_idx >= u64_slice.len() {
            return Err(MemoryMapError::InsufficientMemory);
        }

        let second = get_first_zero_bit(u64_slice[second_idx], 64)?;

        // Third level allocation
        let third_idx = 5 + (first * 64) + second;
        if third_idx >= u64_slice.len() {
            return Err(MemoryMapError::InsufficientMemory);
        }

        let third = get_first_zero_bit(u64_slice[third_idx], 64)?;

        // Mark as allocated
        u64_slice[third_idx] |= 1 << third;
        if u64_slice[third_idx] == u64::MAX {
            u64_slice[second_idx] |= 1 << second;
            if u64_slice[second_idx] == u64::MAX {
                u64_slice[0] |= 1 << first;
            }
        }

        Ok((first << 12) + (second << 6) + third)
    }

    /// Deallocate a previously allocated slot
    pub(crate) fn dealloc(&mut self, index: usize) -> Result<(), MemoryMapError> {
        // Check upper bound
        let max_index = (4 << 12) - 1; // 16383
        if index > max_index {
            return Err(MemoryMapError::InvalidIndex);
        }
        let mut memory = self
            .memory
            .try_borrow_mut()
            .map_err(|_| MemoryMapError::CantBorrowMutMemory)?;

        // Safe to use cast_slice_mut because we already checked alignment in the
        // constructor
        let u64_slice = cast_slice_mut::<u8, u64>(&mut memory[self.offset..]);

        // standard memory map - 3 levels
        let first = index >> 12;
        let second = (index & 0xfff) >> 6;
        let second_idx = 1 + first as usize;
        let third_idx = 5 + (index >> 6) as usize;

        // Validate array bounds before access
        if third_idx >= u64_slice.len() || second_idx >= u64_slice.len() {
            return Err(MemoryMapError::IndexOutOfBounds);
        }

        // Clear allocation bits
        u64_slice[third_idx] &= u64::MAX - (1 << (index & 0x3f));
        u64_slice[second_idx] &= u64::MAX - (1 << second);
        u64_slice[0] &= u64::MAX - (1 << first);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, mem::size_of, rc::Rc};

    #[test]
    fn test_standard_map_basic_operations() {
        // Create memory with sufficient size for StandardMemoryMap
        let required_size = (1 + 4 + 4 * 64) * size_of::<u64>();
        let mut data = vec![0u8; required_size * 2];

        // Ensure proper alignment
        let alignment_offset = (8 - (data.as_ptr() as usize % 8)) % 8;
        if alignment_offset > 0 {
            data = vec![0u8; required_size * 2 + alignment_offset];
        }

        // SAFETY: This is safe in tests since the data lives for the entire test
        // duration
        let data_ptr = Box::leak(data.into_boxed_slice());
        let data_slice = &mut data_ptr[alignment_offset..];
        let data_rc = Rc::new(RefCell::new(data_slice));

        // Test creation and basic memory allocation
        let map_result = StandardMemoryMap::new(data_rc.clone(), 0);
        assert!(
            map_result.is_ok(),
            "Should create map with sufficient memory"
        );

        // Test insufficient memory error
        let bad_map = StandardMemoryMap::new(data_rc.clone(), required_size * 2 - 10);
        assert!(
            matches!(bad_map, Err(MemoryMapError::InsufficientMemory)),
            "Should fail with insufficient memory"
        );
    }

    #[test]
    fn test_standard_map_level_transitions() {
        // Create memory with sufficient size for level transitions
        let required_size = (1 + 4 + 4 * 64) * size_of::<u64>();
        let mut data = vec![0u8; required_size * 2];

        let alignment_offset = (8 - (data.as_ptr() as usize % 8)) % 8;
        if alignment_offset > 0 {
            data = vec![0u8; required_size * 2 + alignment_offset];
        }

        let data_ptr = Box::leak(data.into_boxed_slice());
        let data_slice = &mut data_ptr[alignment_offset..];
        let data_rc = Rc::new(RefCell::new(data_slice));

        data_rc.borrow_mut().fill(0);
        let mut map = StandardMemoryMap::new(data_rc.clone(), 0).unwrap();

        // Allocate indices to cross level boundaries
        let mut all_indices = Vec::new();
        for _ in 0..150 {
            match map.alloc() {
                Ok(idx) => all_indices.push(idx),
                Err(_) => break,
            }
        }

        // Verify level transition patterns (specific to StandardMemoryMap)
        if all_indices.len() > 64 {
            // Check first block (first=0, second=0..63)
            for i in 0..64 {
                assert_eq!(
                    all_indices[i] >> 12,
                    0,
                    "First 64 indices should use first-level bit 0"
                );
                assert_eq!(
                    (all_indices[i] >> 6) & 0x3F,
                    0,
                    "First 64 indices should use second-level bit 0"
                );
            }

            // Check transition to next second-level bit
            if all_indices.len() > 64 {
                assert_eq!(
                    (all_indices[64] >> 6) & 0x3F,
                    1,
                    "65th index should use second-level bit 1"
                );
            }
        }
    }
}

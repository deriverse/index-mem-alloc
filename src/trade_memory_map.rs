use crate::{get_first_zero_bit::get_first_zero_bit, get_u64, get_u64_mut, MemoryMapError};
use std::{mem::size_of, ptr::NonNull};

/// Standard memory map implementation (3 levels, 4 bits at first level)
#[derive(Clone)]
pub struct StandardMemoryMap {
    memory: NonNull<u8>,
    size: usize,
}

impl StandardMemoryMap {
    /// Create a new standard memory map
    pub(crate) fn new(memory: NonNull<u8>, size: usize) -> Result<Self, MemoryMapError> {
        // Calculate required memory size for standard map:
        // - First level: 1 word to track available blocks in level 2
        // - Second level: 4 words (one per bit in first level)
        // - Third level: 4*64 words (one per bit in second level)
        let required_size = (1 + 4 + 4 * 64) * size_of::<u64>();

        // Check if there's enough memory
        if size < required_size {
            return Err(MemoryMapError::InsufficientMemory);
        }

        Ok(Self { memory, size })
    }

    /// Allocate a new slot
    pub(crate) fn alloc(&mut self) -> Result<usize, MemoryMapError> {
        // First level allocation (4 bits)
        let first_word = get_u64(self.memory, self.size, 0)?;
        let first = get_first_zero_bit(*first_word, 4)?;

        // Second level allocation
        let second_idx = 1 + first;
        let second_word = get_u64(self.memory, self.size, second_idx)?;
        let second = get_first_zero_bit(*second_word, 64)?;

        // Third level allocation
        let third_idx = 5 + (first * 64) + second;
        let third_word = get_u64(self.memory, self.size, third_idx)?;
        let third = get_first_zero_bit(*third_word, 64)?;

        // Mark as allocated
        let third_word_mut = get_u64_mut(self.memory, self.size, third_idx)?;
        *third_word_mut |= 1 << third;

        if *third_word_mut == u64::MAX {
            let second_word_mut = get_u64_mut(self.memory, self.size, second_idx)?;
            *second_word_mut |= 1 << second;

            if *second_word_mut == u64::MAX {
                let first_word_mut = get_u64_mut(self.memory, self.size, 0)?;
                *first_word_mut |= 1 << first;
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

        // standard memory map - 3 levels
        let first = index >> 12;
        let second = (index & 0xfff) >> 6;
        let second_idx = 1 + first;
        let third_idx = 5 + (index >> 6);

        // Clear allocation bits
        let third_word = get_u64_mut(self.memory, self.size, third_idx)?;
        *third_word &= !(1 << (index & 0x3f));

        let second_word = get_u64_mut(self.memory, self.size, second_idx)?;
        *second_word &= !(1 << second);

        let first_word = get_u64_mut(self.memory, self.size, 0)?;
        *first_word &= !(1 << first);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_aligned_memory;

    #[test]
    fn test_standard_map_basic_operations() {
        // Create memory with sufficient size for StandardMemoryMap
        let required_size = (1 + 4 + 4 * 64) * size_of::<u64>();
        let (data, ptr) = create_aligned_memory(required_size * 2);

        // Test creation and basic memory allocation
        let map_result = StandardMemoryMap::new(ptr, data.len());
        assert!(
            map_result.is_ok(),
            "Should create map with sufficient memory"
        );

        // Test insufficient memory error
        let (_small_data, small_ptr) = create_aligned_memory(10);
        let bad_map = StandardMemoryMap::new(small_ptr, 10);
        assert!(
            matches!(bad_map, Err(MemoryMapError::InsufficientMemory)),
            "Should fail with insufficient memory"
        );
    }

    #[test]
    fn test_standard_map_level_transitions() {
        // Create memory with sufficient size for level transitions
        let required_size = (1 + 4 + 4 * 64) * size_of::<u64>();
        let (mut data, ptr) = create_aligned_memory(required_size * 2);

        data.fill(0);
        let mut map = StandardMemoryMap::new(ptr, data.len()).unwrap();

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

    #[test]
    fn test_standard_map_allocation_and_deallocation() {
        let required_size = (1 + 4 + 4 * 64) * size_of::<u64>();
        let (mut data, ptr) = create_aligned_memory(required_size);

        data.fill(0);
        let mut map = StandardMemoryMap::new(ptr, data.len()).unwrap();

        // Allocate several indices
        let idx1 = map.alloc().unwrap();
        let idx2 = map.alloc().unwrap();
        let idx3 = map.alloc().unwrap();

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(idx3, 2);

        // Deallocate middle index
        map.dealloc(idx2).unwrap();

        // Should reuse the deallocated index
        let idx4 = map.alloc().unwrap();
        assert_eq!(idx4, idx2);

        // Test invalid index deallocation
        let result = map.dealloc(20000);
        assert!(matches!(result, Err(MemoryMapError::InvalidIndex)));
    }

    #[test]
    fn test_standard_map_capacity() {
        let required_size = (1 + 4 + 4 * 64) * size_of::<u64>();
        let (mut data, ptr) = create_aligned_memory(required_size);

        data.fill(0);
        let mut map = StandardMemoryMap::new(ptr, data.len()).unwrap();

        // StandardMemoryMap with 4 bits at first level can allocate up to 4 * 64 * 64 =
        // 16384 indices Let's allocate a reasonable subset to test
        let mut allocated = Vec::new();
        for _ in 0..1000 {
            match map.alloc() {
                Ok(idx) => allocated.push(idx),
                Err(_) => break,
            }
        }

        assert!(
            allocated.len() >= 1000,
            "Should be able to allocate at least 1000 indices"
        );

        // Deallocate all
        for idx in allocated {
            map.dealloc(idx).unwrap();
        }

        // Should be able to allocate again
        let idx = map.alloc().unwrap();
        assert_eq!(idx, 0, "After deallocating all, should start from 0");
    }
}

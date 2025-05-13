use crate::{get_first_zero_bit::get_first_zero_bit, get_u64, get_u64_mut, MemoryMapError};
use std::{mem::size_of, ptr::NonNull};

/// Small memory map implementation (2 levels)
#[derive(Clone)]
pub struct SmallMemoryMap {
    memory: NonNull<u8>,
    size: usize,
}

impl SmallMemoryMap {
    /// Create a new small memory map
    pub fn new(memory: NonNull<u8>, size: usize) -> Result<Self, MemoryMapError> {
        // Calculate required memory size for small map:
        // - First level: 1 word to track available blocks in level 2
        // - Second level: 64 words (one per bit in first level)
        let required_size = (1 + 64) * size_of::<u64>();

        // Check if there's enough memory
        if size < required_size {
            return Err(MemoryMapError::InsufficientMemory);
        }

        Ok(Self { memory, size })
    }

    /// Allocate a new slot
    pub fn alloc(&mut self) -> Result<usize, MemoryMapError> {
        // First level allocation
        let first_word = get_u64(self.memory, self.size, 0)?;
        let first = get_first_zero_bit(*first_word, 64)?;

        // Second level allocation
        let second_idx = 1 + first;
        let second_word = get_u64(self.memory, self.size, second_idx)?;
        let second = get_first_zero_bit(*second_word, 64)?;

        // Mark as allocated
        let second_word_mut = get_u64_mut(self.memory, self.size, second_idx)?;
        *second_word_mut |= 1 << second;

        if *second_word_mut == u64::MAX {
            let first_word_mut = get_u64_mut(self.memory, self.size, 0)?;
            *first_word_mut |= 1 << first;
        }

        Ok((first << 6) + second)
    }

    /// Deallocate a previously allocated slot
    pub fn dealloc(&mut self, index: usize) -> Result<(), MemoryMapError> {
        // Check upper bound
        let max_index = (64 << 6) - 1; // 4095
        if index > max_index {
            return Err(MemoryMapError::InvalidIndex);
        }

        // Small memory map - 2 levels
        let first = index >> 6;
        let second_idx = 1 + first;

        // Clear allocation bits
        let second_word = get_u64_mut(self.memory, self.size, second_idx)?;
        *second_word &= !(1 << (index & 0x3f));

        let first_word = get_u64_mut(self.memory, self.size, 0)?;
        *first_word &= !(1 << first);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_aligned_memory;
    use std::mem::size_of;

    #[test]
    fn test_small_map_basic_operations() {
        let required_size = (1 + 64) * size_of::<u64>();
        let (mut data, ptr) = create_aligned_memory(required_size * 2);

        // 1. Test creation
        let map_result = SmallMemoryMap::new(ptr, data.len());
        assert!(
            map_result.is_ok(),
            "Should create map with sufficient memory"
        );

        // 2. Test insufficient memory at creation
        let bad_map_result = SmallMemoryMap::new(ptr, 10); // Too small
        assert!(
            matches!(bad_map_result, Err(MemoryMapError::InsufficientMemory)),
            "Should fail with insufficient memory"
        );

        // 3. Test basic allocation and deallocation
        data.fill(0); // Clear the memory
        let mut map = SmallMemoryMap::new(ptr, data.len()).unwrap();

        // Allocate a few indices
        let index1 = map.alloc().unwrap();
        let index2 = map.alloc().unwrap();
        let index3 = map.alloc().unwrap();

        assert_eq!(index1, 0, "First allocation should be 0");
        assert_eq!(index2, 1, "Second allocation should be 1");
        assert_eq!(index3, 2, "Third allocation should be 2");

        // Deallocate and verify reuse
        map.dealloc(index2).unwrap();
        let index4 = map.alloc().unwrap();
        assert_eq!(index4, index2, "Should reuse deallocated index");

        // 4. Test invalid deallocation
        let invalid_index = 5000; // Beyond capacity
        let dealloc_result = map.dealloc(invalid_index);
        assert!(
            matches!(dealloc_result, Err(MemoryMapError::InvalidIndex)),
            "Should reject invalid index"
        );
    }

    #[test]
    fn test_small_map_level_transition() {
        let required_size = (1 + 64) * size_of::<u64>();
        let (mut data, ptr) = create_aligned_memory(required_size * 2);

        data.fill(0); // Clear the memory
        let mut map = SmallMemoryMap::new(ptr, data.len()).unwrap();

        // Allocate and track indices
        let mut all_indices = Vec::new();

        // Allocate at least 65 indices to cross level boundary
        for _ in 0..70 {
            match map.alloc() {
                Ok(idx) => all_indices.push(idx),
                Err(_) => break,
            }
        }

        // Verify patterns
        if all_indices.len() > 64 {
            // Check first level bits
            for i in 0..64 {
                assert_eq!(
                    all_indices[i] >> 6,
                    0,
                    "First 64 indices should use first-level bit 0"
                );
            }

            // Check transition to second bit in first level
            assert_eq!(
                all_indices[64] >> 6,
                1,
                "65th index should use first-level bit 1"
            );

            // Second level bits
            assert_eq!(
                all_indices[64] & 0x3F,
                0,
                "65th index should use second-level bit 0"
            );
        }
    }

    #[test]
    fn test_deallocation_and_reuse() {
        let required_size = (1 + 64) * size_of::<u64>();
        let (mut data, ptr) = create_aligned_memory(required_size);

        data.fill(0);
        let mut map = SmallMemoryMap::new(ptr, data.len()).unwrap();

        // Allocate some indices
        let idx1 = map.alloc().unwrap();
        let idx2 = map.alloc().unwrap();
        let idx3 = map.alloc().unwrap();

        // Deallocate middle one
        map.dealloc(idx2).unwrap();

        // Should reuse the deallocated index
        let idx4 = map.alloc().unwrap();
        assert_eq!(idx4, idx2, "Should reuse deallocated index");

        // Deallocate all
        map.dealloc(idx1).unwrap();
        map.dealloc(idx3).unwrap();
        map.dealloc(idx4).unwrap();

        // Should start from the beginning again
        let idx5 = map.alloc().unwrap();
        assert_eq!(
            idx5, 0,
            "Should start from beginning after deallocating all"
        );
    }
}

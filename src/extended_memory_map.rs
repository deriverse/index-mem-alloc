use crate::{get_first_zero_bit::get_first_zero_bit, get_u64, get_u64_mut, MemoryMapError};
use std::{mem::size_of, ptr::NonNull};

const FIRST_LEVEL_BITS: usize = 8;
const SECOND_LEVEL_BITS: usize = 64;
const THIRD_LEVEL_BITS: usize = 64;
const MAX_INDEX: usize = (FIRST_LEVEL_BITS * SECOND_LEVEL_BITS * THIRD_LEVEL_BITS) - 1; // 32767

/// Extended memory map implementation (3 levels, 8 bits at first level)
#[derive(Clone)]
pub struct ExtendedMemoryMap {
    pub(crate) memory: NonNull<u8>,
}

impl std::fmt::Debug for ExtendedMemoryMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Extended Memory Map")
    }
}

impl ExtendedMemoryMap {
    /// Calculate required memory size for Extended map:
    /// - First level: 1 word to track available blocks in level 2
    /// - Second level: FIRST_LEVEL_BITS words (one per bit in first level)
    /// - Third level: FIRST_LEVEL_BITS*SECOND_LEVEL_BITS words (one per bit in
    ///   second level)
    pub(crate) const SIZE: usize =
        (1 + FIRST_LEVEL_BITS + FIRST_LEVEL_BITS * SECOND_LEVEL_BITS) * size_of::<u64>();

    // 0 0 0 0 ||| 0 * 8 | 0 * 8 | 0 * 8 | 0 * 8 |||

    /// Allocate a new slot
    pub(crate) fn alloc(&mut self) -> Result<usize, MemoryMapError> {
        // First level allocation (FIRST_LEVEL_BITS bits)
        let first_word = get_u64(self.memory, Self::SIZE, 0)?;
        let first = get_first_zero_bit(*first_word, FIRST_LEVEL_BITS)?;

        // Second level allocation
        let second_idx = 1 + first;
        let second_word = get_u64(self.memory, Self::SIZE, second_idx)?;
        let second = get_first_zero_bit(*second_word, SECOND_LEVEL_BITS)?;

        // Third level allocation
        let third_idx = FIRST_LEVEL_BITS + 1 + (first * SECOND_LEVEL_BITS) + second;
        let third_word = get_u64(self.memory, Self::SIZE, third_idx)?;
        let third = get_first_zero_bit(*third_word, THIRD_LEVEL_BITS)?;

        // Mark as allocated
        let third_word_mut = get_u64_mut(self.memory, Self::SIZE, third_idx)?;
        *third_word_mut |= 1 << third;

        if *third_word_mut == u64::MAX {
            let second_word_mut = get_u64_mut(self.memory, Self::SIZE, second_idx)?;
            *second_word_mut |= 1 << second;

            if *second_word_mut == u64::MAX {
                let first_word_mut = get_u64_mut(self.memory, Self::SIZE, 0)?;
                *first_word_mut |= 1 << first;
            }
        }

        Ok((first << 12) + (second << 6) + third)
    }

    /// Deallocate a previously allocated slot
    pub(crate) fn dealloc(&mut self, index: usize) -> Result<(), MemoryMapError> {
        if index > MAX_INDEX {
            return Err(MemoryMapError::InvalidIndex);
        }

        // Extended memory map - 3 levels
        let first = index >> 12;
        let second = (index & 0xfff) >> 6;
        let second_idx = 1 + first;
        let third_idx = FIRST_LEVEL_BITS + 1 + (index >> 6);

        // Clear allocation bits
        let third_word = get_u64_mut(self.memory, Self::SIZE, third_idx)?;
        *third_word &= !(1 << (index & 0x3f));

        let second_word = get_u64_mut(self.memory, Self::SIZE, second_idx)?;
        *second_word &= !(1 << second);

        let first_word = get_u64_mut(self.memory, Self::SIZE, 0)?;
        *first_word &= !(1 << first);

        Ok(())
    }

    /// Mark a specific index as allocated
    pub(crate) fn alloc_at(&mut self, index: usize) -> Result<(), MemoryMapError> {
        if index > MAX_INDEX {
            return Err(MemoryMapError::InvalidIndex);
        }
        let first_idx = index >> 12;
        let second_idx = (index & 0xfff) >> 6;
        let bit_in_third = index & 0x3f;

        let second_word_idx = 1 + first_idx;
        let third_word_idx = FIRST_LEVEL_BITS + 1 + (index >> 6);
        let third_value = 1u64 << bit_in_third;

        let third_word_mut = get_u64_mut(self.memory, Self::SIZE, third_word_idx)?;
        if *third_word_mut & third_value != 0 {
            return Err(MemoryMapError::DoubleAllocation(index));
        }

        // Mark as allocated in third level
        *third_word_mut |= third_value;

        // Update second level if needed
        if *third_word_mut == u64::MAX {
            let second_value = 1u64 << second_idx;
            let second_word_mut = get_u64_mut(self.memory, Self::SIZE, second_word_idx)?;
            *second_word_mut |= second_value;

            // Update first level if needed
            if *second_word_mut == u64::MAX {
                let first_value = 1u64 << first_idx;
                let first_word_mut = get_u64_mut(self.memory, Self::SIZE, 0)?;
                *first_word_mut |= first_value;
            }
        }

        Ok(())
    }

    /// Check if a specific index is allocated
    pub(crate) fn is_allocated(&self, index: usize) -> Result<bool, MemoryMapError> {
        if index > MAX_INDEX {
            return Err(MemoryMapError::InvalidIndex);
        }

        // Calculate the third level word index (same as in dealloc)
        let third_idx = FIRST_LEVEL_BITS + 1 + (index >> 6);
        let third_bit = index & 0x3f;

        // Get the third level word and check if the bit is set
        let third_word = get_u64(self.memory, Self::SIZE, third_idx)?;
        let is_allocated = (third_word & (1 << third_bit)) != 0;

        Ok(is_allocated)
    }

    /// Reset all allocations, clearing the entire memory map
    pub(crate) fn reset(&mut self) -> Result<(), MemoryMapError> {
        // Clear first level (1 word)
        let first_word = get_u64_mut(self.memory, Self::SIZE, 0)?;
        *first_word = 0;

        // Clear second level (FIRST_LEVEL_BITS words)
        for i in 1..=FIRST_LEVEL_BITS {
            let second_word = get_u64_mut(self.memory, Self::SIZE, i)?;
            *second_word = 0;
        }

        // Clear third level (FIRST_LEVEL_BITS * SECOND_LEVEL_BITS words)
        let third_level_start = 1 + FIRST_LEVEL_BITS;
        let third_level_count = FIRST_LEVEL_BITS * SECOND_LEVEL_BITS;
        for i in 0..third_level_count {
            let third_word = get_u64_mut(self.memory, Self::SIZE, third_level_start + i)?;
            *third_word = 0;
        }

        Ok(())
    }
}

impl PartialEq for ExtendedMemoryMap {
    fn eq(&self, other: &Self) -> bool {
        const WORDS_TO_COMPARE: usize = ExtendedMemoryMap::SIZE / size_of::<u64>();

        (0..WORDS_TO_COMPARE).all(|index| {
            match (
                get_u64(self.memory, Self::SIZE, index),
                get_u64(other.memory, Self::SIZE, index),
            ) {
                (Ok(a), Ok(b)) => a == b,
                _ => false,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_aligned_memory, MapType, MemoryMap};

    fn get_required_size() -> usize {
        ExtendedMemoryMap::SIZE
    }

    #[test]
    fn allocation_by_index() {
        let required_size = get_required_size();
        let mut data = create_aligned_memory(required_size);

        let mut map = MemoryMap::new_from_slice(&mut data, 0, MapType::Extended).unwrap();

        map.alloc_at(1552).unwrap();
        let double_alloc = map.alloc_at(1552);

        // Try allocate on the same address
        assert!(matches!(
            double_alloc,
            Err(MemoryMapError::DoubleAllocation(1552))
        ));
        assert_eq!(map.is_allocated(1552).unwrap(), true);

        map.alloc().unwrap();
        let double_alloc = map.alloc_at(0);

        assert!(matches!(
            double_alloc,
            Err(MemoryMapError::DoubleAllocation(0))
        ));
    }

    #[test]
    fn test_extended_map_level_transitions() {
        // Create memory with sufficient size for level transitions
        let required_size = get_required_size();
        let mut data = create_aligned_memory(required_size);

        data.fill(0);
        let mut map = MemoryMap::new_from_slice(&mut data, 0, MapType::Extended).unwrap();

        // Allocate indices to cross level boundaries
        let mut all_indices = Vec::new();
        for _ in 0..150 {
            match map.alloc() {
                Ok(idx) => {
                    all_indices.push(idx);
                    assert!(
                        map.is_allocated(idx).unwrap(),
                        "Index {} should be allocated",
                        idx
                    );
                }
                Err(_) => break,
            }
        }

        // Verify level transition patterns (specific to ExtendedMemoryMap)
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
    fn test_extended_map_allocation_and_deallocation() {
        let required_size = get_required_size();
        let mut data = create_aligned_memory(required_size);

        data.fill(0);
        let mut map = MemoryMap::new_from_slice(&mut data, 0, MapType::Extended).unwrap();

        // Check initial state
        assert!(!map.is_allocated(0).unwrap());
        assert!(!map.is_allocated(1).unwrap());
        assert!(!map.is_allocated(2).unwrap());

        // Allocate several indices
        let idx1 = map.alloc().unwrap();
        let idx2 = map.alloc().unwrap();
        let idx3 = map.alloc().unwrap();

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(idx3, 2);

        // Check allocation state
        assert!(map.is_allocated(idx1).unwrap());
        assert!(map.is_allocated(idx2).unwrap());
        assert!(map.is_allocated(idx3).unwrap());
        assert!(!map.is_allocated(3).unwrap());

        // Deallocate middle index
        map.dealloc(idx2).unwrap();
        assert!(!map.is_allocated(idx2).unwrap());
        assert!(map.is_allocated(idx1).unwrap());
        assert!(map.is_allocated(idx3).unwrap());

        // Should reuse the deallocated index
        let idx4 = map.alloc().unwrap();
        assert_eq!(idx4, idx2);
        assert!(map.is_allocated(idx4).unwrap());

        // Test invalid index deallocation
        let result = map.dealloc(FIRST_LEVEL_BITS * SECOND_LEVEL_BITS * THIRD_LEVEL_BITS + 1);
        assert!(matches!(result, Err(MemoryMapError::InvalidIndex)));
    }

    #[test]
    fn test_extended_map_capacity() {
        let required_size = get_required_size();
        let mut data = create_aligned_memory(required_size);

        data.fill(0);
        let mut map = MemoryMap::new_from_slice(&mut data, 0, MapType::Extended).unwrap();

        // ExtendedMemoryMap with 8 bits at first level can allocate up to 8 * 64 * 64 =
        // 32767 indices Let's allocate a reasonable subset to test
        let mut allocated = Vec::new();
        for _ in 0..1000 {
            match map.alloc() {
                Ok(idx) => {
                    allocated.push(idx);
                    assert!(
                        map.is_allocated(idx).unwrap(),
                        "Index {} should be allocated",
                        idx
                    );
                }
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
            assert!(
                !map.is_allocated(idx).unwrap(),
                "Index {} should be deallocated",
                idx
            );
        }

        // Should be able to allocate again
        let idx = map.alloc().unwrap();
        assert_eq!(idx, 0, "After deallocating all, should start from 0");
        assert!(map.is_allocated(idx).unwrap());
    }

    #[test]
    fn test_is_allocated_level_boundaries() {
        let required_size = get_required_size();
        let mut data = create_aligned_memory(required_size);
        data.fill(0);

        let mut map = MemoryMap::new_from_slice(&mut data, 0, MapType::Extended).unwrap();

        // Test specific boundary indices for Extended map (3 levels: 8, 64, 64)
        let test_indices = [
            0,    // first=0, second=0, third=0
            63,   // first=0, second=0, third=63 (last in first third-level block)
            64,   // first=0, second=1, third=0 (first in second third-level block)
            65,   // first=0, second=1, third=1
            4095, // first=0, second=63, third=63 (last in first second-level block)
            8192, // first=1, second=0, third=0 (first in second second-level block)
            8193, // first=1, second=0, third=1
        ];

        // Initially all should be unallocated
        for &idx in &test_indices {
            assert!(
                !map.is_allocated(idx).unwrap(),
                "Index {} should not be allocated initially",
                idx
            );
        }

        // Allocate specific indices and verify
        for &idx in &test_indices {
            // Force allocation by allocating sequential indices up to this point
            while map.alloc().unwrap() < idx {
                // Continue allocating until we reach the target index
            }
            // Now idx should be allocated
            assert!(
                map.is_allocated(idx).unwrap(),
                "Index {} should be allocated",
                idx
            );
        }

        // Test invalid index
        let invalid_result = map.is_allocated(MAX_INDEX + 1);
        assert!(
            matches!(invalid_result, Err(MemoryMapError::InvalidIndex)),
            "Should return InvalidIndex for index beyond MAX_INDEX"
        );

        // Test maximum valid index
        assert!(
            !map.is_allocated(MAX_INDEX).unwrap(),
            "MAX_INDEX should not be allocated yet"
        );

        // Reset
        map.reset().unwrap();

        // Verify all are unallocated
        for &idx in &test_indices {
            assert!(
                !map.is_allocated(idx).unwrap(),
                "Index {} should be unallocated after reset",
                idx
            );
        }
    }
}

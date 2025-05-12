use crate::{get_first_zero_bit::get_first_zero_bit, get_u64, get_u64_mut, MemoryMapError};
use std::{mem::size_of, ptr::NonNull};
/// Max memory map implementation (3 levels, 64 bits at first level)
#[derive(Clone)]
pub struct MaxMemoryMap {
    memory: NonNull<u8>,
    size: usize,
}

impl MaxMemoryMap {
    /// Create a new Max memory map
    pub(crate) fn new(memory: NonNull<u8>, size: usize) -> Result<Self, MemoryMapError> {
        // Calculate required memory size for max map:
        // - First level: 1 word to track available blocks in level 2
        // - Second level: 64 words (one per bit in first level)
        // - Third level: 64*64 words (one per bit in second level)
        let required_size = (1 + 64 + 64 * 64) * size_of::<u64>();

        // Check if there's enough memory
        if size < required_size {
            return Err(MemoryMapError::InsufficientMemory);
        }

        Ok(Self { memory, size })
    }

    /// Allocate a new slot
    pub(crate) fn alloc(&mut self) -> Result<usize, MemoryMapError> {
        // First level allocation (64 bits)
        let first_word = get_u64(self.memory, self.size, 0)?;
        let first = get_first_zero_bit(*first_word, 64)?;

        // Second level allocation
        let second_idx = 1 + first;
        let second_word = get_u64(self.memory, self.size, second_idx)?;
        let second = get_first_zero_bit(*second_word, 64)?;

        // Third level allocation
        let third_idx = 65 + (first * 64) + second;
        let third_word = get_u64(self.memory, self.size, third_idx)?;
        let third = get_first_zero_bit(*third_word, 64)?;

        // Mark as allocated in third level
        let third_word_mut = get_u64_mut(self.memory, self.size, third_idx)?;
        *third_word_mut |= 1 << third;

        // Update second level if needed
        if *third_word_mut == u64::MAX {
            let second_word_mut = get_u64_mut(self.memory, self.size, second_idx)?;
            *second_word_mut |= 1 << second;

            // Update first level if needed
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
        let max_index = (64 << 12) - 1; // 262143
        if index > max_index {
            return Err(MemoryMapError::InvalidIndex);
        }

        // max memory map - 3 levels
        let first = index >> 12;
        let second = (index & 0xfff) >> 6;
        let second_idx = 1 + first;
        let third_idx = 65 + (index >> 6);

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
pub(crate) mod tests {
    use super::*;
    use crate::create_aligned_memory;
    use std::ptr::NonNull;

    // Calculate required memory size for max map
    fn get_required_size() -> usize {
        (1 + 64 + 64 * 64) * size_of::<u64>()
    }

    #[test]
    fn test_basic_creation() {
        let required_size = get_required_size();
        let (data, ptr) = create_aligned_memory(required_size);

        // Create a MaxMemoryMap with sufficient memory
        let map_result = MaxMemoryMap::new(ptr, data.len());
        assert!(
            map_result.is_ok(),
            "Should create map with sufficient memory"
        );
    }

    #[test]
    fn test_insufficient_memory() {
        let (data, ptr) = create_aligned_memory(100); // Too small

        // Try to create with insufficient memory
        let map_result = MaxMemoryMap::new(ptr, data.len());
        assert!(
            matches!(map_result, Err(MemoryMapError::InsufficientMemory)),
            "Should fail with insufficient memory"
        );
    }

    #[test]
    fn test_basic_allocation() {
        let required_size = get_required_size();
        let (mut data, ptr) = create_aligned_memory(required_size);

        // Reset memory for this test
        data.fill(0);

        // Create map and perform first allocation
        let mut map = MaxMemoryMap::new(ptr, data.len()).unwrap();
        let index1 = map.alloc();

        assert!(index1.is_ok(), "First allocation should succeed");
        assert_eq!(index1.unwrap(), 0, "First allocation should return index 0");
    }

    #[test]
    fn test_multiple_allocations() {
        let required_size = get_required_size();
        let (mut data, ptr) = create_aligned_memory(required_size);
        data.fill(0);

        let mut map = MaxMemoryMap::new(ptr, data.len()).unwrap();

        // Allocate 5 indices and verify they are sequential
        let mut indices = Vec::new();
        for i in 0..5 {
            let index = map.alloc().unwrap();
            indices.push(index);
            assert_eq!(index, i, "Index should match iteration count");
        }

        // Verify all indices are unique
        for i in 0..indices.len() {
            for j in i + 1..indices.len() {
                assert_ne!(indices[i], indices[j], "Allocated indices should be unique");
            }
        }
    }

    #[test]
    fn test_deallocation() {
        let required_size = get_required_size();
        let (mut data, ptr) = create_aligned_memory(required_size);
        data.fill(0);

        let mut map = MaxMemoryMap::new(ptr, data.len()).unwrap();

        // Allocate 3 indices
        let index1 = map.alloc().unwrap();
        let index2 = map.alloc().unwrap();
        let index3 = map.alloc().unwrap();

        // Verify the sequence
        assert_eq!(index1, 0, "First allocation should be 0");
        assert_eq!(index2, 1, "Second allocation should be 1");
        assert_eq!(index3, 2, "Third allocation should be 2");

        // Deallocate the middle one
        let dealloc_result = map.dealloc(index2);
        assert!(dealloc_result.is_ok(), "Deallocation should succeed");

        // Next allocation should reuse the deallocated index
        let index4 = map.alloc().unwrap();
        assert_eq!(index4, index2, "Should reuse deallocated index");

        // One more allocation should be a new index
        let index5 = map.alloc().unwrap();
        assert_eq!(index5, index3 + 1, "Next allocation should be a new index");
    }

    #[test]
    fn test_invalid_deallocation() {
        let required_size = get_required_size();
        let (mut data, ptr) = create_aligned_memory(required_size);
        data.fill(0);

        let mut map = MaxMemoryMap::new(ptr, data.len()).unwrap();

        // Try to deallocate an invalid index
        let invalid_index = 1_000_000; // Way beyond our capacity
        let dealloc_result = map.dealloc(invalid_index);
        assert!(
            matches!(dealloc_result, Err(MemoryMapError::InvalidIndex)),
            "Should reject invalid index"
        );
    }

    #[test]
    fn test_allocation_after_multiple_deallocations() {
        let required_size = get_required_size();
        let (mut data, ptr) = create_aligned_memory(required_size);
        data.fill(0);

        let mut map = MaxMemoryMap::new(ptr, data.len()).unwrap();

        // Allocate 10 indices
        let mut indices = Vec::new();
        for _ in 0..10 {
            indices.push(map.alloc().unwrap());
        }

        // Deallocate some specific indices
        let to_deallocate = [2, 5, 7];
        for &idx in &to_deallocate {
            map.dealloc(indices[idx]).unwrap();
        }

        // Next allocations should reuse deallocated indices in order
        for &idx in &to_deallocate {
            let new_index = map.alloc().unwrap();
            assert_eq!(
                new_index, indices[idx],
                "Should reuse deallocated index {}",
                indices[idx]
            );
        }
    }

    #[test]
    fn test_level_transitions() {
        let required_size = get_required_size();
        let (mut data, ptr) = create_aligned_memory(required_size);
        data.fill(0);

        let mut map = MaxMemoryMap::new(ptr, data.len()).unwrap();

        // Allocate and track indices
        let mut all_indices = Vec::new();

        // Allocate 100 indices - should cross level boundaries
        for _ in 0..100 {
            let idx = map.alloc().unwrap();
            all_indices.push(idx);
        }

        // Check patterns in allocated indices
        // First 64 indices should have the form (0 << 12) + (0 << 6) + i
        // where i ranges from 0 to 63
        for i in 0..64 {
            assert_eq!(all_indices[i], i, "First 64 indices should be sequential");
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

        // Next batch of indices should have the form (0 << 12) + (1 << 6) + i
        assert_eq!(
            all_indices[64] >> 6,
            1,
            "65th index should use second-level bit 1"
        );
        assert_eq!(
            all_indices[64] & 0x3F,
            0,
            "65th index should use third-level bit 0"
        );

        // Deallocate in reverse order
        for idx in all_indices.iter().rev() {
            map.dealloc(*idx).unwrap();
        }

        // After deallocating everything, next allocation should be 0 again
        let index = map.alloc().unwrap();
        assert_eq!(
            index, 0,
            "After deallocating all, should start from 0 again"
        );
    }

    #[test]
    fn test_full_allocation() {
        let required_size = get_required_size();
        let (mut data, ptr) = create_aligned_memory(required_size);
        data.fill(0);

        let mut map = MaxMemoryMap::new(ptr, data.len()).unwrap();

        // Allocate as many indices as possible (limiting to avoid excessive test time)
        let mut allocated = Vec::new();
        let mut last_index = None;

        // Try to allocate up to 10,000 indices
        for _ in 0..10_000 {
            match map.alloc() {
                Ok(idx) => {
                    // Check that indices are increasing
                    if let Some(last) = last_index {
                        assert!(
                            idx > last || allocated.contains(&idx),
                            "New index should be greater than previous or reused"
                        );
                    }
                    last_index = Some(idx);
                    allocated.push(idx);
                }
                Err(_) => break,
            }
        }

        println!("Successfully allocated {} indices", allocated.len());
        assert!(
            allocated.len() > 100,
            "Should allocate a significant number of indices"
        );

        // Now deallocate every third index
        let to_deallocate: Vec<_> = allocated
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 3 == 0)
            .map(|(_, &idx)| idx)
            .collect();

        for &idx in &to_deallocate {
            map.dealloc(idx).unwrap();
        }

        // Reallocate and verify we get the same indices back (in some order)
        let mut reallocated = Vec::new();
        for _ in 0..to_deallocate.len() {
            match map.alloc() {
                Ok(idx) => reallocated.push(idx),
                Err(_) => break,
            }
        }

        // Sort both for comparison
        let mut to_deallocate = to_deallocate.clone();
        to_deallocate.sort();
        reallocated.sort();

        assert_eq!(
            to_deallocate, reallocated,
            "Should reallocate exactly the same indices that were deallocated"
        );
    }

    #[test]
    fn test_multiple_maps_in_same_buffer() {
        let single_map_size = get_required_size();
        let (mut data, base_ptr) = create_aligned_memory(single_map_size * 2);
        data.fill(0);

        // Create first pointer for the first map
        let ptr1 = base_ptr;

        // Create second pointer for the second map (offset by single_map_size)
        let ptr2_raw = unsafe { base_ptr.as_ptr().add(single_map_size) };
        let ptr2 = NonNull::new(ptr2_raw).expect("Offset pointer should not be null");

        // Step 1: Create first map and allocate
        let mut map1 = MaxMemoryMap::new(ptr1, single_map_size).unwrap();
        let index1 = map1.alloc().unwrap();
        assert_eq!(index1, 0, "First allocation in map1 should be 0");

        // Step 2: Create second map at different offset and allocate
        let mut map2 = MaxMemoryMap::new(ptr2, single_map_size).unwrap();
        let index2 = map2.alloc().unwrap();
        assert_eq!(index2, 0, "First allocation in map2 should be 0");

        // Step 3: Allocate again from both maps
        let index1_2 = map1.alloc().unwrap();
        assert_eq!(index1_2, 1, "Second allocation in map1 should be 1");

        let index2_2 = map2.alloc().unwrap();
        assert_eq!(index2_2, 1, "Second allocation in map2 should be 1");

        // Step 4: Deallocate from first map
        map1.dealloc(index1).unwrap();

        // Step 5: Verify independence of both maps
        let index1_3 = map1.alloc().unwrap();
        assert_eq!(index1_3, index1, "Map1 should reuse deallocated index");

        let index2_3 = map2.alloc().unwrap();
        assert_eq!(index2_3, 2, "Map2 should allocate next sequential index");
    }
}

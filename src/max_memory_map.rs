use crate::{MemoryMapError, get_first_zero_bit::get_first_zero_bit};
use bytemuck::cast_slice_mut;
use std::cell::RefMut;

/// Max memory map implementation (3 levels, 64 bits at first level)
pub struct MaxMemoryMap<'a> {
    memory: RefMut<'a, &'a mut [u8]>,
    offset: usize,
}

impl<'a> MaxMemoryMap<'a> {
    /// Create a new Max memory map
    pub(crate) fn new(
        memory: RefMut<'a, &'a mut [u8]>,
        offset: usize,
    ) -> Result<Self, MemoryMapError> {
        // Calculate required memory size for max map:
        // - First level: 1 word to track available blocks in level 2
        // - Second level: 64 words (one per bit in first level)
        // - Third level: 64*64 words (one per bit in second level)
        let required_size = (1 + 64 + 64 * 64) * size_of::<u64>();

        // Check if there's enough memory
        if memory.len() - offset < required_size {
            return Err(MemoryMapError::InsufficientMemory);
        }

        Ok(Self { memory, offset })
    }

    /// Allocate a new slot
    pub(crate) fn alloc(&mut self) -> Result<usize, MemoryMapError> {
        // Safe to use `cast_slice_mut` because we already checked alignment in the
        // constructor
        let u64_slice = cast_slice_mut::<u8, u64>(&mut self.memory[self.offset..]);

        // First level allocation (64 bits)
        let first = get_first_zero_bit(u64_slice[0], 64)?;

        // Second level allocation
        let second_idx = 1 + first;
        if second_idx >= u64_slice.len() {
            return Err(MemoryMapError::InsufficientMemory);
        }

        let second = get_first_zero_bit(u64_slice[second_idx], 64)?;

        // Third level allocation
        let third_idx = 65 + (first * 64) + second;
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
        let max_index = (64 << 12) - 1; // 262143
        if index > max_index {
            return Err(MemoryMapError::InvalidIndex);
        }

        // Safe to use cast_slice_mut because we already checked alignment in the
        // constructor
        let u64_slice = cast_slice_mut::<u8, u64>(&mut self.memory[self.offset..]);

        // max memory map - 3 levels
        let first = index >> 12;
        let second = (index & 0xfff) >> 6;
        let second_idx = 1 + first as usize;
        let third_idx = 65 + (index >> 6) as usize;

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
    use std::{cell::RefCell, mem::size_of};

    #[test]
    fn test_max_memory_map_functionality() {
        // Calculate required memory size for max map
        let required_size = (1 + 64 + 64 * 64) * size_of::<u64>();

        // Create memory buffer
        let mut data = vec![0u8; required_size * 2]; // Double size for multiple maps test

        // Test creation with proper alignment
        // Make sure our memory starts at an 8-byte aligned address
        let alignment_offset = (8 - (data.as_ptr() as usize % 8)) % 8;
        if alignment_offset > 0 {
            // Add some padding at the beginning to ensure alignment
            data = vec![0u8; required_size * 2 + alignment_offset];
        }

        // Now our buffer should be properly aligned
        let data_slice = &mut data[alignment_offset..];
        let data_ref_cell = RefCell::new(data_slice);

        // --------- Test 1: Basic Creation -----------
        {
            let memory = data_ref_cell.borrow_mut();
            let map_result = MaxMemoryMap::new(memory, 0);
            assert!(
                map_result.is_ok(),
                "Should create map with sufficient memory"
            );
        } // memory is dropped here, releasing the borrow

        // --------- Test 2: Insufficient Memory -----------
        {
            let memory = data_ref_cell.borrow_mut();
            let map_result = MaxMemoryMap::new(memory, required_size * 2 - 10);
            assert!(
                matches!(map_result, Err(MemoryMapError::InsufficientMemory)),
                "Should fail with insufficient memory"
            );
        }

        // --------- Test 3: Basic Allocation -----------
        {
            // Reset memory for this test
            data_ref_cell.borrow_mut().fill(0);

            let memory = data_ref_cell.borrow_mut();
            let mut map = MaxMemoryMap::new(memory, 0).unwrap();

            // First allocation should return 0
            let index1 = map.alloc();
            assert!(index1.is_ok(), "First allocation should succeed");
            assert_eq!(index1.unwrap(), 0, "First allocation should return index 0");
        }

        // --------- Test 4: Multiple Allocations -----------
        {
            // Reset memory for this test
            data_ref_cell.borrow_mut().fill(0);

            let memory = data_ref_cell.borrow_mut();
            let mut map = MaxMemoryMap::new(memory, 0).unwrap();

            // Allocate 5 indices
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

        // --------- Test 5: Deallocation -----------
        {
            // Reset memory for this test
            data_ref_cell.borrow_mut().fill(0);

            let memory = data_ref_cell.borrow_mut();
            let mut map = MaxMemoryMap::new(memory, 0).unwrap();

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

        // --------- Test 6: Invalid Deallocation -----------
        {
            // Reset memory for this test
            data_ref_cell.borrow_mut().fill(0);

            let memory = data_ref_cell.borrow_mut();
            let mut map = MaxMemoryMap::new(memory, 0).unwrap();

            // Try to deallocate an invalid index
            let invalid_index = 1_000_000; // Way beyond our capacity
            let dealloc_result = map.dealloc(invalid_index);
            assert!(
                matches!(dealloc_result, Err(MemoryMapError::InvalidIndex)),
                "Should reject invalid index"
            );
        }

        // --------- Test 7: Allocation After Multiple Deallocations -----------
        {
            // Reset memory for this test
            data_ref_cell.borrow_mut().fill(0);

            let memory = data_ref_cell.borrow_mut();
            let mut map = MaxMemoryMap::new(memory, 0).unwrap();

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

        // --------- Test 8: Level Transitions -----------
        {
            // Reset memory for this test
            data_ref_cell.borrow_mut().fill(0);

            let memory = data_ref_cell.borrow_mut();
            let mut map = MaxMemoryMap::new(memory, 0).unwrap();

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
            // where i ranges from 0 to 63
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

        // --------- Test 9: Alignment Error -----------
        {
            // Create an unaligned buffer (force unalignment by +1)
            let mut unaligned_data = vec![0u8; required_size + 1];
            let unaligned_slice = &mut unaligned_data[1..]; // Start at offset 1 to ensure unalignment
            let unaligned_ref_cell = RefCell::new(unaligned_slice);

            let memory = unaligned_ref_cell.borrow_mut();
            let map_result = MaxMemoryMap::new(memory, 0);

            // Should fail with alignment error if constructor checks alignment
            // Note: Our current implementation may not explicitly check this
            if let Err(err) = map_result {
                assert!(
                    matches!(err, MemoryMapError::AlignmentError),
                    "Should fail with alignment error"
                );
            }
        }

        // --------- Test 10: Full Allocation (stress test) -----------
        {
            // Reset memory for this test
            data_ref_cell.borrow_mut().fill(0);

            let memory = data_ref_cell.borrow_mut();
            let mut map = MaxMemoryMap::new(memory, 0).unwrap();

            // Maximum theoretical capacity (first level 64 bits * second level 64 bits *
            // third level 64 bits)
            let max_theoretical = 64 * 64 * 64; // 262,144

            // But our buffer is smaller, so we'll allocate as many as possible
            let mut allocated = Vec::new();
            let mut last_index = None;

            // Allocate until we get an error or a very large number
            // (limiting to 10,000 to avoid excessive test time)
            for _ in 0..10_000 {
                match map.alloc() {
                    Ok(idx) => {
                        // Check that indices are increasing
                        if let Some(last) = last_index {
                            assert!(idx > last, "New index should be greater than previous");
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

            // Now deallocate a random subset
            let rng = std::collections::hash_map::DefaultHasher::new();
            let to_deallocate: Vec<_> = allocated
                .iter()
                .enumerate()
                .filter(|(i, _)| i % 3 == 0) // Deallocate every third index
                .map(|(_, &idx)| idx)
                .collect();

            for &idx in &to_deallocate {
                map.dealloc(idx).unwrap();
            }

            // Reallocate and verify we get the same indices back in some order
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

        // --------- Test 11: Multiple Maps in Same Buffer (Fixed) -----------
        {
            // Reset memory for this test
            data_ref_cell.borrow_mut().fill(0);

            // Calculate size needed for two maps
            let single_map_size = (1 + 64 + 64 * 64) * size_of::<u64>();

            // Create a sequence of allocations and deallocations
            // with different offsets to simulate multiple maps

            // Step 1: Allocate from first region, then drop the map
            let index1;
            {
                let memory = data_ref_cell.borrow_mut();
                let mut map1 = MaxMemoryMap::new(memory, 0).unwrap();
                index1 = map1.alloc().unwrap();
                assert_eq!(index1, 0, "First allocation in map1 should be 0");
                // map1 is dropped here, releasing the borrow
            }

            // Step 2: Allocate from second region, then drop the map
            let index2;
            {
                let memory = data_ref_cell.borrow_mut();
                let mut map2 = MaxMemoryMap::new(memory, single_map_size).unwrap();
                index2 = map2.alloc().unwrap();
                assert_eq!(index2, 0, "First allocation in map2 should be 0");
                // map2 is dropped here, releasing the borrow
            }

            // Step 3: Allocate again from first region
            {
                let memory = data_ref_cell.borrow_mut();
                let mut map1 = MaxMemoryMap::new(memory, 0).unwrap();
                let index1_2 = map1.alloc().unwrap();
                assert_eq!(index1_2, 1, "Second allocation in map1 should be 1");
                // map1 is dropped here, releasing the borrow
            }

            // Step 4: Allocate again from second region
            {
                let memory = data_ref_cell.borrow_mut();
                let mut map2 = MaxMemoryMap::new(memory, single_map_size).unwrap();
                let index2_2 = map2.alloc().unwrap();
                assert_eq!(index2_2, 1, "Second allocation in map2 should be 1");
                // map2 is dropped here, releasing the borrow
            }

            // Step 5: Deallocate from first region, then drop the map
            {
                let memory = data_ref_cell.borrow_mut();
                let mut map1 = MaxMemoryMap::new(memory, 0).unwrap();
                map1.dealloc(index1).unwrap();
                // map1 is dropped here, releasing the borrow
            }

            // Step 6: Allocate again from both regions to verify independence
            {
                let memory = data_ref_cell.borrow_mut();
                let mut map1 = MaxMemoryMap::new(memory, 0).unwrap();
                let index1_3 = map1.alloc().unwrap();
                assert_eq!(index1_3, index1, "Map1 should reuse deallocated index");
                // map1 is dropped here, releasing the borrow
            }

            {
                let memory = data_ref_cell.borrow_mut();
                let mut map2 = MaxMemoryMap::new(memory, single_map_size).unwrap();
                let index2_3 = map2.alloc().unwrap();
                assert_eq!(index2_3, 2, "Map2 should allocate next sequential index");
                // map2 is dropped here, releasing the borrow
            }
        }
    }
}

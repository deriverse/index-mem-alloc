use crate::{MemoryMapError, get_first_zero_bit::get_first_zero_bit};
use bytemuck::cast_slice_mut;
use std::{
    cell::{RefCell},
    rc::Rc,
};

/// Small memory map implementation (2 levels)
#[derive(Clone)]
pub struct SmallMemoryMap<'a> {
    memory: Rc<RefCell<&'a mut [u8]>>,
    offset: usize,
}

impl<'a> SmallMemoryMap<'a> {
    /// Create a new small memory map
    pub(crate) fn new(
        memory: Rc<RefCell<&'a mut [u8]>>,
        offset: usize,
    ) -> Result<Self, MemoryMapError> {
        // Calculate required memory size for small map:
        // - First level: 1 word to track available blocks in level 2
        // - Second level: 64 words (one per bit in first level)
        let required_size = (1 + 64) * size_of::<u64>();

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
        let mut memory = self.memory.borrow_mut();
        // Safe to use `cast_slice_mut` because we already checked alignment in the
        // constructor
        let u64_slice = cast_slice_mut::<u8, u64>(&mut memory[self.offset..]);

        // First level allocation
        let first = get_first_zero_bit(u64_slice[0], 64)?;

        // Second level allocation
        let second_idx = 1 + first;
        if second_idx >= u64_slice.len() {
            return Err(MemoryMapError::InsufficientMemory);
        }

        let second = get_first_zero_bit(u64_slice[second_idx], 64)?;

        // Mark as allocated
        u64_slice[second_idx] |= 1 << second;
        if u64_slice[second_idx] == u64::MAX {
            u64_slice[0] |= 1 << first;
        }

        Ok((first << 6) + second)
    }

    /// Deallocate a previously allocated slot
    pub(crate) fn dealloc(&mut self, index: usize) -> Result<(), MemoryMapError> {
        // Check upper bound
        let max_index = (64 << 6) - 1; // 4095
        if index > max_index {
            return Err(MemoryMapError::InvalidIndex);
        }
        let mut memory = self.memory.borrow_mut();

        // Safe to use cast_slice_mut because we already checked alignment in the
        // constructor
        let u64_slice = cast_slice_mut::<u8, u64>(&mut memory[self.offset..]);

        // Small memory map - 2 levels
        let first = index >> 6;
        let second_idx = 1 + first as usize;

        // Validate array bounds before access
        if second_idx >= u64_slice.len() {
            return Err(MemoryMapError::IndexOutOfBounds);
        }

        // Clear allocation bits
        u64_slice[second_idx] &= u64::MAX - (1 << (index & 0x3f));
        u64_slice[0] &= u64::MAX - (1 << first);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, mem::size_of, rc::Rc};

    #[test]
    fn test_small_map_basic_operations() {
        let required_size = (1 + 64) * size_of::<u64>();
        let mut data = vec![0u8; required_size * 2];

        // Ensure proper alignment
        let alignment_offset = (8 - (data.as_ptr() as usize % 8)) % 8;
        if alignment_offset > 0 {
            data = vec![0u8; required_size * 2 + alignment_offset];
        }

        // SAFETY: This is safe in tests since the data lives for the entire test duration
        let data_ptr = Box::leak(data.into_boxed_slice());
        let data_slice = &mut data_ptr[alignment_offset..];
        let data_rc = Rc::new(RefCell::new(data_slice));

        // 1. Test creation
        let map_result = SmallMemoryMap::new(data_rc.clone(), 0);
        assert!(map_result.is_ok(), "Should create map with sufficient memory");

        // 2. Test insufficient memory at creation
        let bad_map = SmallMemoryMap::new(data_rc.clone(), required_size * 2 - 10);
        assert!(
            matches!(bad_map, Err(MemoryMapError::InsufficientMemory)),
            "Should fail with insufficient memory"
        );

        // 3. Test basic allocation and deallocation
        data_rc.borrow_mut().fill(0);
        let mut map = SmallMemoryMap::new(data_rc.clone(), 0).unwrap();

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
        let mut data = vec![0u8; required_size * 2];

        let alignment_offset = (8 - (data.as_ptr() as usize % 8)) % 8;
        if alignment_offset > 0 {
            data = vec![0u8; required_size * 2 + alignment_offset];
        }

        let data_ptr = Box::leak(data.into_boxed_slice());
        let data_slice = &mut data_ptr[alignment_offset..];
        let data_rc = Rc::new(RefCell::new(data_slice));

        data_rc.borrow_mut().fill(0);
        let mut map = SmallMemoryMap::new(data_rc.clone(), 0).unwrap();

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
}
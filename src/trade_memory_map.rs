use crate::{MemoryMapError, get_first_zero_bit::get_first_zero_bit};
use bytemuck::cast_slice_mut;
use std::cell::RefMut;

/// Standard memory map implementation (3 levels, 4 bits at first level)
pub struct StandardMemoryMap<'a> {
    memory: RefMut<'a, &'a mut [u8]>,
    offset: usize,
}

impl<'a> StandardMemoryMap<'a> {
    /// Create a new standard memory map
    pub(crate) fn new(
        memory: RefMut<'a, &'a mut [u8]>,
        offset: usize,
    ) -> Result<Self, MemoryMapError> {
        // Calculate required memory size for standard map:
        // - First level: 1 word to track available blocks in level 2
        // - Second level: 4 words (one per bit in first level)
        // - Third level: 4*64 words (one per bit in second level)
        let required_size = (1 + 4 + 4 * 64) * size_of::<u64>();

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

        // Safe to use cast_slice_mut because we already checked alignment in the
        // constructor
        let u64_slice = cast_slice_mut::<u8, u64>(&mut self.memory[self.offset..]);

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

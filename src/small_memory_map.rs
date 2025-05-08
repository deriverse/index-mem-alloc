use crate::{MemoryMapError, get_first_zero_bit::get_first_zero_bit};
use bytemuck::cast_slice_mut;
use std::cell::RefMut;

/// Small memory map implementation (2 levels)
pub struct SmallMemoryMap<'a> {
    memory: RefMut<'a, &'a mut [u8]>,
    offset: usize,
}

impl<'a> SmallMemoryMap<'a> {
    /// Create a new small memory map
    pub(crate) fn new(
        memory: RefMut<'a, &'a mut [u8]>,
        offset: usize,
    ) -> Result<Self, MemoryMapError> {
        // Calculate required memory size for small map:
        // - First level: 1 word to track available blocks in level 2
        // - Second level: 64 words (one per bit in first level)
        let required_size = (1 + 64) * size_of::<u64>();

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

        // Safe to use cast_slice_mut because we already checked alignment in the
        // constructor
        let u64_slice = cast_slice_mut::<u8, u64>(&mut self.memory[self.offset..]);

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

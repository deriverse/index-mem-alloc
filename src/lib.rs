mod get_first_zero_bit;

use bytemuck::{cast_slice_mut};
use std::cell::RefMut;
use crate::get_first_zero_bit::get_first_zero_bit;

/// Error types that can occur during memory map operations
#[derive(Debug, Clone, Copy)]
pub enum MemoryMapError {
    InvalidOffset,
    NoAvailableSlots,
    AlignmentError,
    InsufficientMemory,
    InvalidIndex,
    IndexOutOfBounds,
}

/// Defines the type of memory map
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryMapType {
    /// Standard 3-level memory map with 64 bits in first level
    Standard,
    /// Trade 3-level memory map with 4 bits in first level
    Trade,
    /// Small 2-level memory map
    Small,
}

impl MemoryMapType {
    /// Returns the number of bits in the first level
    pub fn first_level_bits(&self) -> isize {
        match self {
            Self::Standard => 64,
            Self::Trade => 4,
            Self::Small => 64,
        }
    }

    /// Returns the base offset of the third level
    pub fn third_level_base(&self) -> usize {
        match self {
            Self::Standard => 65, // 1 + 64
            Self::Trade => 5,     // 1 + 4
            Self::Small => 0,     // Not applicable
        }
    }

    /// Returns whether this memory map has a third level
    pub fn has_third_level(&self) -> bool {
        match self {
            Self::Standard | Self::Trade => true,
            Self::Small => false,
        }
    }
}

/// Trait for all memory map operations
pub trait MemoryMap {
    /// Get the type of this memory map
    fn map_type(&self) -> MemoryMapType;

    /// Allocate a new slot
    fn alloc(&mut self) -> Result<isize, MemoryMapError>;

    /// Deallocate a previously allocated slot
    fn dealloc(&mut self, index: isize) -> Result<(), MemoryMapError>;
}

/// Memory map implementation
pub struct MemoryMapImpl<'a> {
    memory: RefMut<'a, [u8]>,
    offset: usize,
    map_type: MemoryMapType,
}

impl<'a> MemoryMapImpl<'a> {
    /// Create a new memory map
    pub fn new(
        memory: RefMut<'a, [u8]>,
        offset: usize,
        map_type: MemoryMapType,
    ) -> Result<Self, MemoryMapError> {
        // Check offset validity
        if offset >= memory.len() {
            return Err(MemoryMapError::InvalidOffset);
        }

        // Check alignment for u64
        if (memory.as_ptr() as usize + offset) % align_of::<u64>() != 0 {
            return Err(MemoryMapError::AlignmentError);
        }

        // Calculate required memory size:
        // - First level: 1 word to track available blocks in level 2
        // - Second level: N words (one per bit in first level)
        // - Third level: N * 64 words (one per bit in second level)
        let required_size = match map_type {
            // Standard: 1 word (level 1) + 64 words (level 2) + 64*64 words (level 3)
            MemoryMapType::Standard => (1 + 64 + 64 * 64) * size_of::<u64>(),
            // Trade: 1 word (level 1) + 4 words (level 2) + 4*64 words (level 3)
            MemoryMapType::Trade => (1 + 4 + 4 * 64) * size_of::<u64>(),
            // Small: 1 word (level 1) + 64 words (level 2)
            MemoryMapType::Small => (1 + 64) * size_of::<u64>(),
        };

        // Check if there's enough memory
        if memory.len() - offset < required_size {
            return Err(MemoryMapError::InsufficientMemory);
        }

        Ok(Self {
            memory,
            offset,
            map_type,
        })
    }

    /// Create a standard memory map
    pub fn standard(memory: RefMut<'a, [u8]>, offset: usize) -> Result<Self, MemoryMapError> {
        Self::new(memory, offset, MemoryMapType::Standard)
    }

    /// Create a trade memory map
    pub fn trade(memory: RefMut<'a, [u8]>, offset: usize) -> Result<Self, MemoryMapError> {
        Self::new(memory, offset, MemoryMapType::Trade)
    }

    /// Create a small memory map
    pub fn small(memory: RefMut<'a, [u8]>, offset: usize) -> Result<Self, MemoryMapError> {
        Self::new(memory, offset, MemoryMapType::Small)
    }
}

impl<'a> MemoryMap for MemoryMapImpl<'a> {
    fn map_type(&self) -> MemoryMapType {
        self.map_type
    }

    fn alloc(&mut self) -> Result<isize, MemoryMapError> {
        // Safe to use `cast_slice_mut` because we already checked alignment in the constructor
        let u64_slice = cast_slice_mut::<u8, u64>(&mut self.memory[self.offset..]);

        // Get first level bits and base offsets
        let first_level_bits = self.map_type.first_level_bits();

        // First level allocation
        let first = get_first_zero_bit(u64_slice[0], first_level_bits)?;

        // Second level allocation
        let second_idx = 1 + first as usize;
        if second_idx >= u64_slice.len() {
            return Err(MemoryMapError::InsufficientMemory);
        }

        let second = get_first_zero_bit(u64_slice[second_idx], 64)?;

        // For Small memory map, we're done after two levels
        if self.map_type == MemoryMapType::Small {
            // Mark as allocated
            u64_slice[second_idx] |= 1 << second;
            if u64_slice[second_idx] == u64::MAX {
                u64_slice[0] |= 1 << first;
            }

            return Ok((first << 6) + second);
        }

        // Third level allocation for Standard and Trade
        let third_base = self.map_type.third_level_base();
        let third_idx = third_base + (first as usize * 64) + second as usize;

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

    fn dealloc(&mut self, index: isize) -> Result<(), MemoryMapError> {
        // Check for negative index
        if index < 0 {
            return Err(MemoryMapError::InvalidIndex);
        }

        // Check upper bound based on memory map type
        let max_index = match self.map_type {
            MemoryMapType::Standard => (64 << 12) - 1, // 262143
            MemoryMapType::Trade => (4 << 12) - 1,     // 16383
            MemoryMapType::Small => (64 << 6) - 1,     // 4095
        };

        if index > max_index {
            return Err(MemoryMapError::InvalidIndex);
        }

        // Safe to use cast_slice_mut because we already checked alignment in the constructor
        let u64_slice = cast_slice_mut::<u8, u64>(&mut self.memory[self.offset..]);

        if self.map_type == MemoryMapType::Small {
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
        } else {
            // Standard/Trade memory map - 3 levels
            let first = index >> 12;
            let second = (index & 0xfff) >> 6;
            let second_idx = 1 + first as usize;
            let third_idx = self.map_type.third_level_base() + (index >> 6) as usize;

            // Validate array bounds before access
            if third_idx >= u64_slice.len() || second_idx >= u64_slice.len() {
                return Err(MemoryMapError::IndexOutOfBounds);
            }

            // Clear allocation bits
            u64_slice[third_idx] &= u64::MAX - (1 << (index & 0x3f));
            u64_slice[second_idx] &= u64::MAX - (1 << second);
            u64_slice[0] &= u64::MAX - (1 << first);
        }

        Ok(())
    }
}
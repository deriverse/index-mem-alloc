mod get_first_zero_bit;
mod max_memory_map;
mod small_memory_map;
mod trade_memory_map;

use crate::{
    max_memory_map::MaxMemoryMap, small_memory_map::SmallMemoryMap,
    trade_memory_map::StandardMemoryMap,
};
use std::{cell::RefMut, mem::align_of};

/// Error types that can occur during memory map operations
#[derive(Debug, Clone, Copy)]
pub enum MemoryMapError {
    InvalidOffset,
    NoAvailableSlots,
    AlignmentError,
    InsufficientMemory,
    InvalidIndex,
    IndexOutOfBounds,
    InvalidMapType,
}

/// Available memory map types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapType {
    /// 3-level memory map with 64 bits in first level
    Max,
    /// 3-level memory map with 4 bits in first level
    Standard,
    /// 2-level memory map
    Small,
}

/// Memory map implementations
pub enum MemoryMap<'a> {
    /// 3-level memory map with 64 bits in first level
    Max(MaxMemoryMap<'a>),
    /// 3-level memory map with 4 bits in first level
    Standard(StandardMemoryMap<'a>),
    /// 2-level memory map
    Small(SmallMemoryMap<'a>),
}

impl<'a> MemoryMap<'a> {
    /// Create a new memory map
    pub fn new(
        memory: RefMut<'a, &'a mut [u8]>,
        offset: usize,
        map_type: MapType,
    ) -> Result<Self, MemoryMapError> {
        // Check offset validity
        if offset >= memory.len() {
            return Err(MemoryMapError::InvalidOffset);
        }

        // Check alignment for u64
        if (memory.as_ptr() as usize + offset) % align_of::<u64>() != 0 {
            return Err(MemoryMapError::AlignmentError);
        }

        // Create the appropriate memory map implementation
        match map_type {
            MapType::Max => Ok(Self::Max(MaxMemoryMap::new(memory, offset)?)),
            MapType::Standard => Ok(Self::Standard(StandardMemoryMap::new(memory, offset)?)),
            MapType::Small => Ok(Self::Small(SmallMemoryMap::new(memory, offset)?)),
        }
    }

    /// Allocate a new slot
    pub fn alloc(&mut self) -> Result<usize, MemoryMapError> {
        match self {
            Self::Max(map) => map.alloc(),
            Self::Standard(map) => map.alloc(),
            Self::Small(map) => map.alloc(),
        }
    }

    /// Deallocate a previously allocated slot
    pub fn dealloc(&mut self, index: usize) -> Result<(), MemoryMapError> {
        match self {
            Self::Max(map) => map.dealloc(index),
            Self::Standard(map) => map.dealloc(index),
            Self::Small(map) => map.dealloc(index),
        }
    }
}

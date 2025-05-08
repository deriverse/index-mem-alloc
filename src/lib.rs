mod get_first_zero_bit;
mod small_memory_map;
mod standard_memory_map;
mod trade_memory_map;

use crate::{
    small_memory_map::SmallMemoryMap, standard_memory_map::StandardMemoryMap,
    trade_memory_map::TradeMemoryMap,
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
    /// Standard 3-level memory map with 64 bits in first level
    Standard,
    /// Trade 3-level memory map with 4 bits in first level
    Trade,
    /// Small 2-level memory map
    Small,
}

/// Memory map implementations
pub enum MemoryMap<'a> {
    /// Standard 3-level memory map with 64 bits in first level
    Standard(StandardMemoryMap<'a>),
    /// Trade 3-level memory map with 4 bits in first level
    Trade(TradeMemoryMap<'a>),
    /// Small 2-level memory map
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
            MapType::Standard => Ok(Self::Standard(StandardMemoryMap::new(memory, offset)?)),
            MapType::Trade => Ok(Self::Trade(TradeMemoryMap::new(memory, offset)?)),
            MapType::Small => Ok(Self::Small(SmallMemoryMap::new(memory, offset)?)),
        }
    }

    /// Allocate a new slot
    pub fn alloc(&mut self) -> Result<usize, MemoryMapError> {
        match self {
            Self::Standard(map) => map.alloc(),
            Self::Trade(map) => map.alloc(),
            Self::Small(map) => map.alloc(),
        }
    }

    /// Deallocate a previously allocated slot
    pub fn dealloc(&mut self, index: usize) -> Result<(), MemoryMapError> {
        match self {
            Self::Standard(map) => map.dealloc(index),
            Self::Trade(map) => map.dealloc(index),
            Self::Small(map) => map.dealloc(index),
        }
    }
}

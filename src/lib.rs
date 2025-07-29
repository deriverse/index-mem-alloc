mod get_first_zero_bit;
mod max_memory_map;
mod small_memory_map;
mod standard_memory_map;

use crate::{
    max_memory_map::MaxMemoryMap, small_memory_map::SmallMemoryMap,
    standard_memory_map::StandardMemoryMap,
};
use solana_program::account_info::AccountInfo;
use std::{
    mem::{align_of, size_of},
    ptr::NonNull,
};

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
    DoubleAllocation(usize),
    NullPointer,
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
#[derive(Clone, Debug)]
pub enum MemoryMap {
    /// 3-level memory map with 64 bits in first level
    Max(MaxMemoryMap),
    /// 3-level memory map with 4 bits in first level
    Standard(StandardMemoryMap),
    /// 2-level memory map
    Small(SmallMemoryMap),
}

impl MemoryMap {
    pub const fn len(&self) -> usize {
        match self {
            Self::Small(_) => SmallMemoryMap::SIZE,
            Self::Standard(_) => StandardMemoryMap::SIZE,
            Self::Max(_) => MaxMemoryMap::SIZE,
        }
    }
}

impl PartialEq for MemoryMap {
    fn eq(&self, other: &Self) -> bool {
        if self.get_type() != other.get_type() {
            return false;
        }

        let words_to_compare = self.len() / size_of::<u64>();
        let self_mem = self.get_memory();
        let other_mem = other.get_memory();

        (0..words_to_compare).all(|index| {
            match (
                get_u64(self_mem, self.len(), index),
                get_u64(other_mem, other.len(), index),
            ) {
                (Ok(a), Ok(b)) => a == b,
                _ => false,
            }
        })
    }
}

impl MemoryMap {
    /// Create a new memory map from AccountInfo
    ///
    /// # Safety
    /// This is safe in Solana context where:
    /// - Programs are single-threaded
    /// - AccountInfo lives for the entire process_instruction call
    /// - There's no concurrent access to the data
    pub fn new(
        account: &AccountInfo,
        offset: usize,
        map_type: MapType,
    ) -> Result<Self, MemoryMapError> {
        let mut data = account.data.borrow_mut();
        Self::new_from_slice(&mut data, offset, map_type)
    }

    /// Create a new memory map from mutable byte slice
    pub fn new_from_slice(
        data: &mut [u8],
        offset: usize,
        map_type: MapType,
    ) -> Result<Self, MemoryMapError> {
        // Check offset validity
        if offset >= data.len() {
            return Err(MemoryMapError::InvalidOffset);
        }

        // Check alignment for u64
        let ptr = data[offset..].as_mut_ptr();
        if (ptr as usize) % align_of::<u64>() != 0 {
            return Err(MemoryMapError::AlignmentError);
        }

        // Create NonNull pointer - guaranteed to be non-null
        let memory = NonNull::new(ptr).ok_or(MemoryMapError::NullPointer)?;

        let remaining_size = data.len() - offset;

        // Create the appropriate memory map implementation
        match map_type {
            MapType::Max => {
                if remaining_size < MaxMemoryMap::SIZE {
                    return Err(MemoryMapError::InsufficientMemory);
                }
                Ok(Self::Max(MaxMemoryMap { memory }))
            }
            MapType::Standard => {
                if remaining_size < StandardMemoryMap::SIZE {
                    return Err(MemoryMapError::InsufficientMemory);
                }
                Ok(Self::Standard(StandardMemoryMap { memory }))
            }
            MapType::Small => {
                if remaining_size < SmallMemoryMap::SIZE {
                    return Err(MemoryMapError::InsufficientMemory);
                }
                Ok(Self::Small(SmallMemoryMap { memory }))
            }
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

    /// Mark a specific index as allocated
    pub fn alloc_at(&mut self, index: usize) -> Result<(), MemoryMapError> {
        match self {
            MemoryMap::Max(map) => map.alloc_at(index),
            MemoryMap::Standard(map) => map.alloc_at(index),
            MemoryMap::Small(map) => map.alloc_at(index),
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

    /// Check if a specific index is allocated
    pub fn is_allocated(&self, index: usize) -> Result<bool, MemoryMapError> {
        match self {
            Self::Max(map) => map.is_allocated(index),
            Self::Standard(map) => map.is_allocated(index),
            Self::Small(map) => map.is_allocated(index),
        }
    }

    /// Reset all allocations, clearing the entire memory map
    pub fn reset(&mut self) -> Result<(), MemoryMapError> {
        match self {
            Self::Max(map) => map.reset(),
            Self::Standard(map) => map.reset(),
            Self::Small(map) => map.reset(),
        }
    }

    pub fn get_type(&self) -> MapType {
        match self {
            MemoryMap::Max(_) => MapType::Max,
            MemoryMap::Standard(_) => MapType::Standard,
            MemoryMap::Small(_) => MapType::Small,
        }
    }

    fn get_memory(&self) -> NonNull<u8> {
        match self {
            MemoryMap::Max(map) => map.memory,
            MemoryMap::Standard(map) => map.memory,
            MemoryMap::Small(map) => map.memory,
        }
    }
}

/// Helper function to get mutable u64 at specified index
#[inline]
pub(crate) fn get_u64_mut<'a>(
    memory: NonNull<u8>,
    size: usize,
    index: usize,
) -> Result<&'a mut u64, MemoryMapError> {
    if index * size_of::<u64>() >= size {
        return Err(MemoryMapError::IndexOutOfBounds);
    }

    unsafe {
        let ptr = memory.as_ptr().add(index * size_of::<u64>()) as *mut u64;
        Ok(&mut *ptr)
    }
}

/// Helper function to get u64 at specified index
#[inline]
pub(crate) fn get_u64<'a>(
    memory: NonNull<u8>,
    size: usize,
    index: usize,
) -> Result<&'a u64, MemoryMapError> {
    if index * size_of::<u64>() >= size {
        return Err(MemoryMapError::IndexOutOfBounds);
    }

    unsafe {
        let ptr = memory.as_ptr().add(index * size_of::<u64>()) as *const u64;
        Ok(&*ptr)
    }
}

#[cfg(test)]
pub(crate) fn create_aligned_memory(size: usize) -> Vec<u8> {
    let mut data = vec![0u8; size + 8]; // Add extra space for alignment

    // Ensure proper alignment
    let ptr = data.as_ptr();
    let misalignment = ptr as usize % 8;
    if misalignment != 0 {
        data.rotate_left(8 - misalignment);
    }

    data
}

#[cfg(test)]
mod tests {

    use super::*;

    fn create_aligned_buffer(size: usize) -> Vec<u8> {
        let mut data = vec![0u8; size + 8];
        let ptr = data.as_ptr();
        let misalignment = ptr as usize % 8;
        if misalignment != 0 {
            data.rotate_left(8 - misalignment);
        }
        data
    }

    #[test]
    fn eq_test() {
        let mut buffer = create_aligned_buffer(SmallMemoryMap::SIZE);
        let map = MemoryMap::new_from_slice(&mut buffer, 0, MapType::Small).unwrap();

        let mut buffer = create_aligned_buffer(SmallMemoryMap::SIZE);
        let map2 = MemoryMap::new_from_slice(&mut buffer, 0, MapType::Small).unwrap();

        assert_eq!(map, map2, "Empty maps with similar type must be the same");

        let mut buffer = create_aligned_buffer(StandardMemoryMap::SIZE);
        let map3 = MemoryMap::new_from_slice(&mut buffer, 0, MapType::Standard).unwrap();

        assert_ne!(
            map, map3,
            "Empty maps with different types must not be similar"
        );

        let required_size = StandardMemoryMap::SIZE;
        let mut data = create_aligned_buffer(required_size);
        let mut data2 = create_aligned_buffer(required_size);

        let mut map = MemoryMap::new_from_slice(&mut data, 0, MapType::Standard).unwrap();
        let mut map2 = MemoryMap::new_from_slice(&mut data2, 0, MapType::Standard).unwrap();

        let transform = |map: &mut MemoryMap| {
            map.alloc().unwrap();
            map.alloc().unwrap();
            map.alloc_at(100).unwrap();
            map.alloc_at(200).unwrap();
            map.alloc_at(300).unwrap();
        };

        transform(&mut map);
        transform(&mut map2);

        assert_eq!(
            map, map2,
            "After simmilar transformation maps must be the same"
        );
        map.alloc_at(10).unwrap();

        assert_ne!(
            map, map2,
            "Adter different sequence of transformation maps must not be same"
        );

        map.reset().unwrap();
        map2.reset().unwrap();

        assert_eq!(map, map2, "After reseteting 2 maps they must be the same");
        let mut data = create_aligned_buffer(required_size);

        let new_map = MemoryMap::new_from_slice(&mut data, 0, MapType::Standard).unwrap();

        assert_eq!(new_map, map, "Reseted map must be equal to an empty map");
    }

    #[test]
    fn test_memory_map_creation() {
        let mut buffer = create_aligned_buffer(SmallMemoryMap::SIZE * 2);

        // Test successful creation
        let result = MemoryMap::new_from_slice(&mut buffer, 0, MapType::Small);
        assert!(result.is_ok());

        // Test alignment error
        let unaligned_result = MemoryMap::new_from_slice(&mut buffer, 1, MapType::Small);
        assert!(matches!(
            unaligned_result,
            Err(MemoryMapError::AlignmentError)
        ));

        // Test invalid offset
        let invalid_result = MemoryMap::new_from_slice(&mut buffer, 2048, MapType::Small);
        assert!(matches!(invalid_result, Err(MemoryMapError::InvalidOffset)));
    }

    #[test]
    fn test_memory_map_operations() {
        let mut buffer = create_aligned_buffer(SmallMemoryMap::SIZE);
        let mut map = MemoryMap::new_from_slice(&mut buffer, 0, MapType::Small).unwrap();

        // Test allocation
        let idx1 = map.alloc().unwrap();
        let idx2 = map.alloc().unwrap();
        assert_ne!(idx1, idx2);

        // Test by index allocation
        let double_alloc = map.alloc_at(idx2);

        assert!(
            matches!(double_alloc, Err(MemoryMapError::DoubleAllocation(index)) if index == idx2),
            "Incorrect index in Double Allocation error, expected error on index: {}",
            idx2
        );

        map.alloc_at(1423)
            .expect("Allocation should have been performed succesfully");

        // Test deallocation and reuse
        map.dealloc(idx1).unwrap();
        let idx3 = map.alloc().unwrap();
        assert_eq!(idx1, idx3);
    }
}

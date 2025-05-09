mod get_first_zero_bit;
mod max_memory_map;
mod small_memory_map;
mod trade_memory_map;

use crate::{
    max_memory_map::MaxMemoryMap, small_memory_map::SmallMemoryMap,
    trade_memory_map::StandardMemoryMap,
};
use std::{cell::RefCell, mem::align_of, rc::Rc};

/// Error types that can occur during memory map operations
#[derive(Debug, Clone, Copy)]
pub enum MemoryMapError {
    CantBorrowMemory,
    CantBorrowMutMemory,
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
#[derive(Clone)]
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
        memory: Rc<RefCell<&'a mut [u8]>>,
        offset: usize,
        map_type: MapType,
    ) -> Result<Self, MemoryMapError> {
        {
            let memory_ref = memory
                .try_borrow()
                .map_err(|_| MemoryMapError::CantBorrowMemory)?;
            // Check offset validity
            if offset >= memory_ref.len() {
                return Err(MemoryMapError::InvalidOffset);
            }

            // Check alignment for u64
            if (memory_ref.as_ptr() as usize + offset) % align_of::<u64>() != 0 {
                return Err(MemoryMapError::AlignmentError);
            }
        }

        // Create the appropriate memory map implementation
        // The inner constructors use `memory.borrow()` which is safe here because:
        // - We've already verified the memory can be borrowed with try_borrow() above
        // - Solana programs run in a single-threaded environment
        match map_type {
            MapType::Max => Ok(Self::Max(MaxMemoryMap::new(memory.clone(), offset)?)),
            MapType::Standard => Ok(Self::Standard(StandardMemoryMap::new(
                memory.clone(),
                offset,
            )?)),
            MapType::Small => Ok(Self::Small(SmallMemoryMap::new(memory.clone(), offset)?)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, mem::{size_of}, rc::Rc};

    // Create aligned memory buffer for testing
    fn create_aligned_buffer(size: usize) -> Rc<RefCell<&'static mut [u8]>> {
        // Create a buffer large enough for any type of map
        let required_size = (1 + 64 + 64 * 64) * size_of::<u64>(); // Size for Max map
        let size = if size < required_size { required_size } else { size };

        let data = vec![0u8; size + 16]; // Add extra space for alignment

        // Ensure 8-byte alignment
        let ptr = data.as_ptr();
        let misalignment = ptr as usize % 8;
        let alignment_offset = if misalignment == 0 { 0 } else { 8 - misalignment };

        // SAFETY: This is safe in tests since the data lives for the entire test duration
        let data_ptr = Box::leak(data.into_boxed_slice());
        let aligned_slice = &mut data_ptr[alignment_offset..];

        // Verify alignment
        assert_eq!(
            (aligned_slice.as_ptr() as usize) % 8,
            0,
            "Buffer should be 8-byte aligned"
        );

        Rc::new(RefCell::new(aligned_slice))
    }

    // Create unaligned memory buffer for testing
    fn create_unaligned_buffer(size: usize) -> Rc<RefCell<&'static mut [u8]>> {
        // Create a buffer with unaligned address
        let data = vec![0u8; size + 16]; // Add extra space for alignment manipulation

        // Ensure we start with aligned memory, then offset by 1
        let ptr = data.as_ptr();
        let misalignment = ptr as usize % 8;
        let alignment_offset = if misalignment == 0 { 1 } else { 9 - misalignment };

        // SAFETY: This is safe in tests since the data lives for the entire test duration
        let data_ptr = Box::leak(data.into_boxed_slice());
        let unaligned_slice = &mut data_ptr[alignment_offset..];

        // Verify unalignment
        assert_ne!(
            (unaligned_slice.as_ptr() as usize) % 8,
            0,
            "Buffer should not be 8-byte aligned"
        );

        Rc::new(RefCell::new(unaligned_slice))
    }

    #[test]
    fn test_memory_map_alignment() {
        // Create buffers with sufficient size for any memory map
        let aligned_rc = create_aligned_buffer(10000);
        let unaligned_rc = create_unaligned_buffer(10000);

        // Double-check our test setup
        {
            let aligned_ref = aligned_rc.borrow();
            let unaligned_ref = unaligned_rc.borrow();

            // Print diagnostic info
            println!("Aligned buffer address: {:p}, aligned? {}",
                     aligned_ref.as_ptr(),
                     (aligned_ref.as_ptr() as usize) % 8 == 0);

            println!("Unaligned buffer address: {:p}, aligned? {}",
                     unaligned_ref.as_ptr(),
                     (unaligned_ref.as_ptr() as usize) % 8 == 0);

            // Verify buffer lengths are sufficient
            println!("Aligned buffer length: {}", aligned_ref.len());
            println!("Unaligned buffer length: {}", unaligned_ref.len());
        }

        // Test 1: Aligned memory should work fine with Small map type (smallest requirements)
        let aligned_result = MemoryMap::new(aligned_rc.clone(), 0, MapType::Small);
        if let Err(err) = &aligned_result {
            println!("Failed to create map with aligned memory: {:?}", err);
        }
        assert!(aligned_result.is_ok(),
                "MemoryMap creation should succeed with aligned memory");

        // Test 2: Unaligned memory should produce alignment error
        let unaligned_result = MemoryMap::new(unaligned_rc.clone(), 0, MapType::Small);
        assert!(
            matches!(unaligned_result, Err(MemoryMapError::AlignmentError)),
            "MemoryMap::new should fail with alignment error for unaligned memory"
        );

        // Test 3: Even with aligned memory, an unaligned offset should fail
        let unaligned_offset = 1; // This should make the effective address unaligned
        let offset_result = MemoryMap::new(aligned_rc.clone(), unaligned_offset, MapType::Small);
        assert!(
            matches!(offset_result, Err(MemoryMapError::AlignmentError)),
            "MemoryMap::new should fail with alignment error for unaligned offset"
        );
    }
}
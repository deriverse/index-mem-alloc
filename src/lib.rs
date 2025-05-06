const U64MAX: u64 = 0xffffffffffffffff;

#[inline(always)]
fn get_first_zero_bit(pattern: u64, bits: isize) -> isize {
    if bits < 33 {
        for j in 0..bits {
            if pattern & (1 << j) == 0 {
                return j;
            }
        }
    } else if pattern & 0xffffffff == 0xffffffff {
        if pattern & 0xffff00000000 == 0xffff00000000 {
            for j in 48..bits {
                if pattern & (1 << j) == 0 {
                    return j;
                }
            }
        } else {
            for j in 32..bits.min(48) {
                if pattern & (1 << j) == 0 {
                    return j;
                }
            }
        }
    } else if pattern & 0xffff == 0xffff {
        for j in 16..32 {
            if pattern & (1 << j) == 0 {
                return j;
            }
        }
    } else {
        for j in 0..16 {
            if pattern & (1 << j) == 0 {
                return j;
            }
        }
    }
    -1
}

pub trait MemoryMapAlloc {
    fn alloc(&self) -> isize;
}

pub trait MemoryMapDealloc {
    fn dealloc(&self, index: isize);
}

#[derive(Clone, Copy)]
pub struct MemoryMap {
    pub entry: *mut u64,
    pub blocks: isize,
}

impl MemoryMapAlloc for MemoryMap {
    fn alloc(&self) -> isize {
        unsafe {
            let first = get_first_zero_bit(*self.entry, self.blocks);
            if first >= 0 {
                let second_ptr = self.entry.offset(1 + first);
                let second = get_first_zero_bit(*second_ptr, 64);
                let third_ptr = self.entry.offset(65 + (first << 6) + second);
                let third = get_first_zero_bit(*third_ptr, 64);
                *third_ptr += 1 << third;
                if *third_ptr == U64MAX {
                    *second_ptr += 1 << second;
                    if *second_ptr == U64MAX {
                        *self.entry += 1 << first;
                    }
                }
                (first << 12) + (second << 6) + third
            } else {
                -1
            }
        }
    }
}

impl MemoryMapDealloc for MemoryMap {
    fn dealloc(&self, index: isize) {
        let first = index >> 12;
        let second = (index & 0xfff) >> 6;
        unsafe {
            *(self.entry.offset(65 + (index >> 6))) &= U64MAX - (1 << (index & 0x3f));
            *(self.entry.offset(1 + first)) &= U64MAX - (1 << second);
            *self.entry &= U64MAX - (1 << first);
        }
    }
}

#[derive(Clone, Copy)]
pub struct TradeMemoryMap {
    pub entry: *mut u64,
}

impl MemoryMapAlloc for TradeMemoryMap {
    fn alloc(&self) -> isize {
        unsafe {
            let first = get_first_zero_bit(*self.entry, 4);
            if first >= 0 {
                let second_ptr = self.entry.offset(1 + first);
                let second = get_first_zero_bit(*second_ptr, 64);
                let third_ptr = self.entry.offset(5 + (first << 6) + second);
                let third = get_first_zero_bit(*third_ptr, 64);
                *third_ptr += 1 << third;
                if *third_ptr == U64MAX {
                    *second_ptr += 1 << second;
                    if *second_ptr == U64MAX {
                        *self.entry += 1 << first;
                    }
                }
                (first << 12) + (second << 6) + third
            } else {
                -1
            }
        }
    }
}

impl MemoryMapDealloc for TradeMemoryMap {
    fn dealloc(&self, index: isize) {
        let first = index >> 12;
        let second = (index & 0xfff) >> 6;
        unsafe {
            *(self.entry.offset(5 + (index >> 6))) &= U64MAX - (1 << (index & 0x3f));
            *(self.entry.offset(1 + first)) &= U64MAX - (1 << second);
            *self.entry &= U64MAX - (1 << first);
        }
    }
}

#[derive(Clone, Copy)]
pub struct SmallMemoryMap {
    pub entry: *mut u64,
}

impl MemoryMapAlloc for SmallMemoryMap {
    fn alloc(&self) -> isize {
        unsafe {
            let first = get_first_zero_bit(*self.entry, 64);
            if first >= 0 {
                let second_ptr = self.entry.offset(1 + first);
                let second = get_first_zero_bit(*second_ptr, 64);
                *second_ptr += 1 << second;
                if *second_ptr == U64MAX {
                    *self.entry += 1 << first;
                }
                (first << 6) + second
            } else {
                -1
            }
        }
    }
}

impl MemoryMapDealloc for SmallMemoryMap {
    fn dealloc(&self, index: isize) {
        let first = index >> 6;
        unsafe {
            *(self.entry.offset(1 + first)) &= U64MAX - (1 << (index & 0x3f));
            *self.entry &= U64MAX - (1 << first);
        }
    }
}

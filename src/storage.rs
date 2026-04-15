// src/storage.rs
//! SoA storage with optimized double-buffer and dirty-page tracking

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferMode {
    Single,
    Double,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FloatOffset(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IntOffset(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FieldIndex(pub u16);

const PAGE_SIZE: usize = 512;

pub struct Storage {
    buf_a: Vec<f64>,
    buf_b: Vec<f64>,
    mode: BufferMode,
    dirty_pages: Vec<bool>,
    pub ints: Vec<i64>,
    float_allocated: usize,
    int_allocated: usize,
    snapshot_valid: bool,
    tick_in_progress: bool,
}

impl Storage {
    pub fn new() -> Self {
        Self::with_mode(BufferMode::Single)
    }

    pub fn with_mode(mode: BufferMode) -> Self {
        Self {
            buf_a: Vec::new(),
            buf_b: Vec::new(),
            mode,
            dirty_pages: Vec::new(),
            ints: Vec::new(),
            float_allocated: 0,
            int_allocated: 0,
            snapshot_valid: false,
            tick_in_progress: false,
        }
    }

    pub fn set_mode(&mut self, mode: BufferMode) {
        self.mode = mode;
        if mode == BufferMode::Double {
            self.buf_b.resize(self.buf_a.len(), 0.0);
            self.buf_b.copy_from_slice(&self.buf_a);
            let page_count = (self.buf_a.len() + PAGE_SIZE - 1) / PAGE_SIZE;
            self.dirty_pages = vec![false; page_count];
            self.snapshot_valid = true;
        }
    }

    pub fn mode(&self) -> BufferMode {
        self.mode
    }

    pub fn alloc_floats(&mut self, count: usize) -> FloatOffset {
        let offset = self.float_allocated;
        self.float_allocated += count;
        if self.buf_a.len() < self.float_allocated {
            self.buf_a.resize(self.float_allocated, 0.0);
            if self.mode == BufferMode::Double {
                self.buf_b.resize(self.float_allocated, 0.0);
                let page_count = (self.float_allocated + PAGE_SIZE - 1) / PAGE_SIZE;
                self.dirty_pages.resize(page_count, false);
            }
        }
        FloatOffset(offset as u32)
    }

    pub fn alloc_ints(&mut self, count: usize) -> IntOffset {
        let offset = self.int_allocated;
        self.int_allocated += count;
        if self.ints.len() < self.int_allocated {
            self.ints.resize(self.int_allocated, 0);
        }
        IntOffset(offset as u32)
    }

    #[inline(always)]
    pub fn read_float(&self, offset: FloatOffset, field: FieldIndex) -> f64 {
        let idx = offset.0 as usize + field.0 as usize;
        self.buf_a[idx]
    }

    #[inline(always)]
    pub fn init_float(&mut self, offset: FloatOffset, field: FieldIndex, value: f64) {
        let idx = offset.0 as usize + field.0 as usize;
        if idx < self.buf_a.len() {
            self.buf_a[idx] = value;
            if self.mode == BufferMode::Double && idx < self.buf_b.len() {
                self.buf_b[idx] = value;
            }
        }
    }

    #[inline(always)]
    pub fn read_abs(&self, abs: usize) -> f64 {
        if abs < self.buf_a.len() {
            self.buf_a[abs]
        } else {
            0.0
        }
    }

    #[inline(always)]
    pub fn write_abs(&mut self, abs: usize, value: f64) {
        debug_assert!(
            !self.tick_in_progress,
            "write_abs called during tick — use resonators for in-tick writes"
        );
        if abs < self.buf_a.len() {
            self.buf_a[abs] = value;
            if self.mode == BufferMode::Double && abs < self.buf_b.len() {
                self.buf_b[abs] = value;
            }
        }
    }

    #[inline]
    pub fn begin_tick(&mut self) {
        self.tick_in_progress = true;
    }

    #[inline]
    pub fn end_tick(&mut self) {
        self.tick_in_progress = false;
    }

    #[inline]
    pub fn commit(&mut self) {
        match self.mode {
            BufferMode::Single => {}
            BufferMode::Double => {
                let len = self.buf_a.len();
                for (page_idx, dirty) in self.dirty_pages.iter_mut().enumerate() {
                    if *dirty {
                        let start = page_idx * PAGE_SIZE;
                        let end = (start + PAGE_SIZE).min(len);
                        self.buf_b[start..end].copy_from_slice(&self.buf_a[start..end]);
                        *dirty = false;
                    }
                }
                self.snapshot_valid = true;
            }
        }
    }

    pub fn memory_bytes(&self) -> usize {
        self.buf_a.len() * 8 + self.ints.len() * 8
    }

    pub fn total_memory_bytes(&self) -> usize {
        let float_bytes = match self.mode {
            BufferMode::Single => self.buf_a.len() * 8,
            BufferMode::Double => (self.buf_a.len() + self.buf_b.len()) * 8,
        };
        float_bytes + self.ints.len() * 8
    }

    pub fn float_count(&self) -> usize {
        self.buf_a.len()
    }

    pub fn raw_ptrs(&mut self) -> StoragePtr {
        let dirty_pages_ptr = if self.mode == BufferMode::Double {
            self.dirty_pages.as_mut_ptr()
        } else {
            std::ptr::null_mut()
        };
        let dirty_pages_len = self.dirty_pages.len();

        match self.mode {
            BufferMode::Single => StoragePtr {
                read_floats: self.buf_a.as_ptr(),
                write_floats: self.buf_a.as_mut_ptr(),
                ints: self.ints.as_mut_ptr(),
                float_len: self.buf_a.len(),
                int_len: self.ints.len(),
                dirty_pages: dirty_pages_ptr,
                dirty_pages_len,
            },
            BufferMode::Double => StoragePtr {
                read_floats: self.buf_b.as_ptr(),
                write_floats: self.buf_a.as_mut_ptr(),
                ints: self.ints.as_mut_ptr(),
                float_len: self.buf_a.len().min(self.buf_b.len()),
                int_len: self.ints.len(),
                dirty_pages: dirty_pages_ptr,
                dirty_pages_len,
            },
        }
    }
}

impl Default for Storage {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
pub struct StoragePtr {
    pub read_floats: *const f64,
    pub write_floats: *mut f64,
    pub ints: *mut i64,
    pub float_len: usize,
    pub int_len: usize,
    dirty_pages: *mut bool,
    dirty_pages_len: usize,
}

unsafe impl Send for StoragePtr {}
unsafe impl Sync for StoragePtr {}

impl StoragePtr {
    #[inline(always)]
    pub unsafe fn read_float(&self, abs_offset: usize) -> f64 {
        debug_assert!(
            abs_offset < self.float_len,
            "read_float out of bounds: {} >= {}",
            abs_offset,
            self.float_len
        );
        *self.read_floats.add(abs_offset)
    }

    #[inline(always)]
    pub unsafe fn write_float(&self, abs_offset: usize, value: f64) {
        debug_assert!(
            abs_offset < self.float_len,
            "write_float out of bounds: {} >= {}",
            abs_offset,
            self.float_len
        );
        *self.write_floats.add(abs_offset) = value;
        
        if !self.dirty_pages.is_null() {
            let page = abs_offset / PAGE_SIZE;
            if page < self.dirty_pages_len {
                *self.dirty_pages.add(page) = true;
            }
        }
    }

    #[inline(always)]
    pub unsafe fn read_int(&self, abs_offset: usize) -> i64 {
        debug_assert!(abs_offset < self.int_len);
        *self.ints.add(abs_offset)
    }

    #[inline(always)]
    pub unsafe fn write_int(&self, abs_offset: usize, value: i64) {
        debug_assert!(abs_offset < self.int_len);
        *self.ints.add(abs_offset) = value;
    }
}
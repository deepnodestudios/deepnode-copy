use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::sync::Arc;
use parking_lot::Mutex;

/// 4096-byte (sector) aligned buffer required for Windows Direct I/O (FILE_FLAG_NO_BUFFERING).
pub struct AlignedBuffer {
    ptr: *mut u8,
    layout: Layout,
    capacity: usize,
    len: usize,
}

// Safety: The buffer owns its raw pointer and can be safely sent and shared across threads.
unsafe impl Send for AlignedBuffer {}
unsafe impl Sync for AlignedBuffer {}

impl AlignedBuffer {
    /// Creates a new buffer aligned to `alignment` (default 4096 bytes for disk sector alignment).
    pub fn new(capacity: usize, alignment: usize) -> Self {
        let align = if alignment == 0 || (alignment & (alignment - 1)) != 0 {
            4096
        } else {
            alignment
        };

        // Align capacity up to multiple of align
        let aligned_cap = (capacity + align - 1) & !(align - 1);
        let layout = Layout::from_size_align(aligned_cap, align)
            .expect("Invalid layout for aligned buffer");

        let ptr = unsafe { alloc_zeroed(layout) };
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }

        Self {
            ptr,
            layout,
            capacity: aligned_cap,
            len: 0,
        }
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.capacity) }
    }

    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn set_len(&mut self, len: usize) {
        assert!(len <= self.capacity, "len exceeds aligned capacity");
        self.len = len;
    }

    #[inline]
    pub fn reset(&mut self) {
        self.len = 0;
    }
}

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                dealloc(self.ptr, self.layout);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

/// A high-performance thread-safe memory pool of sector-aligned buffers to avoid continuous heap allocations.
#[derive(Clone)]
pub struct AlignedBufferPool {
    pool: Arc<Mutex<Vec<AlignedBuffer>>>,
    buffer_size: usize,
    alignment: usize,
}

impl AlignedBufferPool {
    pub fn new(buffer_size: usize, alignment: usize, initial_count: usize) -> Self {
        let mut buffers = Vec::with_capacity(initial_count);
        for _ in 0..initial_count {
            buffers.push(AlignedBuffer::new(buffer_size, alignment));
        }
        Self {
            pool: Arc::new(Mutex::new(buffers)),
            buffer_size,
            alignment,
        }
    }

    pub fn acquire(&self) -> AlignedBuffer {
        let mut guard = self.pool.lock();
        if let Some(mut buf) = guard.pop() {
            buf.reset();
            buf
        } else {
            AlignedBuffer::new(self.buffer_size, self.alignment)
        }
    }

    pub fn release(&self, mut buffer: AlignedBuffer) {
        buffer.reset();
        let mut guard = self.pool.lock();
        if guard.len() < 32 {
            guard.push(buffer);
        }
    }
}

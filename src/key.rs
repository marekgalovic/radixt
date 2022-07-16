use std::alloc::{alloc, dealloc, Layout};
use std::ops::Deref;
use std::ptr;

pub(crate) struct Key {
    inner: ptr::NonNull<u8>,
}

unsafe impl Send for Key {}
unsafe impl Sync for Key {}

impl Key {
    pub(crate) fn new(data: &[u8]) -> Self {
        assert!(data.len() < 256, "Key length must be < 256");
        // Allocate memory
        let ptr = unsafe { alloc(Self::layout(data.len() + 1)) };
        // Check that allocation was successful and convert to NonNull
        let inner = ptr::NonNull::new(ptr).expect("allocation failed");
        // Write length and data
        unsafe {
            // SAFETY
            // The pointer is guaranteed to be allocated with size >= 1
            ptr::write(inner.as_ptr(), data.len() as u8);
            ptr::copy(data.as_ptr(), inner.as_ptr().add(1), data.len());
        }

        Key { inner }
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        // The first byte represents key length
        (unsafe {
            // SAFETY
            // The pointer is guaranteed to be allocated with at least one byte
            *self.inner.as_ptr()
        }) as usize
    }

    #[inline(always)]
    fn layout(n: usize) -> Layout {
        Layout::array::<u8>(n).expect("invalid layout")
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            // SAFETY
            // The pointer is guaranteed to be allocated with known size
            dealloc(self.inner.as_ptr(), Self::layout(self.len() + 1))
        }
    }
}

impl Deref for Key {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe {
            // SAFETY
            // The pointer is guaranteed to be allocated with known size
            let v = self.inner.as_ptr().add(1) as *const u8;
            std::slice::from_raw_parts(v, self.len())
        }
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty() {
        let k = Key::new(&[]);

        assert_eq!(k.len(), 0);
        assert_eq!(k.deref(), &[]);
    }

    #[test]
    fn test_non_empty() {
        let k = Key::new(&[1, 2, 3, 4, 5]);
        assert_eq!(k.len(), 5);
        assert_eq!(k.deref(), &[1, 2, 3, 4, 5]);
        assert_eq!(&k[2..], &[3, 4, 5]);
        assert_eq!(&k[..2], &[1, 2]);
        assert_eq!(&k[2..4], &[3, 4]);
    }
}

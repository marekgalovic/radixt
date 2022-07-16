use std::alloc::{alloc, dealloc, realloc, Layout};
use std::ops::Deref;
use std::ptr;

#[repr(transparent)]
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

    pub(crate) fn extend(&mut self, data: &[u8]) {
        let new_len = self.len() + data.len();
        assert!(new_len < 256, "Cannot extend to key with length >= 256");

        let new_ptr = unsafe {
            // SAFETY
            // The current pointer is guaranteed to be non null and we are
            // reallocating to new size which is < 256.
            realloc(
                self.inner.as_ptr(),
                Self::layout(self.len() + 1),
                Self::layout(new_len + 1).size(),
            )
        };
        self.inner = ptr::NonNull::new(new_ptr).expect("allocation failed");
        let old_len = self.len();
        unsafe {
            // SAFETY
            // The pointer is guaranteed to be non null and was successfully
            // reallocated to fit new data.
            ptr::write(self.inner.as_ptr(), new_len as u8);
            ptr::copy(
                data.as_ptr(),
                self.inner.as_ptr().add(old_len + 1),
                data.len(),
            )
        }
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

    #[test]
    fn test_extend() {
        let mut k = Key::new(&[1, 2, 3, 4, 5]);
        assert_eq!(k.len(), 5);
        assert_eq!(k.deref(), &[1, 2, 3, 4, 5]);

        k.extend(&[6, 7, 8]);

        assert_eq!(k.len(), 8);
        assert_eq!(k.deref(), &[1, 2, 3, 4, 5, 6, 7, 8]);
    }
}

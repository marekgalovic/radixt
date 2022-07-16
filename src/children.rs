use std::alloc::{alloc, dealloc, realloc, Layout};
use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr;

use crate::node::Node;

/// Non-empty vector of T
/// The allocated capacity is always equal to the number of elements.
/// The children array must have at least one element and at most 256 elements.
#[repr(packed)]
pub(crate) struct Children<T> {
    len: u8,
    inner: ptr::NonNull<Node<T>>,
    _marker: PhantomData<T>,
}

unsafe impl<T: Send> Send for Children<T> {}
unsafe impl<T: Sync> Sync for Children<T> {}

impl<T> Children<T> {
    pub(crate) fn new(node: Node<T>) -> Self {
        // Allocate children array
        let ptr = unsafe { alloc(Self::layout(1)) };
        let inner = ptr::NonNull::new(ptr as *mut Node<T>).expect("allocation failed");
        // Insert node
        unsafe {
            // SAFETY
            // The pointer is successfuly allocated with capacity = 1
            ptr::write(inner.as_ptr(), node);
        }

        Children {
            len: 0,
            inner,
            _marker: PhantomData::default(),
        }
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.len as usize + 1
    }

    #[inline]
    pub(crate) fn insert(&mut self, idx: usize, node: Node<T>) {
        assert!(idx <= self.len(), "Insert index must be <= length");
        self.grow();

        if idx < self.len() {
            // Shift elements at idx to the right
            unsafe {
                // SAFETY
                // At this point, capacity for the new element was successfully
                // allocated.
                ptr::copy(
                    self.inner.as_ptr().add(idx),
                    self.inner.as_ptr().add(idx + 1),
                    self.len() - idx,
                );
            }
        }
        // Insert new node
        unsafe {
            // SAFETY
            // At this point, capacity for the new element was successfully
            // allocated.
            ptr::write(self.inner.as_ptr().add(idx), node);
        }
        self.len += 1;
    }

    #[inline]
    pub(crate) fn remove(&mut self, idx: usize) -> Node<T> {
        assert!(
            self.len() > 1,
            "Cannot remove last child. Drop children instead."
        );
        assert!(idx < self.len(), "Remove index must be < length");

        let removed = unsafe {
            // SAFETY
            // Pointer is guaranteed to be not null and index is checked to
            // be within bounds.
            ptr::read(self.inner.as_ptr().add(idx))
        };
        self.len -= 1;

        unsafe {
            // SAFETY
            // Pointer is guaranteed to be not null and index is checked to
            // be within bounds.
            ptr::copy(
                self.inner.as_ptr().add(idx + 1),
                self.inner.as_ptr().add(idx),
                self.len() - idx,
            );
        };
        self.shrink();
        removed
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, node: Node<T>) {
        self.insert(self.len(), node);
    }

    #[inline(always)]
    fn layout(n: usize) -> Layout {
        Layout::array::<Node<T>>(n).expect("invalid layout")
    }

    #[inline(always)]
    fn grow(&mut self) {
        assert!(
            self.len() < 256,
            "Cannot grow children array to more than 256 items"
        );

        let new_layout = Self::layout(self.len() + 1);
        let new_ptr = unsafe {
            // SAFETY
            // The old pointer is guaranteed to be allocated with size = len
            realloc(
                self.inner.as_ptr() as *mut u8,
                Self::layout(self.len()),
                new_layout.size(),
            )
        };
        self.inner = ptr::NonNull::new(new_ptr as *mut Node<T>).expect("allocation failed");
    }

    #[inline(always)]
    fn shrink(&mut self) {
        assert!(self.len() > 0, "Cannot shrink below 0");
        let new_layout = Self::layout(self.len());
        let new_ptr = unsafe {
            // SAFETY
            // The old pointer is guaranteed to be allocated with size = len + 1
            realloc(
                self.inner.as_ptr() as *mut u8,
                Self::layout(self.len() + 1),
                new_layout.size(),
            )
        };
        self.inner = ptr::NonNull::new(new_ptr as *mut Node<T>).expect("allocation failed");
    }
}

impl<T> Drop for Children<T> {
    fn drop(&mut self) {
        for i in 0..self.len() {
            // Drop nodes
            let _ = unsafe {
                // SAFETY
                // We are only reading from the allocated chunk
                ptr::read(self.inner.as_ptr().add(i))
            };
        }
        unsafe {
            // SAFETY
            // The pointer is guaranteed to be allocated with known size
            dealloc(self.inner.as_ptr() as *mut u8, Self::layout(self.len()))
        }
    }
}

impl<T> Deref for Children<T> {
    type Target = [Node<T>];

    fn deref(&self) -> &Self::Target {
        unsafe {
            // SAFETY
            // The pointer is guaranteed to be allocated with known size
            std::slice::from_raw_parts(self.inner.as_ptr(), self.len())
        }
    }
}

impl<T> DerefMut for Children<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            // SAFETY
            // The pointer is guaranteed to be allocated with known size
            std::slice::from_raw_parts_mut(self.inner.as_ptr(), self.len())
        }
    }
}

impl<T: Debug> Debug for Children<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc_dealloc() {
        let c: Children<()> = Children::new(Node::new(&[]));
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn test_add_items() {
        let mut c: Children<()> = Children::new(Node::new(&[]));

        assert_eq!(c.len(), 1);
        assert_eq!(c[0].key(), &[]);

        // Push
        c.push(Node::new(&[1, 2]));
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].key(), &[]);
        assert_eq!(c[1].key(), &[1, 2]);

        // Insert last
        c.insert(2, Node::new(&[3, 4]));
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].key(), &[]);
        assert_eq!(c[1].key(), &[1, 2]);
        assert_eq!(c[2].key(), &[3, 4]);

        // Insert mid
        c.insert(2, Node::new(&[2, 3]));
        assert_eq!(c.len(), 4);
        assert_eq!(c[0].key(), &[]);
        assert_eq!(c[1].key(), &[1, 2]);
        assert_eq!(c[2].key(), &[2, 3]);
        assert_eq!(c[3].key(), &[3, 4]);

        // Insert first
        c.insert(0, Node::new(&[0, 0]));
        assert_eq!(c.len(), 5);
        assert_eq!(c[0].key(), &[0, 0]);
        assert_eq!(c[1].key(), &[]);
        assert_eq!(c[2].key(), &[1, 2]);
        assert_eq!(c[3].key(), &[2, 3]);
        assert_eq!(c[4].key(), &[3, 4]);
    }

    #[test]
    fn test_push_full() {
        let mut c: Children<()> = Children::new(Node::new(0_u32.to_be_bytes().as_slice()));

        for i in 1..=255_u32 {
            c.push(Node::new(i.to_be_bytes().as_slice()));
        }

        assert_eq!(c.len(), 256);
        for i in 0..=255_u32 {
            assert_eq!(c[i as usize].key(), i.to_be_bytes().as_slice());
        }
    }

    #[test]
    #[should_panic]
    fn test_push_more_than_256_items() {
        let mut c: Children<()> = Children::new(Node::new(0_u32.to_be_bytes().as_slice()));

        for i in 1..=256_u32 {
            c.push(Node::new(i.to_be_bytes().as_slice()));
        }
    }

    #[test]
    fn test_remove_items() {
        let mut c: Children<()> = Children::new(Node::new(&[0, 1]));
        c.push(Node::new(&[1, 2]));
        c.push(Node::new(&[2, 3]));
        c.push(Node::new(&[3, 4]));
        c.push(Node::new(&[4, 5]));

        assert_eq!(c.len(), 5);
        assert_eq!(c[0].key(), &[0, 1]);
        assert_eq!(c[1].key(), &[1, 2]);
        assert_eq!(c[2].key(), &[2, 3]);
        assert_eq!(c[3].key(), &[3, 4]);
        assert_eq!(c[4].key(), &[4, 5]);

        // Remove first
        assert_eq!(c.remove(0).key(), &[0, 1]);
        assert_eq!(c.len(), 4);
        assert_eq!(c[0].key(), &[1, 2]);
        assert_eq!(c[1].key(), &[2, 3]);
        assert_eq!(c[2].key(), &[3, 4]);
        assert_eq!(c[3].key(), &[4, 5]);

        // Remove last
        assert_eq!(c.remove(3).key(), &[4, 5]);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].key(), &[1, 2]);
        assert_eq!(c[1].key(), &[2, 3]);
        assert_eq!(c[2].key(), &[3, 4]);

        // Remove mid
        assert_eq!(c.remove(1).key(), &[2, 3]);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].key(), &[1, 2]);
        assert_eq!(c[1].key(), &[3, 4]);

        // Remove mid
        assert_eq!(c.remove(1).key(), &[3, 4]);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].key(), &[1, 2]);
    }

    #[test]
    #[should_panic]
    fn test_remove_invalid_offset() {
        let mut c: Children<()> = Children::new(Node::new(&[0, 1]));
        c.push(Node::new(&[1, 2]));

        assert_eq!(c.len(), 2);
        assert_eq!(c[0].key(), &[0, 1]);
        assert_eq!(c[1].key(), &[1, 2]);

        c.remove(2);
    }

    #[test]
    #[should_panic]
    fn test_remove_last_item() {
        let mut c: Children<()> = Children::new(Node::new(&[0, 1]));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].key(), &[0, 1]);

        c.remove(0);
    }
}

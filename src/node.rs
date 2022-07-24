use std::alloc::{alloc, dealloc, realloc, Layout};
use std::marker::PhantomData;
use std::mem::size_of;
use std::ptr;
use std::slice::{from_raw_parts, from_raw_parts_mut};

use bitflags::bitflags;

use crate::longest_common_prefix;

bitflags! {
    struct Flags: u8 {
        const VALUE_ALLOCATED = 0b0000_0001;
        const VALUE_INITIALIZED = 0b0000_0010;
        const HAS_CHILDREN = 0b0000_0100;
    }
}

#[derive(Debug)]
pub struct Node<T> {
    /// Layout:
    ///     - flags: u8
    ///     - key_len: u8
    ///     - key: [u8; key_len]
    ///     - value: size_of<T> (optional - Flags::VALUE_ALLOCATED)
    ///     - children_count: u8 (optional - Flags::HAS_CHILDREN)
    ///     - children: [Node<T>; children_count] (optional - Flags::HAS_CHILDREN)
    data: ptr::NonNull<u8>,
    _phantom: PhantomData<T>,
}

unsafe impl<T> Send for Node<T> {}
unsafe impl<T> Sync for Node<T> {}

impl<T> Node<T> {
    #[inline]
    pub(crate) fn new(key: &[u8]) -> Self {
        assert!(key.len() < 256, "Key length must be < 256");
        // Allocate
        let flags = Flags::empty();
        let data = Self::alloc(flags, key.len());
        // Write key
        unsafe {
            ptr::write(data.as_ptr().add(1), key.len() as u8);
            ptr::copy(key.as_ptr(), data.as_ptr().add(2), key.len());
        }

        Node {
            data,
            _phantom: PhantomData::default(),
        }
    }

    #[inline]
    fn new_with_value(key: &[u8], value: T) -> Self {
        assert!(key.len() < 256, "Key length must be < 256");
        // Allocate
        let mut flags = Flags::empty();
        flags.set(Flags::VALUE_ALLOCATED, true);
        flags.set(Flags::VALUE_INITIALIZED, true);
        let data = Self::alloc(flags, key.len());
        unsafe {
            // Write key
            ptr::write(data.as_ptr().add(1), key.len() as u8);
            ptr::copy(key.as_ptr(), data.as_ptr().add(2), key.len());
            // Write value
            ptr::write(data.as_ptr().add(2 + key.len()) as *mut T, value);
        }

        Node {
            data,
            _phantom: PhantomData::default(),
        }
    }

    // Exposed API
    #[inline(always)]
    pub(crate) fn key(&self) -> &[u8] {
        unsafe { from_raw_parts(self.key_ptr(), self.key_len()) }
    }

    #[inline(always)]
    pub(crate) fn value(&self) -> Option<&T> {
        if self.flags().contains(Flags::VALUE_INITIALIZED) {
            return unsafe { Some(&*self.value_ptr()) };
        }
        None
    }

    #[inline(always)]
    pub(crate) fn value_mut(&mut self) -> Option<&mut T> {
        if self.flags().contains(Flags::VALUE_INITIALIZED) {
            return unsafe { Some(&mut *self.value_ptr()) };
        }
        None
    }

    #[inline]
    pub(crate) fn take_value(&mut self) -> Option<T> {
        if self.flags().contains(Flags::VALUE_INITIALIZED) {
            self.set_flags(Flags::VALUE_INITIALIZED, false);
            Some(unsafe { ptr::read(self.value_ptr()) })
        } else {
            None
        }
    }

    #[inline(always)]
    pub(crate) fn children(&self) -> &[Node<T>] {
        if !self.flags().contains(Flags::HAS_CHILDREN) {
            return &[];
        }

        unsafe { from_raw_parts(self.children_ptr(), *self.children_len_ptr() as usize + 1) }
    }

    #[inline(always)]
    pub(crate) fn children_mut(&mut self) -> &mut [Node<T>] {
        if !self.flags().contains(Flags::HAS_CHILDREN) {
            return &mut [];
        }

        unsafe { from_raw_parts_mut(self.children_ptr(), *self.children_len_ptr() as usize + 1) }
    }

    /// Returns an iterator over node's children that deallocates this node's
    /// children after iteration.
    #[inline(always)]
    pub(crate) fn take_children(&mut self) -> TakeChildren<'_, T> {
        TakeChildren::new(self)
    }

    #[inline]
    pub(crate) fn insert(&mut self, key: &[u8], value: T) -> Option<T> {
        if key.is_empty() {
            return self.replace_value(value);
        }

        let (prefix_len, child_idx) = longest_common_prefix(self.children(), key);
        if prefix_len == 0 {
            // No child shares a prefix with the key
            if key.len() > 255 {
                // Key length is greater than 255. Insert a child with key len == 255 and insert
                // the remainder into it.
                self.insert_child(child_idx, Node::new(&key[..255]));
                return self.children_mut()[child_idx].insert(&key[255..], value);
            }
            // Insert a new child at child_idx offset
            self.insert_child(child_idx, Node::new_with_value(key, value));
            return None;
        }

        // Some child shares a prefix with the key
        let children = self.children_mut();
        if prefix_len == children[child_idx].key().len() {
            // Child's key is a prefix of the inserted key
            return children[child_idx].insert(&key[prefix_len..], value);
        }

        // Only a portion of child's key shares prefix with the inserted key
        Self::split_child(children, child_idx, prefix_len, key, value)
    }

    #[inline]
    pub(crate) fn remove(&mut self, key: &[u8]) -> Option<T> {
        if key.is_empty() {
            let removed = self.take_value();
            if (self.children().len() == 1)
                && (self.key_len() + self.children()[0].key().len() < 256)
            {
                // If the node has only one child and len(node key + child key) < 256
                // then we can merge the nodes together.
                let mut child_node = self.remove_child(0);
                self.extend_key(child_node.key());
                if let Some(v) = child_node.take_value() {
                    self.replace_value(v);
                }
                self.move_children(child_node);
            }
            return removed;
        }

        match self.select_next_child(key) {
            Some((prefix_len, child_idx)) => {
                let children = self.children_mut();
                let removed = children[child_idx].remove(&key[prefix_len..]);

                if removed.is_some() && children[child_idx].is_empty() {
                    self.remove_child(child_idx);
                }

                removed
            }
            None => None,
        }
    }

    #[inline]
    pub(crate) fn get(&self, key: &[u8]) -> Option<&T> {
        if key.is_empty() {
            return self.value();
        }

        match self.select_next_child(key) {
            Some((prefix_len, child_idx)) => self.children()[child_idx].get(&key[prefix_len..]),
            None => None,
        }
    }

    #[inline]
    pub(crate) fn get_mut(&mut self, key: &[u8]) -> Option<&mut T> {
        if key.is_empty() {
            return self.value_mut();
        }

        match self.select_next_child(key) {
            Some((prefix_len, child_idx)) => {
                self.children_mut()[child_idx].get_mut(&key[prefix_len..])
            }
            None => None,
        }
    }

    /// Returns a reference to a node which matches a given prefix.
    #[inline]
    pub(crate) fn find_prefix(&self, prefix: &[u8]) -> Option<(usize, &Node<T>)> {
        let (prefix_len, child_idx) = longest_common_prefix(self.children(), prefix);
        if prefix_len == 0 {
            // No child matches the prefix
            return None;
        }

        let suffix = &prefix[prefix_len..];
        let child = &self.children()[child_idx];
        if suffix.is_empty() {
            return Some((0, child));
        }
        child
            .find_prefix(&prefix[prefix_len..])
            .map(|(k, n)| (prefix_len + k, n))
    }

    /// Returns a mutable reference to a node which matches a given prefix.
    /// The mutable reference should only be used to get a mutable reference
    /// to the value and not to mutate the node's structure (e.g. by calling
    /// insert or remove).
    #[inline]
    pub(crate) fn find_prefix_mut(&mut self, prefix: &[u8]) -> Option<(usize, &mut Node<T>)> {
        let (prefix_len, child_idx) = longest_common_prefix(self.children(), prefix);
        if prefix_len == 0 {
            // No child matches the prefix
            return None;
        }

        let suffix = &prefix[prefix_len..];
        let child = &mut self.children_mut()[child_idx];
        if suffix.is_empty() {
            return Some((0, child));
        }
        child
            .find_prefix_mut(&prefix[prefix_len..])
            .map(|(k, n)| (prefix_len + k, n))
    }

    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.value().is_none() && self.children().is_empty()
    }

    // Memory management
    #[inline(always)]
    fn create_layout(flags: Flags, key_len: usize, children_count: usize) -> Layout {
        let mut layout = Layout::array::<u8>(key_len + 2).expect("invalid layout");

        if flags.contains(Flags::VALUE_ALLOCATED) {
            layout = layout.extend(Layout::new::<T>()).expect("invalid layout").0;
        }

        if flags.contains(Flags::HAS_CHILDREN) {
            layout = layout
                .extend(Layout::new::<u8>())
                .expect("invalid layout")
                .0;
            layout = layout
                .extend(Layout::array::<Node<T>>(children_count).expect("invalid layout"))
                .expect("invalid layout")
                .0;
        }

        layout.pad_to_align()
    }

    #[inline(always)]
    fn flags(&self) -> Flags {
        unsafe { Flags::from_bits(*self.data.as_ptr()).expect("invalid flags") }
    }

    #[inline(always)]
    fn set_flags(&self, other: Flags, value: bool) {
        let mut flags = self.flags();
        flags.set(other, value);
        unsafe { ptr::write(self.data.as_ptr(), flags.bits()) }
    }

    #[inline(always)]
    fn curr_layout(&self) -> Layout {
        Self::create_layout(self.flags(), self.key_len(), self.children().len())
    }

    #[inline(always)]
    fn alloc(flags: Flags, key_len: usize) -> ptr::NonNull<u8> {
        let layout = Self::create_layout(flags, key_len, 0);
        let ptr = unsafe { alloc(layout) };
        let data = ptr::NonNull::new(ptr).expect("allocation failed");
        unsafe {
            ptr::write(data.as_ptr(), flags.bits());
        }
        data
    }

    #[inline(always)]
    fn realloc(
        &mut self,
        old_layout: Layout,
        new_flags: Flags,
        key_len: usize,
        children_count: usize,
    ) {
        let new_layout = Self::create_layout(new_flags, key_len, children_count);
        let new_ptr = unsafe { realloc(self.data.as_ptr(), old_layout, new_layout.size()) };
        self.data = ptr::NonNull::new(new_ptr).expect("allocation failed");
        unsafe {
            ptr::write(self.data.as_ptr(), new_flags.bits());
        }
    }

    #[inline(always)]
    fn data_size(&self) -> usize {
        let mut size = 2 + self.key_len();
        if self.flags().contains(Flags::VALUE_ALLOCATED) {
            size += size_of::<T>();
        }
        if self.flags().contains(Flags::HAS_CHILDREN) {
            size += 1 + self.children().len() * size_of::<Node<T>>();
        }
        size
    }

    #[inline(always)]
    fn key_len(&self) -> usize {
        unsafe { *self.key_len_ptr() as usize }
    }

    #[inline(always)]
    unsafe fn key_len_ptr(&self) -> *mut u8 {
        self.data.as_ptr().add(1)
    }

    #[inline(always)]
    unsafe fn key_ptr(&self) -> *mut u8 {
        self.data.as_ptr().add(2)
    }

    #[inline(always)]
    unsafe fn value_ptr(&self) -> *mut T {
        assert!(self.flags().contains(Flags::VALUE_ALLOCATED));
        self.data.as_ptr().add(2 + self.key_len()) as *mut T
    }

    #[inline(always)]
    unsafe fn children_len_ptr(&self) -> *mut u8 {
        assert!(self.flags().contains(Flags::HAS_CHILDREN));
        let mut ptr = self.data.as_ptr().add(2 + self.key_len());
        if self.flags().contains(Flags::VALUE_ALLOCATED) {
            ptr = ptr.add(size_of::<T>());
        }
        ptr
    }

    #[inline(always)]
    unsafe fn children_ptr(&self) -> *mut Node<T> {
        self.children_len_ptr().add(1) as *mut Node<T>
    }

    // Key access methods
    #[inline]
    fn strip_key_prefix(&mut self, prefix_len: usize) {
        assert!(prefix_len <= self.key_len(), "Invalid prefix len");

        let old_layout = self.curr_layout();
        let new_key_len = self.key_len() - prefix_len;
        let copy_size = self.data_size() - prefix_len - 2;
        unsafe {
            // Write new length
            ptr::write(self.key_len_ptr(), new_key_len as u8);
            // Shift left
            ptr::copy(self.key_ptr().add(prefix_len), self.key_ptr(), copy_size)
        }
        self.realloc(old_layout, self.flags(), new_key_len, self.children().len());
    }

    #[inline]
    fn extend_key(&mut self, suffix: &[u8]) {
        let new_key_len = self.key_len() + suffix.len();
        assert!(new_key_len < 256, "Cannot extend key. Suffix is too long.");

        self.realloc(
            self.curr_layout(),
            self.flags(),
            new_key_len,
            self.children().len(),
        );

        unsafe {
            // Shift value + children right
            ptr::copy(
                self.key_ptr().add(self.key_len()),
                self.key_ptr().add(new_key_len),
                self.data_size() - self.key_len() - 2,
            );
            // Extend key
            ptr::copy(
                suffix.as_ptr(),
                self.key_ptr().add(self.key_len()),
                suffix.len(),
            );
            // Write new key length
            ptr::write(self.key_len_ptr(), new_key_len as u8);
        }
    }

    // Value access methods
    #[inline]
    fn replace_value(&mut self, value: T) -> Option<T> {
        if !self.flags().contains(Flags::VALUE_ALLOCATED) {
            // Allocate value if it's not allocated
            let children_count = self.children().len();

            let mut new_flags = self.flags();
            new_flags.set(Flags::VALUE_ALLOCATED, true);
            self.realloc(
                self.curr_layout(),
                new_flags,
                self.key_len(),
                children_count,
            );

            if self.flags().contains(Flags::HAS_CHILDREN) {
                // Move children to the right to make space for value
                unsafe {
                    ptr::copy(
                        self.value_ptr() as *mut u8,
                        self.children_len_ptr(),
                        1 + children_count * size_of::<Node<T>>(),
                    )
                }
            }
        }

        if self.flags().contains(Flags::VALUE_INITIALIZED) {
            // Replace old value
            Some(unsafe { std::mem::replace::<T>(&mut *self.value_ptr(), value) })
        } else {
            // Write value and set initialized flag
            unsafe {
                ptr::write(self.value_ptr(), value);
            }
            self.set_flags(Flags::VALUE_INITIALIZED, true);
            None
        }
    }

    // Children access methods
    #[inline(always)]
    fn select_next_child(&self, key: &[u8]) -> Option<(usize, usize)> {
        let (prefix_len, child_idx) = longest_common_prefix(self.children(), key);
        if (prefix_len == 0) || (prefix_len < self.children()[child_idx].key().len()) {
            // There is no or only a partial match in which case the
            // key does not exist in the tree.
            return None;
        }
        Some((prefix_len, child_idx))
    }

    #[inline]
    fn insert_child(&mut self, idx: usize, node: Node<T>) {
        assert!(idx <= self.children().len(), "invalid offset");
        assert!(self.children().len() < 256, "Children array is full");

        if !self.flags().contains(Flags::HAS_CHILDREN) {
            // Allocate children
            let mut new_flags = self.flags();
            new_flags.set(Flags::HAS_CHILDREN, true);
            self.realloc(self.curr_layout(), new_flags, self.key_len(), 1);
            // Insert at 0th position
            unsafe {
                ptr::write(self.children_len_ptr(), 0);
                ptr::write(self.children_ptr(), node);
            }
        } else {
            // Grow
            self.realloc(
                self.curr_layout(),
                self.flags(),
                self.key_len(),
                self.children().len() + 1,
            );

            // Insert
            unsafe {
                // Shift children to the right
                let node_ptr = self.children_ptr();
                ptr::copy(
                    node_ptr.add(idx),
                    node_ptr.add(idx + 1),
                    self.children().len() - idx,
                );
                // Write new node
                ptr::write(node_ptr.add(idx), node);
                // Increment count
                ptr::write(self.children_len_ptr(), *self.children_len_ptr() + 1);
            }
        }
    }

    #[inline(always)]
    pub(super) fn push_child(&mut self, node: Node<T>) {
        self.insert_child(self.children().len(), node);
    }

    #[inline]
    fn remove_child(&mut self, idx: usize) -> Node<T> {
        assert!(idx < self.children().len(), "invalid offset");

        if self.flags().contains(Flags::HAS_CHILDREN) {
            let removed = unsafe { ptr::read(self.children_ptr().add(idx)) };
            if self.children().len() == 1 {
                // Deallocate children
                let mut new_flags = self.flags();
                new_flags.set(Flags::HAS_CHILDREN, false);
                self.realloc(self.curr_layout(), new_flags, self.key_len(), 0);
            } else {
                assert!(self.children().len() > 1);
                unsafe {
                    // Decrement count
                    ptr::write(self.children_len_ptr(), *self.children_len_ptr() - 1);
                    // Shift children to the left
                    let node_ptr = self.children_ptr();
                    ptr::copy(
                        node_ptr.add(idx + 1),
                        node_ptr.add(idx),
                        self.children().len() - idx,
                    );
                }
                // Shrink
                self.realloc(
                    self.curr_layout(),
                    self.flags(),
                    self.key_len(),
                    self.children().len(),
                );
            }
            removed
        } else {
            panic!("Cannot remove child. Node has not children.");
        }
    }

    #[inline]
    fn move_children(&mut self, mut src_node: Node<T>) {
        assert_eq!(
            self.children().len(),
            0,
            "Cannot move children to a node with children"
        );
        let src_count = src_node.children().len();
        if src_count == 0 {
            // Nothing to move
            return;
        }

        // Allocate children
        let mut new_flags = self.flags();
        new_flags.set(Flags::HAS_CHILDREN, true);
        self.realloc(self.curr_layout(), new_flags, self.key_len(), src_count);

        // Copy from src node to self
        unsafe {
            ptr::copy(
                src_node.children_len_ptr(),
                self.children_len_ptr(),
                1 + src_count * size_of::<Node<T>>(),
            );
        }

        // Deallocate src node children
        src_node.dealloc_children();
    }

    fn dealloc_children(&mut self) {
        let mut flags = self.flags();
        if !flags.contains(Flags::HAS_CHILDREN) {
            panic!("Node has no children");
        }

        flags.set(Flags::HAS_CHILDREN, false);
        self.realloc(self.curr_layout(), flags, self.key_len(), 0);
    }

    #[inline(always)]
    fn split_child(
        children: &mut [Node<T>],
        idx: usize,
        prefix_len: usize,
        key: &[u8],
        value: T,
    ) -> Option<T> {
        // Replace node with new (uninitialized) node
        let mut old = std::mem::replace(&mut children[idx], Node::new(&key[..prefix_len]));
        // Update old node's key
        old.strip_key_prefix(prefix_len);
        // Initialize new node's children with the old node
        children[idx].push_child(old);
        // Insert into the new node
        children[idx].insert(&key[prefix_len..], value)
    }
}

impl<T> Drop for Node<T> {
    fn drop(&mut self) {
        if self.flags().contains(Flags::VALUE_INITIALIZED) {
            // Drop value
            let _value = unsafe { ptr::read(self.value_ptr()) };
        }
        if self.flags().contains(Flags::HAS_CHILDREN) {
            // Drop children
            unsafe {
                let node_ptr = self.children_ptr();
                for i in 0..self.children().len() {
                    let _node = ptr::read(node_ptr.add(i));
                }
            }
        }
        // Deallocate
        unsafe {
            dealloc(self.data.as_ptr(), self.curr_layout());
        }
    }
}

pub(crate) struct TakeChildren<'a, T> {
    node: &'a mut Node<T>,
    start_idx: usize,
    end_idx: usize,
}

impl<'a, T> TakeChildren<'a, T> {
    fn new(node: &'a mut Node<T>) -> Self {
        let children_count = node.children().len();
        TakeChildren {
            node,
            start_idx: 0,
            end_idx: children_count,
        }
    }

    fn read_child(&self, idx: usize) -> Node<T> {
        unsafe { ptr::read(self.node.children_ptr().add(idx)) }
    }
}

impl<'a, T> Drop for TakeChildren<'a, T> {
    fn drop(&mut self) {
        if self.node.flags().contains(Flags::HAS_CHILDREN) {
            for idx in self.start_idx..self.end_idx {
                let _child = self.read_child(idx);
            }
            self.node.dealloc_children();
        }
    }
}

impl<'a, T> Iterator for TakeChildren<'a, T> {
    type Item = Node<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.node.flags().contains(Flags::HAS_CHILDREN) {
            return None;
        }

        let child = self.read_child(self.start_idx);
        self.start_idx += 1;

        if self.start_idx == self.end_idx {
            self.node.dealloc_children();
        }

        Some(child)
    }
}

impl<'a, T> DoubleEndedIterator for TakeChildren<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if !self.node.flags().contains(Flags::HAS_CHILDREN) {
            return None;
        }

        self.end_idx -= 1;
        let child = self.read_child(self.end_idx);

        if self.end_idx == self.start_idx {
            self.node.dealloc_children();
        }

        Some(child)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;
    use std::rc::Rc;

    struct NodeIter<'a, V> {
        stack: Vec<&'a Node<V>>,
    }

    impl<'a, V> NodeIter<'a, V> {
        fn new(root: &'a Node<V>) -> Self {
            NodeIter { stack: vec![root] }
        }
    }

    impl<'a, V> Iterator for NodeIter<'a, V> {
        type Item = &'a Node<V>;

        fn next(&mut self) -> Option<Self::Item> {
            match self.stack.pop() {
                Some(node) => {
                    for child in node.children().iter().rev() {
                        self.stack.push(child);
                    }
                    Some(node)
                }
                None => None,
            }
        }
    }

    #[test]
    fn test_new() {
        let node: Node<()> = Node::new(&[1, 2, 3]);
        assert_eq!(node.key(), &[1, 2, 3]);
        assert!(!node.flags().contains(Flags::VALUE_ALLOCATED));
        assert!(!node.flags().contains(Flags::VALUE_INITIALIZED));
        assert!(!node.flags().contains(Flags::HAS_CHILDREN));
    }

    #[test]
    fn test_modify_value() {
        let mut node: Node<u64> = Node::new(&[1, 2, 3]);
        node.push_child(Node::new(&[1]));
        node.push_child(Node::new(&[2]));

        assert_eq!(node.key(), &[1, 2, 3]);
        assert_eq!(node.value(), None);
        assert!(!node.flags().contains(Flags::VALUE_ALLOCATED));
        assert!(!node.flags().contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1]);
        assert_eq!(node.children()[1].key(), &[2]);

        assert_eq!(node.replace_value(123), None);
        assert_eq!(node.value(), Some(&123));
        assert!(node.flags().contains(Flags::VALUE_ALLOCATED));
        assert!(node.flags().contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1]);
        assert_eq!(node.children()[1].key(), &[2]);

        assert_eq!(node.replace_value(456), Some(123));
        assert_eq!(node.value(), Some(&456));
        assert!(node.flags().contains(Flags::VALUE_ALLOCATED));
        assert!(node.flags().contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1]);
        assert_eq!(node.children()[1].key(), &[2]);

        assert_eq!(node.take_value(), Some(456));
        assert_eq!(node.value(), None);
        assert!(node.flags().contains(Flags::VALUE_ALLOCATED));
        assert!(!node.flags().contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1]);
        assert_eq!(node.children()[1].key(), &[2]);
    }

    #[test]
    fn test_modify_key() {
        let mut node: Node<u64> = Node::new_with_value(&[1, 2, 3, 4, 5], 123);
        node.push_child(Node::new(&[1]));
        node.push_child(Node::new(&[2]));

        assert_eq!(node.key(), &[1, 2, 3, 4, 5]);
        assert_eq!(node.value(), Some(&123));
        assert!(node.flags().contains(Flags::VALUE_ALLOCATED));
        assert!(node.flags().contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1]);
        assert_eq!(node.children()[1].key(), &[2]);

        node.strip_key_prefix(2);
        assert_eq!(node.key(), &[3, 4, 5]);
        assert_eq!(node.value(), Some(&123));
        assert!(node.flags().contains(Flags::VALUE_ALLOCATED));
        assert!(node.flags().contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1]);
        assert_eq!(node.children()[1].key(), &[2]);

        node.extend_key(&[6, 7, 8]);
        assert_eq!(node.key(), &[3, 4, 5, 6, 7, 8]);
        assert_eq!(node.value(), Some(&123));
        assert!(node.flags().contains(Flags::VALUE_ALLOCATED));
        assert!(node.flags().contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1]);
        assert_eq!(node.children()[1].key(), &[2]);
    }

    #[test]
    fn test_insert() {
        let mut node = Node::new("".as_bytes());
        assert_eq!(NodeIter::new(&node).count(), 1);

        node.insert("abc;0".as_bytes(), 1);
        assert_eq!(NodeIter::new(&node).count(), 2);

        node.insert("abb;0".as_bytes(), 2);
        assert_eq!(NodeIter::new(&node).count(), 4);

        node.insert("ab".as_bytes(), 3);
        assert_eq!(NodeIter::new(&node).count(), 4);

        node.insert("abd".as_bytes(), 4);
        assert_eq!(NodeIter::new(&node).count(), 5);

        let mut nodes_map = BTreeMap::new();
        for n in NodeIter::new(&node) {
            nodes_map.insert(n.key(), n);
        }
        assert_eq!(nodes_map.len(), 5);

        assert_eq!(nodes_map.get(node.key()).unwrap().children().len(), 1);
        assert_eq!(nodes_map.get(node.key()).unwrap().value(), None);

        assert_eq!(nodes_map.get("ab".as_bytes()).unwrap().children().len(), 3);
        assert_eq!(nodes_map.get("ab".as_bytes()).unwrap().value(), Some(&3));

        assert_eq!(nodes_map.get("d".as_bytes()).unwrap().children().len(), 0);
        assert_eq!(nodes_map.get("d".as_bytes()).unwrap().value(), Some(&4));

        assert_eq!(nodes_map.get("c;0".as_bytes()).unwrap().children().len(), 0);
        assert_eq!(nodes_map.get("c;0".as_bytes()).unwrap().value(), Some(&1));

        assert_eq!(nodes_map.get("b;0".as_bytes()).unwrap().children().len(), 0);
        assert_eq!(nodes_map.get("b;0".as_bytes()).unwrap().value(), Some(&2));
    }

    #[test]
    fn test_insert_with_key_len_greater_than_255() {
        let mut node = Node::new("".as_bytes());
        assert_eq!(NodeIter::new(&node).count(), 1);

        let key_a = vec![0; 260];
        let mut key_b = key_a.clone();
        key_b.extend(&[1, 2, 3]);
        let mut key_c = key_a.clone();
        key_c.extend(&[4, 5, 6]);
        let key_d = vec![0; 520];
        let key_e = vec![1; 512];
        let key_f = vec![2; 510];

        node.insert(&key_a, 1);
        assert_eq!(NodeIter::new(&node).count(), 3);
        assert_eq!(node.get(&key_a), Some(&1));

        node.insert(&key_b, 2);
        assert_eq!(NodeIter::new(&node).count(), 4);
        assert_eq!(node.get(&key_b), Some(&2));

        node.insert(&key_c, 3);
        assert_eq!(NodeIter::new(&node).count(), 5);
        assert_eq!(node.get(&key_c), Some(&3));

        node.insert(&key_d, 4);
        assert_eq!(NodeIter::new(&node).count(), 7);
        assert_eq!(node.get(&key_d), Some(&4));

        node.insert(&key_e, 5);
        assert_eq!(NodeIter::new(&node).count(), 10);
        assert_eq!(node.get(&key_e), Some(&5));

        node.insert(&key_f, 6);
        assert_eq!(NodeIter::new(&node).count(), 12);
        assert_eq!(node.get(&key_f), Some(&6));
    }

    #[test]
    fn test_remove() {
        let mut node = Node::new(&[]);
        node.insert("hello".as_bytes(), 0);
        node.insert("hell".as_bytes(), 1);
        node.insert("hel".as_bytes(), 2);
        node.insert("h".as_bytes(), 3);

        assert_eq!(node.get("h".as_bytes()), Some(&3));
        assert_eq!(node.get("hel".as_bytes()), Some(&2));
        assert_eq!(node.get("hell".as_bytes()), Some(&1));
        assert_eq!(node.get("hello".as_bytes()), Some(&0));
        assert_eq!(NodeIter::new(&node).count(), 5);

        assert_eq!(node.remove("he".as_bytes()), None);
        assert_eq!(node.get("h".as_bytes()), Some(&3));
        assert_eq!(node.get("hel".as_bytes()), Some(&2));
        assert_eq!(node.get("hell".as_bytes()), Some(&1));
        assert_eq!(node.get("hello".as_bytes()), Some(&0));
        assert_eq!(NodeIter::new(&node).count(), 5);

        assert_eq!(node.remove("hell".as_bytes()), Some(1));
        assert_eq!(node.get("h".as_bytes()), Some(&3));
        assert_eq!(node.get("hel".as_bytes()), Some(&2));
        assert_eq!(node.get("hell".as_bytes()), None);
        assert_eq!(node.get("hello".as_bytes()), Some(&0));
        assert_eq!(NodeIter::new(&node).count(), 4);

        assert_eq!(node.remove("hel".as_bytes()), Some(2));
        assert_eq!(node.get("h".as_bytes()), Some(&3));
        assert_eq!(node.get("hel".as_bytes()), None);
        assert_eq!(node.get("hell".as_bytes()), None);
        assert_eq!(node.get("hello".as_bytes()), Some(&0));
        assert_eq!(NodeIter::new(&node).count(), 3);

        assert_eq!(node.remove("hello".as_bytes()), Some(0));
        assert_eq!(node.get("h".as_bytes()), Some(&3));
        assert_eq!(node.get("hel".as_bytes()), None);
        assert_eq!(node.get("hell".as_bytes()), None);
        assert_eq!(node.get("hello".as_bytes()), None);
        assert_eq!(NodeIter::new(&node).count(), 2);

        assert_eq!(node.remove("h".as_bytes()), Some(3));
        assert_eq!(node.get("h".as_bytes()), None);
        assert_eq!(node.get("hel".as_bytes()), None);
        assert_eq!(node.get("hell".as_bytes()), None);
        assert_eq!(node.get("hello".as_bytes()), None);
        assert_eq!(NodeIter::new(&node).count(), 1);

        node.insert(&vec![0; 512], 3);
        assert_eq!(NodeIter::new(&node).count(), 4);
        assert_eq!(node.get(&vec![0; 512]), Some(&3));

        assert_eq!(node.remove(&vec![0; 512]), Some(3));
        assert_eq!(NodeIter::new(&node).count(), 1);
    }

    // Children tests
    #[test]
    fn test_children_add() {
        let mut node: Node<()> = Node::new(&[]);
        assert!(!node.flags().contains(Flags::HAS_CHILDREN));

        // Push
        node.push_child(Node::new(&[]));
        assert!(node.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 1);
        assert_eq!(node.children()[0].key(), &[]);

        // Push
        node.push_child(Node::new(&[1, 2]));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[]);
        assert_eq!(node.children()[1].key(), &[1, 2]);

        // Insert last
        node.insert_child(2, Node::new(&[3, 4]));
        assert_eq!(node.children().len(), 3);
        assert_eq!(node.children()[0].key(), &[]);
        assert_eq!(node.children()[1].key(), &[1, 2]);
        assert_eq!(node.children()[2].key(), &[3, 4]);

        // Insert mid
        node.insert_child(2, Node::new(&[2, 3]));
        assert_eq!(node.children().len(), 4);
        assert_eq!(node.children()[0].key(), &[]);
        assert_eq!(node.children()[1].key(), &[1, 2]);
        assert_eq!(node.children()[2].key(), &[2, 3]);
        assert_eq!(node.children()[3].key(), &[3, 4]);

        // Insert first
        node.insert_child(0, Node::new(&[0, 0]));
        assert_eq!(node.children().len(), 5);
        assert_eq!(node.children()[0].key(), &[0, 0]);
        assert_eq!(node.children()[1].key(), &[]);
        assert_eq!(node.children()[2].key(), &[1, 2]);
        assert_eq!(node.children()[3].key(), &[2, 3]);
        assert_eq!(node.children()[4].key(), &[3, 4]);
    }

    #[test]
    fn test_children_push_full() {
        let mut node: Node<()> = Node::new(&[]);

        for i in 0..=255_u32 {
            node.push_child(Node::new(i.to_be_bytes().as_slice()));
        }

        assert_eq!(node.children().len(), 256);
        for i in 0..=255_u32 {
            assert_eq!(
                node.children()[i as usize].key(),
                i.to_be_bytes().as_slice()
            );
        }
    }

    #[test]
    #[should_panic]
    fn test_children_push_more_than_256_items() {
        let mut node: Node<()> = Node::new(&[]);

        for i in 0..=256_u32 {
            node.push_child(Node::new(i.to_be_bytes().as_slice()));
        }
    }

    #[test]
    fn test_children_remove() {
        let mut node: Node<()> = Node::new(&[]);
        node.push_child(Node::new(&[0, 1]));
        node.push_child(Node::new(&[1, 2]));
        node.push_child(Node::new(&[2, 3]));
        node.push_child(Node::new(&[3, 4]));
        node.push_child(Node::new(&[4, 5]));

        assert_eq!(node.children().len(), 5);
        assert!(node.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children()[0].key(), &[0, 1]);
        assert_eq!(node.children()[1].key(), &[1, 2]);
        assert_eq!(node.children()[2].key(), &[2, 3]);
        assert_eq!(node.children()[3].key(), &[3, 4]);
        assert_eq!(node.children()[4].key(), &[4, 5]);

        // Remove first
        assert_eq!(node.remove_child(0).key(), &[0, 1]);
        assert!(node.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 4);
        assert_eq!(node.children()[0].key(), &[1, 2]);
        assert_eq!(node.children()[1].key(), &[2, 3]);
        assert_eq!(node.children()[2].key(), &[3, 4]);
        assert_eq!(node.children()[3].key(), &[4, 5]);

        // Remove last
        assert_eq!(node.remove_child(3).key(), &[4, 5]);
        assert!(node.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 3);
        assert_eq!(node.children()[0].key(), &[1, 2]);
        assert_eq!(node.children()[1].key(), &[2, 3]);
        assert_eq!(node.children()[2].key(), &[3, 4]);

        // Remove mid
        assert_eq!(node.remove_child(1).key(), &[2, 3]);
        assert!(node.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1, 2]);
        assert_eq!(node.children()[1].key(), &[3, 4]);

        // Remove mid
        assert_eq!(node.remove_child(1).key(), &[3, 4]);
        assert!(node.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 1);
        assert_eq!(node.children()[0].key(), &[1, 2]);
    }

    #[test]
    #[should_panic]
    fn test_children_remove_invalid_offset() {
        let mut node: Node<()> = Node::new(&[]);
        node.push_child(Node::new(&[0, 1]));
        node.push_child(Node::new(&[1, 2]));

        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[0, 1]);
        assert_eq!(node.children()[1].key(), &[1, 2]);

        node.remove_child(2);
    }

    #[test]
    fn test_children_remove_last_item() {
        let mut node: Node<()> = Node::new(&[]);
        node.push_child(Node::new(&[0, 1]));
        assert_eq!(node.children().len(), 1);
        assert_eq!(node.children()[0].key(), &[0, 1]);

        assert_eq!(node.remove_child(0).key(), &[0, 1]);
        assert_eq!(node.children().len(), 0);
        assert!(!node.flags().contains(Flags::HAS_CHILDREN));
    }

    #[test]
    fn test_find_prefix() {
        let mut root: Node<u32> = Node::new(&[]);
        root.insert(b"foo;bar;1", 1);
        root.insert(b"foo;bar;2", 2);
        root.insert(b"foo;baz;1", 3);
        root.insert(b"foo;baz;2", 4);
        root.insert(b"bar;1", 5);
        root.insert(b"bar;2", 6);

        let prefix = b"foo;ba";
        for i in 1..=prefix.len() {
            let (prefix_len, n) = root.find_prefix(&prefix[..i]).unwrap();
            assert_eq!(n.key(), b"foo;ba");
            assert_eq!(n.children().len(), 2);
            assert_eq!(prefix_len, 0);
        }

        let prefix = b"bar;";
        for i in 1..=prefix.len() {
            let (prefix_len, n) = root.find_prefix(&prefix[..i]).unwrap();
            assert_eq!(n.key(), b"bar;");
            assert_eq!(n.children().len(), 2);
            assert_eq!(n.children()[0].value(), Some(&5));
            assert_eq!(n.children()[1].value(), Some(&6));
            assert_eq!(prefix_len, 0);
        }

        let (prefix_len, n) = root.find_prefix(b"foo;bar").unwrap();
        assert_eq!(n.key(), b"r;");
        assert_eq!(prefix_len, 6);
        assert_eq!(n.children().len(), 2);
        assert_eq!(n.children()[0].value(), Some(&1));
        assert_eq!(n.children()[1].value(), Some(&2));

        let (prefix_len, n) = root.find_prefix(b"foo;baz").unwrap();
        assert_eq!(n.key(), b"z;");
        assert_eq!(prefix_len, 6);
        assert_eq!(n.children().len(), 2);
        assert_eq!(n.children()[0].value(), Some(&3));
        assert_eq!(n.children()[1].value(), Some(&4));

        assert!(root.find_prefix(b"goo").is_none());
        assert!(root.find_prefix(b"fooa").is_none());
        assert!(root.find_prefix(b"foo;bag").is_none());
        assert!(root.find_prefix(b"baz").is_none());
        assert!(root.find_prefix(b"bz").is_none());
    }

    #[test]
    fn test_find_prefix_mut() {
        let mut root: Node<u32> = Node::new(&[]);
        root.insert(b"foo;bar;1", 1);
        root.insert(b"foo;bar;2", 2);
        root.insert(b"foo;baz;1", 3);
        root.insert(b"foo;baz;2", 4);
        root.insert(b"bar;1", 5);
        root.insert(b"bar;2", 6);

        let prefix = b"foo;ba";
        for i in 1..=prefix.len() {
            let (prefix_len, n) = root.find_prefix_mut(&prefix[..i]).unwrap();
            assert_eq!(n.key(), b"foo;ba");
            assert_eq!(n.children().len(), 2);
            assert_eq!(prefix_len, 0);
        }

        let prefix = b"bar;";
        for i in 1..=prefix.len() {
            let (prefix_len, n) = root.find_prefix_mut(&prefix[..i]).unwrap();
            assert_eq!(n.key(), b"bar;");
            assert_eq!(n.children().len(), 2);
            assert_eq!(n.children_mut()[0].value_mut(), Some(&mut 5));
            assert_eq!(n.children_mut()[1].value_mut(), Some(&mut 6));
            assert_eq!(prefix_len, 0);
        }

        let (prefix_len, n) = root.find_prefix_mut(b"foo;bar").unwrap();
        assert_eq!(n.key(), b"r;");
        assert_eq!(prefix_len, 6);
        assert_eq!(n.children().len(), 2);
        assert_eq!(n.children_mut()[0].value_mut(), Some(&mut 1));
        assert_eq!(n.children_mut()[1].value_mut(), Some(&mut 2));

        let (prefix_len, n) = root.find_prefix_mut(b"foo;baz").unwrap();
        assert_eq!(n.key(), b"z;");
        assert_eq!(prefix_len, 6);
        assert_eq!(n.children().len(), 2);
        assert_eq!(n.children_mut()[0].value_mut(), Some(&mut 3));
        assert_eq!(n.children_mut()[1].value_mut(), Some(&mut 4));

        assert!(root.find_prefix(b"goo").is_none());
        assert!(root.find_prefix(b"fooa").is_none());
        assert!(root.find_prefix(b"foo;bag").is_none());
        assert!(root.find_prefix(b"baz").is_none());
        assert!(root.find_prefix(b"bz").is_none());
    }

    #[test]
    fn test_take_children() {
        let rc = Rc::new(());
        let mut root: Node<Rc<()>> = Node::new(&[]);
        root.push_child(Node::new_with_value(b"a", rc.clone()));
        root.push_child(Node::new_with_value(b"b", rc.clone()));
        root.push_child(Node::new_with_value(b"c", rc.clone()));
        root.push_child(Node::new_with_value(b"d", rc.clone()));

        assert_eq!(root.children().len(), 4);
        assert!(root.flags().contains(Flags::HAS_CHILDREN));

        let mut children_it = root.take_children();
        assert_eq!(children_it.next().unwrap().key(), b"a");
        assert_eq!(children_it.next().unwrap().key(), b"b");
        assert_eq!(children_it.next().unwrap().key(), b"c");
        assert_eq!(children_it.next().unwrap().key(), b"d");
        drop(children_it);

        assert!(!root.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(Rc::strong_count(&rc), 1);
    }

    #[test]
    fn test_take_children_rev() {
        let rc = Rc::new(());
        let mut root: Node<Rc<()>> = Node::new(&[]);
        root.push_child(Node::new_with_value(b"a", rc.clone()));
        root.push_child(Node::new_with_value(b"b", rc.clone()));
        root.push_child(Node::new_with_value(b"c", rc.clone()));
        root.push_child(Node::new_with_value(b"d", rc.clone()));

        assert_eq!(root.children().len(), 4);
        assert!(root.flags().contains(Flags::HAS_CHILDREN));

        let mut children_it = root.take_children().rev();
        assert_eq!(children_it.next().unwrap().key(), b"d");
        assert_eq!(children_it.next().unwrap().key(), b"c");
        assert_eq!(children_it.next().unwrap().key(), b"b");
        assert_eq!(children_it.next().unwrap().key(), b"a");
        drop(children_it);

        assert!(!root.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(Rc::strong_count(&rc), 1);
    }

    #[test]
    fn test_take_children_front_back() {
        let rc = Rc::new(());
        let mut root: Node<Rc<()>> = Node::new(&[]);
        root.push_child(Node::new_with_value(b"a", rc.clone()));
        root.push_child(Node::new_with_value(b"b", rc.clone()));
        root.push_child(Node::new_with_value(b"c", rc.clone()));
        root.push_child(Node::new_with_value(b"d", rc.clone()));

        assert_eq!(root.children().len(), 4);
        assert!(root.flags().contains(Flags::HAS_CHILDREN));

        let mut children_it = root.take_children();
        assert_eq!(children_it.next().unwrap().key(), b"a");
        assert_eq!(children_it.next_back().unwrap().key(), b"d");
        assert_eq!(children_it.next().unwrap().key(), b"b");
        assert_eq!(children_it.next_back().unwrap().key(), b"c");
        assert!(children_it.next().is_none());
        assert!(children_it.next_back().is_none());
        drop(children_it);

        assert!(!root.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(Rc::strong_count(&rc), 1);
    }

    #[test]
    fn test_take_children_unfinished() {
        let rc = Rc::new(());
        let mut root: Node<Rc<()>> = Node::new(&[]);
        root.push_child(Node::new_with_value(b"a", rc.clone()));
        root.push_child(Node::new_with_value(b"b", rc.clone()));
        root.push_child(Node::new_with_value(b"c", rc.clone()));
        root.push_child(Node::new_with_value(b"d", rc.clone()));

        assert_eq!(Rc::strong_count(&rc), 5);

        assert_eq!(root.children().len(), 4);
        assert!(root.flags().contains(Flags::HAS_CHILDREN));

        let mut children_it = root.take_children();
        assert_eq!(children_it.next().unwrap().key(), b"a");
        assert_eq!(children_it.next().unwrap().key(), b"b");
        drop(children_it);

        assert!(!root.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(Rc::strong_count(&rc), 1);
    }

    #[test]
    fn test_take_children_rev_unfinished() {
        let rc = Rc::new(());
        let mut root: Node<Rc<()>> = Node::new(&[]);
        root.push_child(Node::new_with_value(b"a", rc.clone()));
        root.push_child(Node::new_with_value(b"b", rc.clone()));
        root.push_child(Node::new_with_value(b"c", rc.clone()));
        root.push_child(Node::new_with_value(b"d", rc.clone()));

        assert_eq!(root.children().len(), 4);
        assert!(root.flags().contains(Flags::HAS_CHILDREN));

        let mut children_it = root.take_children().rev();
        assert_eq!(children_it.next().unwrap().key(), b"d");
        assert_eq!(children_it.next().unwrap().key(), b"c");
        drop(children_it);

        assert!(!root.flags().contains(Flags::HAS_CHILDREN));
        assert_eq!(Rc::strong_count(&rc), 1);
    }

    #[test]
    fn test_take_children_with_no_children() {
        let mut root: Node<u32> = Node::new(&[]);

        assert_eq!(root.children().len(), 0);
        assert!(!root.flags().contains(Flags::HAS_CHILDREN));

        let mut children_it = root.take_children().rev();
        assert!(children_it.next().is_none());
    }
}

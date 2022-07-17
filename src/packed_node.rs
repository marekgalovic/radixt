use std::alloc::{alloc, dealloc, realloc, Layout};
use std::marker::PhantomData;
use std::mem::size_of;
use std::ptr;

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
pub(crate) struct Node<T> {
    /// Node flags
    flags: Flags,
    /// Layout:
    ///     - key_len: u8
    ///     - key: [u8; key_len]
    ///     - value: size_of<T> (optional - Flags::VALUE_ALLOCATED)
    ///     - children_count: u8 (optional - Flags::HAS_CHILDREN)
    ///     - children: [Node<T>; children_count] (optional - Flags::HAS_CHILDREN)
    data: ptr::NonNull<u8>,
    _phantom: PhantomData<T>,
}

impl<T> Node<T> {
    #[inline]
    pub(crate) fn new(key: &[u8]) -> Self {
        assert!(key.len() < 256, "Key length must be < 256");
        // Allocate
        let flags = Flags::empty();
        let data = Self::alloc(flags, key.len());
        // Write key
        unsafe {
            ptr::write(data.as_ptr(), key.len() as u8);
            ptr::copy(key.as_ptr(), data.as_ptr().add(1), key.len());
        }

        Node {
            flags,
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
        let data = Self::alloc(flags, key.len());
        unsafe {
            // Write key
            ptr::write(data.as_ptr(), key.len() as u8);
            ptr::copy(key.as_ptr(), data.as_ptr().add(1), key.len());
            // Write value
            ptr::write(data.as_ptr().add(1 + key.len()) as *mut T, value);
        }
        flags.set(Flags::VALUE_INITIALIZED, true);

        Node {
            flags,
            data,
            _phantom: PhantomData::default(),
        }
    }

    // Exposed API
    #[inline(always)]
    pub(crate) fn key(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.key_ptr(), self.key_len()) }
    }

    #[inline(always)]
    pub(crate) fn value(&self) -> Option<&T> {
        if self.flags.contains(Flags::VALUE_INITIALIZED) {
            return unsafe { Some(&*self.value_ptr()) };
        }
        None
    }

    #[inline(always)]
    pub(crate) fn children(&self) -> &[Node<T>] {
        if !self.flags.contains(Flags::HAS_CHILDREN) {
            return &[];
        }

        unsafe {
            std::slice::from_raw_parts(self.children_ptr(), *self.children_len_ptr() as usize + 1)
        }
    }

    #[inline]
    pub(crate) fn insert(&mut self, key: &[u8], value: T) -> Option<T> {
        if key.is_empty() {
            return self.replace_value(value);
        }

        let (prefix_len, child_idx) = longest_common_prefix(self.children(), key);
        if prefix_len == 0 {
            // No child shares a prefix with the key
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

    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.value().is_none() && self.children().is_empty()
    }

    // Memory management
    #[inline(always)]
    fn create_layout(flags: Flags, key_len: usize, children_count: usize) -> Layout {
        let mut layout = Layout::array::<u8>(key_len + 1).expect("invalid layout");

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
    fn curr_layout(&self) -> Layout {
        Self::create_layout(self.flags, self.key_len(), self.children().len())
    }

    #[inline(always)]
    fn alloc(flags: Flags, key_len: usize) -> ptr::NonNull<u8> {
        let layout = Self::create_layout(flags, key_len, 0);
        let ptr = unsafe { alloc(layout) };
        ptr::NonNull::new(ptr).expect("allocation failed")
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
        self.flags = new_flags;
    }

    #[inline(always)]
    fn data_size(&self) -> usize {
        let mut size = 1 + self.key_len();
        if self.flags.contains(Flags::VALUE_ALLOCATED) {
            size += size_of::<T>();
        }
        if self.flags.contains(Flags::HAS_CHILDREN) {
            size += 1 + self.children().len() * size_of::<Node<T>>();
        }
        size
    }

    #[inline(always)]
    fn key_len(&self) -> usize {
        unsafe { *self.data.as_ptr() as usize }
    }

    #[inline(always)]
    unsafe fn key_len_ptr(&self) -> *mut u8 {
        self.data.as_ptr()
    }

    #[inline(always)]
    unsafe fn key_ptr(&self) -> *mut u8 {
        self.data.as_ptr().add(1)
    }

    #[inline(always)]
    unsafe fn value_ptr(&self) -> *mut T {
        assert!(self.flags.contains(Flags::VALUE_ALLOCATED));
        self.data.as_ptr().add(1 + self.key_len()) as *mut T
    }

    #[inline(always)]
    unsafe fn children_len_ptr(&self) -> *mut u8 {
        assert!(self.flags.contains(Flags::HAS_CHILDREN));
        let mut ptr = self.data.as_ptr().add(1 + self.key_len());
        if self.flags.contains(Flags::VALUE_ALLOCATED) {
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
        let copy_size = self.data_size() - prefix_len - 1;
        // Shift left
        unsafe {
            ptr::write(self.key_len_ptr(), new_key_len as u8);
            ptr::copy(
                self.data.as_ptr().add(1 + prefix_len),
                self.key_ptr(),
                copy_size,
            )
        }
        self.realloc(old_layout, self.flags, new_key_len, self.children().len());
    }

    #[inline]
    fn extend_key(&mut self, suffix: &[u8]) {
        let new_key_len = self.key_len() + suffix.len();
        assert!(new_key_len < 256, "Cannot extend key. Suffix is too long.");

        self.realloc(
            self.curr_layout(),
            self.flags,
            new_key_len,
            self.children().len(),
        );

        unsafe {
            // Shift value + children right
            ptr::copy(
                self.data.as_ptr().add(1 + self.key_len()),
                self.data.as_ptr().add(1 + new_key_len),
                self.data_size() - self.key_len() - 1,
            );
            // Extend key
            ptr::copy(
                suffix.as_ptr(),
                self.data.as_ptr().add(1 + self.key_len()),
                suffix.len(),
            );
            // Write new key length
            ptr::write(self.key_len_ptr(), new_key_len as u8);
        }
    }

    // Value access methods
    #[inline(always)]
    fn value_mut(&mut self) -> Option<&mut T> {
        if self.flags.contains(Flags::VALUE_INITIALIZED) {
            return unsafe { Some(&mut *self.value_ptr()) };
        }
        None
    }

    #[inline]
    fn take_value(&mut self) -> Option<T> {
        if self.flags.contains(Flags::VALUE_INITIALIZED) {
            self.flags.set(Flags::VALUE_INITIALIZED, false);
            Some(unsafe { ptr::read(self.value_ptr()) })
        } else {
            None
        }
    }

    #[inline]
    fn replace_value(&mut self, value: T) -> Option<T> {
        if !self.flags.contains(Flags::VALUE_ALLOCATED) {
            // Allocate value if it's not allocated
            let children_count = self.children().len();

            let mut new_flags = self.flags.clone();
            new_flags.set(Flags::VALUE_ALLOCATED, true);
            self.realloc(
                self.curr_layout(),
                new_flags,
                self.key_len(),
                children_count,
            );

            if self.flags.contains(Flags::HAS_CHILDREN) {
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

        if self.flags.contains(Flags::VALUE_INITIALIZED) {
            // Replace old value
            Some(unsafe { std::mem::replace::<T>(&mut *self.value_ptr(), value) })
        } else {
            // Write value and set initialized flag
            unsafe {
                ptr::write(self.value_ptr(), value);
            }
            self.flags.set(Flags::VALUE_INITIALIZED, true);
            None
        }
    }

    // Children access methods
    #[inline(always)]
    fn children_mut(&self) -> &mut [Node<T>] {
        if !self.flags.contains(Flags::HAS_CHILDREN) {
            return &mut [];
        }

        unsafe {
            std::slice::from_raw_parts_mut(
                self.children_ptr(),
                *self.children_len_ptr() as usize + 1,
            )
        }
    }

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

        if !self.flags.contains(Flags::HAS_CHILDREN) {
            // Allocate children
            let mut new_flags = self.flags.clone();
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
                self.flags,
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

        if self.flags.contains(Flags::HAS_CHILDREN) {
            let removed = unsafe { ptr::read(self.children_ptr().add(idx)) };
            if self.children().len() == 1 {
                // Deallocate children
                let mut new_flags = self.flags.clone();
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
                    self.flags,
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
        let mut new_flags = self.flags.clone();
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
        let mut new_src_flags = src_node.flags.clone();
        new_src_flags.set(Flags::HAS_CHILDREN, false);
        src_node.realloc(src_node.curr_layout(), new_src_flags, src_node.key_len(), 0);
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
        if self.flags.contains(Flags::VALUE_INITIALIZED) {
            // Drop value
            let _value = unsafe { ptr::read(self.data.as_ptr().add(1 + self.key_len()) as *mut T) };
        }
        if self.flags.contains(Flags::HAS_CHILDREN) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_iter::NodeIter;

    use std::collections::BTreeMap;

    #[test]
    fn test_new() {
        let node: Node<()> = Node::new(&[1, 2, 3]);
        assert_eq!(node.key(), &[1, 2, 3]);
        assert!(!node.flags.contains(Flags::VALUE_ALLOCATED));
        assert!(!node.flags.contains(Flags::VALUE_INITIALIZED));
        assert!(!node.flags.contains(Flags::HAS_CHILDREN));
    }

    #[test]
    fn test_modify_value() {
        let mut node: Node<u64> = Node::new(&[1, 2, 3]);
        node.push_child(Node::new(&[1]));
        node.push_child(Node::new(&[2]));

        assert_eq!(node.key(), &[1, 2, 3]);
        assert_eq!(node.value(), None);
        assert!(!node.flags.contains(Flags::VALUE_ALLOCATED));
        assert!(!node.flags.contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags.contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1]);
        assert_eq!(node.children()[1].key(), &[2]);

        assert_eq!(node.replace_value(123), None);
        assert_eq!(node.value(), Some(&123));
        assert!(node.flags.contains(Flags::VALUE_ALLOCATED));
        assert!(node.flags.contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags.contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1]);
        assert_eq!(node.children()[1].key(), &[2]);

        assert_eq!(node.replace_value(456), Some(123));
        assert_eq!(node.value(), Some(&456));
        assert!(node.flags.contains(Flags::VALUE_ALLOCATED));
        assert!(node.flags.contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags.contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1]);
        assert_eq!(node.children()[1].key(), &[2]);

        assert_eq!(node.take_value(), Some(456));
        assert_eq!(node.value(), None);
        assert!(node.flags.contains(Flags::VALUE_ALLOCATED));
        assert!(!node.flags.contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags.contains(Flags::HAS_CHILDREN));
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
        assert!(node.flags.contains(Flags::VALUE_ALLOCATED));
        assert!(node.flags.contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags.contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1]);
        assert_eq!(node.children()[1].key(), &[2]);

        node.strip_key_prefix(2);
        assert_eq!(node.key(), &[3, 4, 5]);
        assert_eq!(node.value(), Some(&123));
        assert!(node.flags.contains(Flags::VALUE_ALLOCATED));
        assert!(node.flags.contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags.contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1]);
        assert_eq!(node.children()[1].key(), &[2]);

        node.extend_key(&[6, 7, 8]);
        assert_eq!(node.key(), &[3, 4, 5, 6, 7, 8]);
        assert_eq!(node.value(), Some(&123));
        assert!(node.flags.contains(Flags::VALUE_ALLOCATED));
        assert!(node.flags.contains(Flags::VALUE_INITIALIZED));
        assert!(node.flags.contains(Flags::HAS_CHILDREN));
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
    }

    // Children tests
    #[test]
    fn test_children_add() {
        let mut node: Node<()> = Node::new(&[]);
        assert!(!node.flags.contains(Flags::HAS_CHILDREN));

        // Push
        node.push_child(Node::new(&[]));
        assert!(node.flags.contains(Flags::HAS_CHILDREN));
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
        assert!(node.flags.contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children()[0].key(), &[0, 1]);
        assert_eq!(node.children()[1].key(), &[1, 2]);
        assert_eq!(node.children()[2].key(), &[2, 3]);
        assert_eq!(node.children()[3].key(), &[3, 4]);
        assert_eq!(node.children()[4].key(), &[4, 5]);

        // Remove first
        assert_eq!(node.remove_child(0).key(), &[0, 1]);
        assert!(node.flags.contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 4);
        assert_eq!(node.children()[0].key(), &[1, 2]);
        assert_eq!(node.children()[1].key(), &[2, 3]);
        assert_eq!(node.children()[2].key(), &[3, 4]);
        assert_eq!(node.children()[3].key(), &[4, 5]);

        // Remove last
        assert_eq!(node.remove_child(3).key(), &[4, 5]);
        assert!(node.flags.contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 3);
        assert_eq!(node.children()[0].key(), &[1, 2]);
        assert_eq!(node.children()[1].key(), &[2, 3]);
        assert_eq!(node.children()[2].key(), &[3, 4]);

        // Remove mid
        assert_eq!(node.remove_child(1).key(), &[2, 3]);
        assert!(node.flags.contains(Flags::HAS_CHILDREN));
        assert_eq!(node.children().len(), 2);
        assert_eq!(node.children()[0].key(), &[1, 2]);
        assert_eq!(node.children()[1].key(), &[3, 4]);

        // Remove mid
        assert_eq!(node.remove_child(1).key(), &[3, 4]);
        assert!(node.flags.contains(Flags::HAS_CHILDREN));
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
        assert!(!node.flags.contains(Flags::HAS_CHILDREN));
    }
}

use std::alloc::{alloc, dealloc, realloc, Layout};
use std::marker::PhantomData;
use std::ptr;
use std::mem::size_of;

use crate::children::Children;
use crate::key::Key;

use bitflags::bitflags;

bitflags! {
    struct Flags: u8 {
        const VALUE_ALLOCATED = 0b0000_0001;
        const VALUE_INITIALIZED = 0b0000_0010;
        const CHILDREN_ALLOCATED = 0b0000_0100;
        const CHILDREN_INITIALIZED = 0b0000_1000;
    }
}

#[derive(Debug)]
pub(crate) struct Node<T> {
    /// Node flags
    flags: Flags,
    /// data
    data: ptr::NonNull<u8>,
    _phantom: PhantomData<T>,
}

impl<T> Node<T> {
    pub(crate) fn new(key: &[u8]) -> Self {
        let flags = Flags::empty();
        let data = Self::alloc(flags, key.len(), 0);

        Node {
            flags,
            data,
            _phantom: PhantomData::default()
        }
    }

    fn new_with_value(key: &[u8], value: T) -> Self {
        let mut flags = Flags::empty();
        flags.set(Flags::VALUE_ALLOCATED, true);
        let data = Self::alloc(flags, key.len(), 0);

        Node {
            flags,
            data,
            _phantom: PhantomData::default()
        }
    }

    #[inline(always)]
    fn create_layout(flags: Flags, key_len: usize, children_count: usize) -> Layout {
        let mut layout = Layout::array::<u8>(key_len + 1).expect("invalid layout");

        if flags.contains(Flags::VALUE_ALLOCATED) {
            layout = layout.extend(Layout::new::<T>()).expect("invalid layout").0;
        }

        if flags.contains(Flags::CHILDREN_ALLOCATED) {
            layout = layout.extend(Layout::new::<u8>()).expect("invalid layout").0;
            layout = layout.extend(Layout::array::<Node<T>>(children_count).expect("invalid layout")).expect("invalid layout").0;
        }

        layout.pad_to_align()
    }

    #[inline(always)]
    fn alloc(flags: Flags, key_len: usize, children_count: usize) -> ptr::NonNull<u8> {
        let layout = Self::create_layout(flags, key_len, 0);
        let ptr = unsafe { alloc(layout) };
        ptr::NonNull::new(ptr).expect("allocation failed")
    }

    #[inline(always)]
    fn key(&self) -> &[u8] {
        unsafe {
            let len = *self.data.as_ptr() as usize;
            std::slice::from_raw_parts(self.data.as_ptr().add(1), len)
        }
    }

    #[inline(always)]
    fn value(&self) -> Option<&T> {
        if self.flags.contains(Flags::VALUE_INITIALIZED) {
            return unsafe {
                let ptr = self.data.as_ptr().add(1 + self.key().len());
                Some(&*(ptr as *const T))
            };
        }
        None
    }

    #[inline(always)]
    fn children(&self) -> &[Node<T>] {
        if self.flags.contains(Flags::CHILDREN_INITIALIZED) {
            return &[]
        }

        let mut offset = 1 + self.key().len();
        if self.flags.contains(Flags::VALUE_ALLOCATED) {
            offset += size_of::<T>();
        }

        unsafe {
            let children_count = *self.data.as_ptr().add(offset) as usize;
            let ptr = self.data.as_ptr().add(1 + offset) as *const Node<T>;
            std::slice::from_raw_parts(ptr, children_count)
        }
    }

    #[inline(always)]
    fn replace_value(&mut self, value: T) -> Option<T> {
        let old_value = if self.flags.contains(Flags::VALUE_INITIALIZED) {
            Some(unsafe {
                let ptr = self.data.as_ptr().add(1 + self.key().len());
                ptr::read(ptr as *const T)
            })
        } else {
            None
        };

        if self.flags.contains(Flags::VALUE_ALLOCATED) {
            unsafe {
                let ptr = self.data.as_ptr().add(1 + self.key().len());
                ptr::write(ptr as *mut T, value);
            }
            
        } else {
            let old_layout = Self::create_layout(self.flags, self.key().len(), self.children().len());
            self.flags.set(Flags::VALUE_ALLOCATED, true);
            let new_layout = Self::create_layout(self.flags, self.key().len(), self.children().len());

            let new_ptr = unsafe {
                realloc(self.data.as_ptr(), old_layout, new_layout.size())
            };
            self.data = ptr::NonNull::new(new_ptr).expect("allocation failed");

            unsafe {
                
            }

        }
        self.flags.set(Flags::VALUE_INITIALIZED, true);

        old_value
    }

    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.value().is_none() && self.children().is_empty()
    }

    pub(crate) fn insert(&mut self, key: &[u8], value: T) -> Option<T> {
        if key.is_empty() {
            return self.value.replace(value);
        }

        let (prefix_len, child_idx) = self.longest_common_prefix(key);
        if prefix_len == 0 {
            // No child shares a prefix with the key
            return self.insert_child(child_idx, key, value);
        }

        // Some child shares a prefix with the key
        let children = self.children.as_mut().unwrap();
        if prefix_len == children[child_idx].key.len() {
            // Child's key is a prefix of the inserted key
            return children[child_idx].insert(&key[prefix_len..], value);
        }

        // Only a portion of child's key shares prefix with the inserted key
        Self::split_child(children, child_idx, prefix_len, key, value)
    }

    pub(crate) fn remove(&mut self, key: &[u8]) -> Option<T> {
        if key.is_empty() {
            let removed = self.value.take();
            if (self.children().len() == 1) && (self.key.len() + self.children()[0].key.len() < 256)
            {
                // If the node has only one child and len(node key + child key) < 256
                // then we can merge the nodes together.
                let mut children = self.children.take().unwrap();
                let child_node = &mut children[0];
                self.key.extend(child_node.key());
                self.value = child_node.value.take();
                self.children = child_node.children.take();
            }
            return removed;
        }

        match self.select_next_child(key) {
            Some((prefix_len, child_idx)) => {
                let children = self.children.as_mut().unwrap();
                let removed = children[child_idx].remove(&key[prefix_len..]);

                if removed.is_some() && children[child_idx].is_empty() {
                    self.remove_child(child_idx);
                }

                removed
            }
            None => None,
        }
    }

    pub(crate) fn get(&self, key: &[u8]) -> Option<&T> {
        if key.is_empty() {
            return self.value.as_ref();
        }

        match self.select_next_child(key) {
            Some((prefix_len, child_idx)) => {
                self.children.as_ref().unwrap()[child_idx].get(&key[prefix_len..])
            }
            None => None,
        }
    }

    pub(crate) fn get_mut(&mut self, key: &[u8]) -> Option<&mut T> {
        if key.is_empty() {
            return self.value.as_mut();
        }

        match self.select_next_child(key) {
            Some((prefix_len, child_idx)) => {
                self.children.as_mut().unwrap()[child_idx].get_mut(&key[prefix_len..])
            }
            None => None,
        }
    }

    #[inline(always)]
    fn select_next_child(&self, key: &[u8]) -> Option<(usize, usize)> {
        let (prefix_len, child_idx) = self.longest_common_prefix(key);
        if (prefix_len == 0) || (prefix_len < self.children.as_ref().unwrap()[child_idx].key.len())
        {
            // There is no or only a partial match in which case the
            // key does not exist in the tree.
            return None;
        }
        Some((prefix_len, child_idx))
    }

    #[inline(always)]
    fn insert_child(&mut self, idx: usize, key: &[u8], value: T) -> Option<T> {
        let node = Node::new_with_value(key, value);
        match &mut self.children {
            Some(children) => children.insert(idx, node),
            None => self.children = Some(Children::new(node)),
        };
        None
    }

    #[inline(always)]
    fn remove_child(&mut self, idx: usize) {
        match &mut self.children {
            Some(children) => {
                if children.len() == 1 {
                    assert_eq!(idx, 0, "Invalid remove index");
                    self.children.take();
                } else {
                    children.remove(idx);
                }
            }
            None => panic!("Cannot remove child. Node has not children."),
        }
    }

    #[inline(always)]
    fn split_child(
        children: &mut Children<T>,
        idx: usize,
        prefix_len: usize,
        key: &[u8],
        value: T,
    ) -> Option<T> {
        // Replace node with new (uninitialized) node
        let mut old = std::mem::replace(&mut children[idx], Node::new(&key[..prefix_len]));
        // Update old node's key
        old.key = Key::new(&old.key[prefix_len..]);
        // Initialize new node's children with the old node
        children[idx].children = Some(Children::new(old));
        // Insert into the new node
        children[idx].insert(&key[prefix_len..], value)
    }

    #[inline]
    fn longest_common_prefix(&self, key: &[u8]) -> (usize, usize) {
        if let Some(children) = &self.children {
            // If an element exists in the array it returns Ok(index)
            // If an element does not exist in the array it returns Err(index) where index
            // is the insert index that maintains the sort order.
            let byte0 = [key[0]];
            let idx = match children.binary_search_by_key(&byte0.as_slice(), |k| &k.key) {
                Ok(idx) => idx,
                Err(idx) => idx,
            };

            if (idx >= children.len()) || (children[idx].key[0] != key[0]) {
                // Not found
                return (0, idx);
            }

            let common_prefix_len = key
                .iter()
                .zip(children[idx].key.iter())
                .take_while(|(a, b)| a == b)
                .count();

            (common_prefix_len, idx)
        } else {
            // The children array is empty so there is not common prefix
            // and the new item should be inserted at index = 0.
            (0, 0)
        }
    }
}

pub(crate) struct NodeIter<'a, V> {
    stack: Vec<&'a Node<V>>,
}

impl<'a, V> NodeIter<'a, V> {
    pub(crate) fn new(root: &'a Node<V>) -> Self {
        NodeIter { stack: vec![root] }
    }
}

impl<'a, V> Iterator for NodeIter<'a, V> {
    type Item = &'a Node<V>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.stack.pop() {
            Some(node) => {
                if let Some(children) = &node.children {
                    for child in children.iter().rev() {
                        self.stack.push(child);
                    }
                }
                Some(node)
            }
            None => None,
        }
    }
}

struct Node2<T> {
    flags: Flags,
    data: *mut u8,
    _marker: std::marker::PhantomData<T>
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    #[test]
    fn test_size() {
        println!("Size Node<()>: {}", std::mem::size_of::<Node<()>>());
        println!("Size Node<u8>: {}", std::mem::size_of::<Node<u8>>());
        println!("Size Node<u32>: {}", std::mem::size_of::<Node<u32>>());
        println!("Size Node<u64>: {}", std::mem::size_of::<Node<u64>>());

        println!("Size Node2<()>: {}", std::mem::size_of::<Node2<()>>());
        println!("Size Node2<u8>: {}", std::mem::size_of::<Node2<u8>>());
        println!("Size Node2<u32>: {}", std::mem::size_of::<Node2<u32>>());
        println!("Size Node2<u64>: {}", std::mem::size_of::<Node2<u64>>());
    }

    #[test]
    fn test_longest_common_prefix() {
        let mut children: Children<()> = Children::new(Node::new("abb;0".as_bytes()));
        children.push(Node::new("cde;1".as_bytes()));
        children.push(Node::new("fgh;2".as_bytes()));
        children.push(Node::new("ijk;3".as_bytes()));

        let mut node = Node::new("".as_bytes());
        node.children = Some(children);

        assert_eq!(node.longest_common_prefix("abb;1".as_bytes()), (4, 0));
        assert_eq!(node.longest_common_prefix("abb;0123".as_bytes()), (5, 0));
        assert_eq!(node.longest_common_prefix("fg".as_bytes()), (2, 2));
        assert_eq!(node.longest_common_prefix("ijk;2".as_bytes()), (4, 3));
        assert_eq!(node.longest_common_prefix("ijk;3ab".as_bytes()), (5, 3));
        assert_eq!(node.longest_common_prefix("i".as_bytes()), (1, 3));
        assert_eq!(node.longest_common_prefix("lmo".as_bytes()), (0, 4));
        assert_eq!(node.longest_common_prefix("bar".as_bytes()), (0, 1));
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
}

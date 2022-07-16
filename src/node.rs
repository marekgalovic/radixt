use crate::children::Children;
use crate::key::Key;

#[derive(Debug)]
pub(crate) struct Node<T> {
    /// Prefix for this node
    key: Key,
    /// Node data
    value: Option<T>,
    /// Node children
    children: Option<Children<T>>,
}

impl<T> Node<T> {
    pub(crate) fn new(key: &[u8]) -> Self {
        Node {
            key: Key::new(key),
            value: None,
            children: None,
        }
    }

    pub(crate) fn new_with_value(key: &[u8], value: T) -> Self {
        Node {
            key: Key::new(key),
            value: Some(value),
            children: None,
        }
    }

    #[inline(always)]
    pub(crate) fn key(&self) -> &[u8] {
        &self.key
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

    pub(crate) fn get(&self, key: &[u8]) -> Option<&T> {
        if key.is_empty() {
            return self.value.as_ref();
        }

        let (prefix_len, child_idx) = self.longest_common_prefix(key);
        if (prefix_len == 0) || (prefix_len < self.children.as_ref().unwrap()[child_idx].key.len())
        {
            // There is no or only a partial match in which case the
            // key does not exist in the tree.
            return None;
        }

        self.children.as_ref().unwrap()[child_idx].get(&key[prefix_len..])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_longest_common_prefix() {
        let mut children: Children<()> = Children::new(Node::new("abb;0".as_bytes()));
        children.push(Node::new("cde;1".as_bytes()));
        children.push(Node::new("fgh;2".as_bytes()));
        children.push(Node::new("ijk;3".as_bytes()));

        let mut node = Node::new("".as_bytes());
        node.children = Some(children);

        println!("{:?}", node.longest_common_prefix("abb;1".as_bytes()));
    }

    #[test]
    fn test_insert() {
        println!("Node size: {}", std::mem::size_of::<Node<()>>());

        let mut node = Node::new("".as_bytes());

        node.insert("abc;0".as_bytes(), 1);
        println!("{:?}", node);

        node.insert("abb;0".as_bytes(), 2);
        println!("{:?}", node);

        node.insert("ab".as_bytes(), 3);
        println!("{:?}", node);
    }

    #[test]
    fn test_wasted_space() {
        let mut node = Node::new(&[]);

        println!("Node size: {}", std::mem::size_of::<Node<u32>>());
        println!("Key size: {}", std::mem::size_of::<Key>());
        println!("Key size: {}", std::mem::size_of::<Box<[u8]>>());
        // println!("Option size: {}", std::mem::size_of::<Option<()>>());

        for i in 0..1000000_u32 {
            node.insert(i.to_be_bytes().as_slice(), i);
        }

        let mut n_nodes = 0;
        let mut n_nodes_with_value = 0;
        for node in NodeIter::new(&node) {
            n_nodes += 1;
            if node.value.is_some() {
                n_nodes_with_value += 1;
            }
            let c_len = node.children.as_ref().map(|c| c.len()).unwrap_or(0);
            assert!(c_len <= 256, "Node has {} children", c_len);
        }
    }
}

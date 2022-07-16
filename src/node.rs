use std::ops::Deref;

#[repr(transparent)]
struct NodeKey(Box<[u8]>);

impl NodeKey {
    fn new(data: &[u8]) -> Self {
        NodeKey(data.into())
    }
}

impl Deref for NodeKey {
    type Target = Box<[u8]>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Debug for NodeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(std::str::from_utf8(&self.0).unwrap())
    }
}

#[derive(Debug)]
pub(crate) struct Node<V> {
    /// Prefix for this node
    key: NodeKey,
    /// Value of this node, if any
    value: Option<V>,
    /// Children of this node sorted by their key in
    /// ascending order.
    children: Vec<Node<V>>,
}

impl<V> Node<V> {
    pub(crate) fn new(key: &[u8], value: Option<V>) -> Self {
        Node {
            key: NodeKey::new(key),
            value,
            children: vec![],
        }
    }

    pub(crate) fn insert(&mut self, key: &[u8], value: V) -> Option<V> {
        if key.is_empty() {
            return self.value.replace(value);
        }

        let (prefix_len, child_idx) = self.longest_common_prefix(key);
        if prefix_len == 0 {
            // No child shares a prefix with the key
            return self.insert_child(child_idx, key, value);
        }

        if prefix_len == self.children[child_idx].key.len() {
            // Child's key is a prefix of the inserted key
            return self.children[child_idx].insert(&key[prefix_len..], value);
        }

        // Only a portion of child's key shares prefix with the inserted key
        self.split_child(child_idx, prefix_len, key, value)
    }

    pub(crate) fn get(&self, key: &[u8]) -> Option<&V> {
        if key.is_empty() {
            return self.value.as_ref();
        }

        let (prefix_len, child_idx) = self.longest_common_prefix(key);
        if (prefix_len == 0) || (prefix_len < self.children[child_idx].key.len()) {
            // There is no or only a partial match in which case the
            // key does not exist in the tree.
            return None;
        }

        self.children[child_idx].get(&key[prefix_len..])
    }

    #[inline(always)]
    fn insert_child(&mut self, idx: usize, key: &[u8], value: V) -> Option<V> {
        self.children.reserve_exact(1);
        self.children.insert(idx, Node::new(key, Some(value)));
        None
    }

    #[inline(always)]
    fn split_child(&mut self, idx: usize, prefix_len: usize, key: &[u8], value: V) -> Option<V> {
        // Replace node with new (uninitialized) node
        let mut old =
            std::mem::replace(&mut self.children[idx], Node::new(&key[..prefix_len], None));
        // Update old node's key
        old.key = NodeKey::new(&old.key[prefix_len..]);
        // Push the old node into new node's children
        self.children[idx].children.push(old);
        // Insert into the new node
        self.children[idx].insert(&key[prefix_len..], value)
    }

    #[inline]
    fn longest_common_prefix(&self, key: &[u8]) -> (usize, usize) {
        // If an element exists in the array it returns Ok(index)
        // If an element does not exist in the array it returns Err(index) where index
        // is the insert index that maintains the sort order.
        let byte0 = [key[0]];
        let idx = match self
            .children
            .binary_search_by_key(&byte0.as_slice(), |k| &k.key)
        {
            Ok(idx) => idx,
            Err(idx) => idx,
        };

        if (idx >= self.children.len()) || (self.children[idx].key[0] != key[0]) {
            // Not found
            return (0, idx);
        }

        let common_prefix_len = key
            .iter()
            .zip(self.children[idx].key.iter())
            .take_while(|(a, b)| a == b)
            .count();

        (common_prefix_len, idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_longest_common_prefix() {
        let mut node = Node::new("".as_bytes(), None);
        node.children.push(Node::new("abb;0".as_bytes(), Some(0)));
        node.children.push(Node::new("cde;1".as_bytes(), Some(1)));
        node.children.push(Node::new("fgh;2".as_bytes(), Some(2)));
        node.children.push(Node::new("ijk;3".as_bytes(), Some(3)));

        println!("{:?}", node.longest_common_prefix("abb;1".as_bytes()));
    }

    #[test]
    fn test_insert() {
        let mut node = Node::new("".as_bytes(), None);

        node.insert("abc;0".as_bytes(), 1);
        println!("{:?}", node);

        node.insert("abb;0".as_bytes(), 2);
        println!("{:?}", node);

        node.insert("ab".as_bytes(), 3);
        println!("{:?}", node);
    }
}

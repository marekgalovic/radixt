pub(crate) mod node;

pub mod iter;
pub mod map;
pub mod set;
pub use map::RadixMap;
pub use set::RadixSet;

#[inline]
fn longest_common_prefix<T>(children: &[node::Node<T>], key: &[u8]) -> (usize, usize) {
    // If an element exists in the array it returns Ok(index)
    // If an element does not exist in the array it returns Err(index) where index
    // is the insert index that maintains the sort order.
    let byte0 = [key[0]];
    let idx = match children.binary_search_by_key(&byte0.as_slice(), |k| k.key()) {
        Ok(idx) => idx,
        Err(idx) => idx,
    };

    if (idx >= children.len()) || (children[idx].key()[0] != key[0]) {
        // Not found
        return (0, idx);
    }

    let common_prefix_len = key
        .iter()
        .zip(children[idx].key().iter())
        .take_while(|(a, b)| a == b)
        .count();

    (common_prefix_len, idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_longest_common_prefix() {
        let mut node: node::Node<()> = node::Node::new("".as_bytes());

        node.push_child(node::Node::new("abb;0".as_bytes()));
        node.push_child(node::Node::new("cde;1".as_bytes()));
        node.push_child(node::Node::new("fgh;2".as_bytes()));
        node.push_child(node::Node::new("ijk;3".as_bytes()));

        assert_eq!(
            longest_common_prefix(node.children(), "abb;1".as_bytes()),
            (4, 0)
        );
        assert_eq!(
            longest_common_prefix(node.children(), "abb;0123".as_bytes()),
            (5, 0)
        );
        assert_eq!(
            longest_common_prefix(node.children(), "fg".as_bytes()),
            (2, 2)
        );
        assert_eq!(
            longest_common_prefix(node.children(), "ijk;2".as_bytes()),
            (4, 3)
        );
        assert_eq!(
            longest_common_prefix(node.children(), "ijk;3ab".as_bytes()),
            (5, 3)
        );
        assert_eq!(
            longest_common_prefix(node.children(), "i".as_bytes()),
            (1, 3)
        );
        assert_eq!(
            longest_common_prefix(node.children(), "lmo".as_bytes()),
            (0, 4)
        );
        assert_eq!(
            longest_common_prefix(node.children(), "bar".as_bytes()),
            (0, 1)
        );
    }
}

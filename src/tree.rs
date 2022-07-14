use std::collections::BTreeMap;

#[derive(Debug)]
pub struct Node<V> {
    value: Option<V>,
    children: Option<Box<BTreeMap<Box<[u8]>, Node<V>>>>,
}

#[inline]
fn init_map<V>(key: &[u8], node: Node<V>) -> Box<BTreeMap<Box<[u8]>, Node<V>>> {
    let mut map: BTreeMap<Box<[u8]>, Node<V>> = BTreeMap::new();
    map.insert(key.into(), node);
    Box::new(map)
}

#[inline]
fn init_map_from_kv<V>(key: &[u8], value: V) -> Box<BTreeMap<Box<[u8]>, Node<V>>> {
    init_map(key, Node::new(Some(value), None))
}

impl<V> Node<V> {
    #[inline(always)]
    pub fn new(value: Option<V>, children: Option<Box<BTreeMap<Box<[u8]>, Node<V>>>>) -> Self {
        Node { value, children }
    }

    pub fn insert(&mut self, key: &[u8], value: V) -> Option<V> {
        if key.is_empty() {
            self.value.replace(value)
        } else if let Some(children) = &mut self.children {
            if let Some((prefix_len, map_key)) = Self::longest_common_prefix(key, children) {
                if prefix_len == map_key.len() {
                    children
                        .get_mut(&map_key)
                        .unwrap()
                        .insert(&key[prefix_len..], value)
                } else {
                    let v = children.remove(&map_key).unwrap();
                    let prefix = &map_key[..prefix_len];

                    children.insert(
                        prefix.into(),
                        Node::new(None, Some(init_map(&map_key[prefix_len..], v))),
                    );
                    children
                        .get_mut(prefix)
                        .unwrap()
                        .insert(&key[prefix_len..], value)
                }
            } else {
                children.insert(key.into(), Node::new(Some(value), None));
                None
            }
        } else {
            self.children = Some(init_map_from_kv(key, value));
            None
        }
    }

    pub fn get(&self, key: &[u8]) -> Option<&V> {
        if key.is_empty() {
            self.value.as_ref()
        } else if let Some(children) = &self.children {
            if let Some((prefix_len, map_key)) = Self::longest_common_prefix(key, children) {
                children.get(&map_key).unwrap().get(&key[prefix_len..])
            } else {
                None
            }
        } else {
            None
        }
    }

    #[inline]
    fn longest_common_prefix<'a>(
        key: &[u8],
        map: &'a BTreeMap<Box<[u8]>, Node<V>>,
    ) -> Option<(usize, Box<[u8]>)> {
        for i in (1..=key.len()).rev() {
            let bound: Box<[u8]> = key[..i].into();
            if let Some((k, _)) = map.range(bound..).next() {
                return Some((i, k.clone()));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_longest_common_prefix() {
        let mut map: BTreeMap<Box<[u8]>, Node<usize>> = BTreeMap::new();
        map.insert("hello".as_bytes().into(), Node::new(Some(0), None));
        map.insert("foo".as_bytes().into(), Node::new(Some(2), None));
        map.insert("bar".as_bytes().into(), Node::new(Some(3), None));

        assert_eq!(
            Node::longest_common_prefix("heloo".as_bytes(), &map),
            Some((3, Box::from("hello".as_bytes())))
        );
        assert_eq!(
            Node::longest_common_prefix("helloa".as_bytes(), &map),
            Some((5, Box::from("hello".as_bytes())))
        );
        assert_eq!(
            Node::longest_common_prefix("hello".as_bytes(), &map),
            Some((5, Box::from("hello".as_bytes())))
        );
        assert_eq!(
            Node::longest_common_prefix("bar".as_bytes(), &map),
            Some((3, Box::from("bar".as_bytes())))
        );
        assert_eq!(
            Node::longest_common_prefix("f".as_bytes(), &map),
            Some((1, Box::from("foo".as_bytes())))
        );
        assert_eq!(
            Node::longest_common_prefix("fo".as_bytes(), &map),
            Some((2, Box::from("foo".as_bytes())))
        );
        assert_eq!(Node::longest_common_prefix("rand".as_bytes(), &map), None);
    }

    #[test]
    fn test_insert_and_get() {
        let mut node: Node<usize> = Node::new(None, None);

        for i in 0..10_u32 {
            node.insert(i.to_be_bytes().as_slice(), i as usize);
        }

        for i in 0..10_u32 {
            assert_eq!(node.get(i.to_be_bytes().as_slice()), Some(&(i as usize)));
        }
    }
}

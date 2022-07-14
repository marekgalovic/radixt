use super::tree::Node;

#[derive(Debug)]
pub struct RadixMap<V> {
    root: Node<V>,
}

impl<V> RadixMap<V> {
    pub fn new() -> Self {
        RadixMap {
            root: Node::new(None, None),
        }
    }

    #[inline(always)]
    pub fn insert<K: AsRef<[u8]>>(&mut self, key: K, value: V) -> Option<V> {
        self.root.insert(key.as_ref(), value)
    }

    #[inline(always)]
    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> Option<&V> {
        self.root.get(key.as_ref())
    }
}

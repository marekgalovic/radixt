use crate::node::Node;

#[derive(Debug)]
pub struct RadixMap<V> {
    root: Node<V>,
    size: usize,
}

impl<V> RadixMap<V> {
    pub fn new() -> Self {
        RadixMap {
            root: Node::new(&[], None),
            size: 0,
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.size
    }

    #[inline(always)]
    pub fn insert<K: AsRef<[u8]>>(&mut self, key: K, value: V) -> Option<V> {
        let old = self.root.insert(key.as_ref(), value);
        self.size += old.is_none() as usize;
        old
    }

    #[inline(always)]
    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> Option<&V> {
        self.root.get(key.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut m = RadixMap::new();

        m.insert("abc;0", 1);
        m.insert("abb;0", 2);
        m.insert("ab", 3);
        m.insert("c", 4);
        m.insert("cad", 5);

        assert_eq!(m.len(), 5);

        assert_eq!(m.get("ab").unwrap(), &3);
        assert_eq!(m.get("abc;0").unwrap(), &1);
        assert_eq!(m.get("abb;0").unwrap(), &2);
        assert_eq!(m.get("c").unwrap(), &4);
        assert_eq!(m.get("cad").unwrap(), &5);

        assert_eq!(m.get("d"), None);
        assert_eq!(m.get("ac"), None);
        assert_eq!(m.get("abd"), None);
        assert_eq!(m.get("abc;"), None);
        assert_eq!(m.get("abc;1"), None);
        assert_eq!(m.get(""), None);
    }
}

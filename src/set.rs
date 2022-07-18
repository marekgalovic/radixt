use crate::iter::{Iter, MapK};
use crate::map::RadixMap;

#[derive(Debug)]
pub struct RadixSet {
    inner: RadixMap<()>,
}

impl RadixSet {
    pub fn new() -> Self {
        RadixSet {
            inner: RadixMap::new(),
        }
    }

    /// Returns the number of elements in the set.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the set contains no elements.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Adds a value to the set.
    ///
    /// If the set did not have an equal element present, true is returned.
    ///
    /// If the set did have an equal element present, false is returned,
    #[inline(always)]
    pub fn insert<K: AsRef<[u8]>>(&mut self, key: K) -> bool {
        self.inner.insert(key.as_ref(), ()).is_none()
    }

    /// If the set contains an element equal to the value, removes it from
    /// the set and drops it. Returns whether such an element was present.
    #[inline(always)]
    pub fn remove<K: AsRef<[u8]>>(&mut self, key: K) -> bool {
        self.inner.remove(key.as_ref()).is_some()
    }

    /// Returns `true` if the set contains an element equal to the value.
    #[inline(always)]
    pub fn contains<K: AsRef<[u8]>>(&self, key: K) -> bool {
        self.inner.contains_key(key)
    }

    /// Gets an iterator that visits the elements of this set in ascending order.
    #[inline(always)]
    pub fn iter(&self) -> Iter<(), MapK<()>> {
        self.inner.keys()
    }

    /// Gets an iterator that visits the elements matching a given prefix in ascending order.
    #[inline(always)]
    pub fn prefix_iter<K: AsRef<[u8]>>(&self, prefix: K) -> Iter<(), MapK<()>> {
        self.inner.prefix_keys(prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut set = RadixSet::new();

        assert_eq!(set.insert("abc;0"), true);
        assert_eq!(set.insert("abb;0"), true);
        assert_eq!(set.insert("ab"), true);
        assert_eq!(set.insert("c"), true);
        assert_eq!(set.insert("cad"), true);
        assert_eq!(set.insert("cad"), false);

        assert_eq!(set.len(), 5);

        assert!(set.contains("ab"));
        assert!(set.contains("abc;0"));
        assert!(set.contains("abb;0"));
        assert!(set.contains("c"));
        assert!(set.contains("cad"));

        assert!(!set.contains("d"));
        assert!(!set.contains("ac"));
        assert!(!set.contains("abd"));
        assert!(!set.contains("abc;"));
        assert!(!set.contains("abc;1"));
        assert!(!set.contains(""));
    }

    fn populated_set() -> RadixSet {
        let mut set = RadixSet::new();
        set.insert("cad");
        set.insert("abc;0");
        set.insert("c");
        set.insert("abb;0");
        set.insert("ab");
        set
    }

    #[test]
    fn test_remove() {
        let mut set = populated_set();

        assert_eq!(set.len(), 5);
        assert!(set.contains("ab"));
        assert!(set.contains("abc;0"));
        assert!(set.contains("abb;0"));
        assert!(set.contains("c"));
        assert!(set.contains("cad"));

        assert!(set.remove("ab"));
        assert_eq!(set.len(), 4);
        assert!(!set.contains("ab"));

        assert!(set.remove("cad"));
        assert_eq!(set.len(), 3);
        assert!(!set.contains("cad"));

        assert!(!set.remove("cad"));
        assert!(!set.remove("foobar"));

        assert!(set.contains("abc;0"));
        assert!(set.contains("abb;0"));
        assert!(set.contains("c"));
    }

    #[test]
    fn test_iter() {
        let set = populated_set();

        let mut it = set.iter();

        let k = it.next().unwrap();
        assert_eq!(k.as_ref(), b"ab");
        let k = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abb;0");
        let k = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abc;0");
        let k = it.next().unwrap();
        assert_eq!(k.as_ref(), b"c");
        let k = it.next().unwrap();
        assert_eq!(k.as_ref(), b"cad");

        assert_eq!(it.next(), None);
    }

    #[test]
    fn test_prefix_iter() {
        let set = populated_set();

        let mut it = set.prefix_iter(b"ab");
        let k = it.next().unwrap();
        assert_eq!(k.as_ref(), b"ab");
        let k = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abb;0");
        let k = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abc;0");
        assert_eq!(it.next(), None);

        let mut it = set.prefix_iter(b"abb");
        let k = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abb;0");
        assert_eq!(it.next(), None);

        let mut it = set.prefix_iter(b"c");
        let k = it.next().unwrap();
        assert_eq!(k.as_ref(), b"c");
        let k = it.next().unwrap();
        assert_eq!(k.as_ref(), b"cad");
        assert_eq!(it.next(), None);

        let mut it = set.prefix_iter(b"ca");
        let k = it.next().unwrap();
        assert_eq!(k.as_ref(), b"cad");
        assert_eq!(it.next(), None);

        let mut it = set.prefix_iter(b"cad");
        let k = it.next().unwrap();
        assert_eq!(k.as_ref(), b"cad");
        assert_eq!(it.next(), None);

        let mut it = set.prefix_iter(b"cada");
        assert_eq!(it.next(), None);

        let mut it = set.prefix_iter(b"abd");
        assert_eq!(it.next(), None);
    }
}

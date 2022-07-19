use std::ops::RangeBounds;

use crate::iter::{
    Iter, IterMap, IterMapMut, IterMut, MapK, MapKV, MapKVMut, MapV, MapVMut, Range, RangeMut,
};
use crate::node::Node;

#[derive(Debug)]
pub struct RadixMap<T> {
    root: Node<T>,
    size: usize,
}

impl<T> RadixMap<T> {
    pub fn new() -> Self {
        RadixMap {
            root: Node::new(&[]),
            size: 0,
        }
    }

    /// Returns the number of elements in the map.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.size
    }

    /// Returns `true` if the map contains no elements.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Inserts a key-value pair into the map.
    ///
    /// If the map did not have this key present, None is returned.
    ///
    /// If the map did have this key present, the value is updated, and the old
    /// value is returned.
    #[inline(always)]
    pub fn insert<K: AsRef<[u8]>>(&mut self, key: K, value: T) -> Option<T> {
        let old = self.root.insert(key.as_ref(), value);
        self.size += old.is_none() as usize;
        old
    }

    /// Removes a key from the map, returning the value at the key if the key
    /// was previously in the map.
    #[inline(always)]
    pub fn remove<K: AsRef<[u8]>>(&mut self, key: K) -> Option<T> {
        let removed = self.root.remove(key.as_ref());
        self.size -= removed.is_some() as usize;
        removed
    }

    /// Returns a reference to the value corresponding to the key.
    #[inline(always)]
    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> Option<&T> {
        self.root.get(key.as_ref())
    }

    /// Returns a mutable reference to the value corresponding to the key.
    #[inline(always)]
    pub fn get_mut<K: AsRef<[u8]>>(&mut self, key: K) -> Option<&mut T> {
        self.root.get_mut(key.as_ref())
    }

    /// Returns `true` if this map contains a value for the specified key.
    #[inline(always)]
    pub fn contains_key<K: AsRef<[u8]>>(&self, key: K) -> bool {
        self.get(key).is_some()
    }

    /// Gets an iterator over the entries of the map, sorted by key.
    /// This iterator allocates a boxed slice for each item. If you
    /// only need to access values consider using [`Self::values()`] instead.
    #[inline(always)]
    pub fn iter(&self) -> Iter<T, MapKV<T>> {
        self.get_iter()
    }

    /// Gets a mutable iterator over the entries of the map, sorted by key.
    /// This iterator allocates a boxed slice for each item. If you only
    /// need to access values consider using [`Self::values_mut()`] instead.
    #[inline(always)]
    pub fn iter_mut(&mut self) -> IterMut<T, MapKVMut<T>> {
        self.get_iter_mut()
    }

    /// Gets an iterator over the entries of the map matching a given prefix, sorted by key.
    ///
    /// This iterator allocates a boxed slice for each item. If you only
    /// need to access values consider using [`Self::prefix_values()`] instead.
    #[inline(always)]
    pub fn prefix_iter<K: AsRef<[u8]>>(&self, prefix: K) -> Iter<T, MapKV<T>> {
        self.get_prefix_iter(prefix)
    }

    /// Gets a mutable iterator over the entries of the map matching a given prefix, sorted by key.
    ///
    /// This iterator allocates a boxed slice for each item. If you only
    /// need to access values consider using [`Self::prefix_values_mut()`] instead.
    #[inline(always)]
    pub fn prefix_iter_mut<K: AsRef<[u8]>>(&mut self, prefix: K) -> IterMut<T, MapKVMut<T>> {
        self.get_prefix_iter_mut(prefix)
    }

    /// Gets an iterator over the values of the map, in order by key.
    #[inline(always)]
    pub fn values(&self) -> Iter<T, MapV<T>> {
        self.get_iter()
    }

    /// Gets a mutable iterator over the values of the map, in order by key.
    #[inline(always)]
    pub fn values_mut(&mut self) -> IterMut<T, MapVMut<T>> {
        self.get_iter_mut()
    }

    /// Gets an iterator over the values of the map matching a given prefix, in order by key.
    #[inline(always)]
    pub fn prefix_values<K: AsRef<[u8]>>(&self, prefix: K) -> Iter<T, MapV<T>> {
        self.get_prefix_iter(prefix)
    }

    /// Gets a mutable iterator over the values of the map matching a given prefix, in order by key.
    #[inline(always)]
    pub fn prefix_values_mut<K: AsRef<[u8]>>(&mut self, prefix: K) -> IterMut<T, MapVMut<T>> {
        self.get_prefix_iter_mut(prefix)
    }

    /// Gets an iterator over the keys of the map, in order by key.
    #[inline(always)]
    pub fn keys(&self) -> Iter<T, MapK<T>> {
        self.get_iter()
    }

    /// Gets an iterator over the keys of the map matching a given prefix, in order by key.
    #[inline(always)]
    pub fn prefix_keys<K: AsRef<[u8]>>(&self, prefix: K) -> Iter<T, MapK<T>> {
        self.get_prefix_iter(prefix)
    }

    /// Constructs an iterator over a sub-range of elements in the map. The simplest
    /// way is to use the range syntax `min..max`, thus `range(min..max)` will yield elements from min
    /// (inclusive) to max (exclusive). The range may also be entered as `(Bound<T>, Bound<T>)`.
    #[inline(always)]
    pub fn range<K: AsRef<[u8]>, B: RangeBounds<K>>(&self, bounds: B) -> Range<T, K, B> {
        Range::new(self.get_iter(), bounds)
    }

    /// Constructs a mutable iterator over a sub-range of elements in the map. The simplest
    /// way is to use the range syntax `min..max`, thus `range(min..max)` will yield elements from min
    /// (inclusive) to max (exclusive). The range may also be entered as `(Bound<T>, Bound<T>)`.
    #[inline(always)]
    pub fn range_mut<K: AsRef<[u8]>, B: RangeBounds<K>>(&mut self, bounds: B) -> RangeMut<T, K, B> {
        RangeMut::new(self.get_iter_mut(), bounds)
    }

    fn get_iter<'a, M: IterMap<'a, T>>(&'a self) -> Iter<'a, T, M> {
        Iter::new(Some(&self.root), vec![])
    }

    fn get_iter_mut<'a, M: IterMapMut<'a, T>>(&'a mut self) -> IterMut<'a, T, M> {
        IterMut::new(Some(&mut self.root), vec![])
    }

    fn get_prefix_iter<'a, M: IterMap<'a, T>, K: AsRef<[u8]>>(
        &'a self,
        prefix: K,
    ) -> Iter<'a, T, M> {
        match self.root.find_prefix(prefix.as_ref()) {
            Some((prefix_len, prefix_node)) => {
                Iter::new(Some(prefix_node), prefix.as_ref()[..prefix_len].to_vec())
            }
            None => Iter::new(None, vec![]),
        }
    }

    fn get_prefix_iter_mut<'a, M: IterMapMut<'a, T>, K: AsRef<[u8]>>(
        &'a mut self,
        prefix: K,
    ) -> IterMut<'a, T, M> {
        match self.root.find_prefix_mut(prefix.as_ref()) {
            Some((prefix_len, prefix_node)) => {
                IterMut::new(Some(prefix_node), prefix.as_ref()[..prefix_len].to_vec())
            }
            None => IterMut::new(None, vec![]),
        }
    }

    #[inline(always)]
    pub(crate) fn root(&self) -> &Node<T> {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn populated_map() -> RadixMap<u32> {
        let mut m = RadixMap::new();

        m.insert("cad", 5);
        m.insert("abc;0", 1);
        m.insert("c", 4);
        m.insert("abb;0", 2);
        m.insert("ab", 3);

        m
    }

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

    #[test]
    fn test_remove() {
        let mut m = populated_map();

        assert_eq!(m.len(), 5);
        assert_eq!(m.get("ab").unwrap(), &3);
        assert_eq!(m.get("abc;0").unwrap(), &1);
        assert_eq!(m.get("abb;0").unwrap(), &2);
        assert_eq!(m.get("c").unwrap(), &4);
        assert_eq!(m.get("cad").unwrap(), &5);

        assert_eq!(m.remove("ab"), Some(3));
        assert_eq!(m.len(), 4);
        assert!(m.get("ab").is_none());

        assert_eq!(m.remove("cad"), Some(5));
        assert_eq!(m.len(), 3);
        assert!(m.get("cad").is_none());

        assert_eq!(m.remove("cad"), None);
        assert_eq!(m.remove("foobar"), None);

        assert_eq!(m.get("abc;0").unwrap(), &1);
        assert_eq!(m.get("abb;0").unwrap(), &2);
        assert_eq!(m.get("c").unwrap(), &4);
    }

    #[test]
    fn test_iter() {
        let m = populated_map();

        let mut it = m.iter();

        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"ab");
        assert_eq!(v, &3);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abb;0");
        assert_eq!(v, &2);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abc;0");
        assert_eq!(v, &1);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"c");
        assert_eq!(v, &4);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"cad");
        assert_eq!(v, &5);

        assert_eq!(it.next(), None);
    }

    #[test]
    fn test_iter_mut() {
        let mut m = populated_map();

        let mut it = m.iter_mut();

        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"ab");
        assert_eq!(v, &mut 3);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abb;0");
        assert_eq!(v, &mut 2);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abc;0");
        assert_eq!(v, &mut 1);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"c");
        assert_eq!(v, &mut 4);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"cad");
        assert_eq!(v, &mut 5);

        assert_eq!(it.next(), None);

        // Modify second item
        assert_eq!(m.get("cad"), Some(&5));
        assert_eq!(m.get("abc;0"), Some(&1));
        assert_eq!(m.get("c"), Some(&4));
        assert_eq!(m.get("abb;0"), Some(&2));
        assert_eq!(m.get("ab"), Some(&3));

        let mut it = m.iter_mut();

        let _ = it.next().unwrap();
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abb;0");
        assert_eq!(v, &mut 2);
        *v = 100;

        for _ in 0..3 {
            let _ = it.next().unwrap();
        }
        assert_eq!(it.next(), None);

        assert_eq!(m.get("cad"), Some(&5));
        assert_eq!(m.get("abc;0"), Some(&1));
        assert_eq!(m.get("c"), Some(&4));
        assert_eq!(m.get("abb;0"), Some(&100));
        assert_eq!(m.get("ab"), Some(&3));
    }

    #[test]
    fn test_prefix_iter() {
        let m = populated_map();

        let mut it = m.prefix_iter(b"ab");
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"ab");
        assert_eq!(v, &3);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abb;0");
        assert_eq!(v, &2);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abc;0");
        assert_eq!(v, &1);
        assert_eq!(it.next(), None);

        let mut it = m.prefix_iter(b"abb");
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abb;0");
        assert_eq!(v, &2);
        assert_eq!(it.next(), None);

        let mut it = m.prefix_iter(b"c");
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"c");
        assert_eq!(v, &4);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"cad");
        assert_eq!(v, &5);
        assert_eq!(it.next(), None);

        let mut it = m.prefix_iter(b"ca");
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"cad");
        assert_eq!(v, &5);
        assert_eq!(it.next(), None);

        let mut it = m.prefix_iter(b"cad");
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"cad");
        assert_eq!(v, &5);
        assert_eq!(it.next(), None);

        let mut it = m.prefix_iter(b"cada");
        assert_eq!(it.next(), None);

        let mut it = m.prefix_iter(b"abd");
        assert_eq!(it.next(), None);
    }

    #[test]
    fn test_prefix_iter_mut() {
        let mut m = populated_map();

        let mut it = m.prefix_iter_mut(b"ab");
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"ab");
        assert_eq!(v, &mut 3);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abb;0");
        assert_eq!(v, &mut 2);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abc;0");
        assert_eq!(v, &mut 1);
        assert_eq!(it.next(), None);

        let mut it = m.prefix_iter_mut(b"abb");
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"abb;0");
        assert_eq!(v, &mut 2);
        assert_eq!(it.next(), None);

        let mut it = m.prefix_iter_mut(b"c");
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"c");
        assert_eq!(v, &mut 4);
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"cad");
        assert_eq!(v, &mut 5);
        assert_eq!(it.next(), None);

        let mut it = m.prefix_iter_mut(b"ca");
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"cad");
        assert_eq!(v, &mut 5);
        assert_eq!(it.next(), None);

        let mut it = m.prefix_iter_mut(b"cad");
        let (k, v) = it.next().unwrap();
        assert_eq!(k.as_ref(), b"cad");
        assert_eq!(v, &mut 5);
        assert_eq!(it.next(), None);

        let mut it = m.prefix_iter_mut(b"cada");
        assert_eq!(it.next(), None);

        let mut it = m.prefix_iter_mut(b"abd");
        assert_eq!(it.next(), None);
    }

    #[test]
    fn test_range() {
        let mut m = RadixMap::new();
        m.insert("aa", 1);
        m.insert("ab", 2);
        m.insert("ac", 3);
        m.insert("ad", 4);
        m.insert("ba", 5);
        m.insert("bb", 6);
        m.insert("bc", 7);
        m.insert("bd", 8);

        assert_eq!(m.range::<&[u8], _>(..).count(), 8);
        assert_eq!(m.range("a"..).count(), 8);
        assert_eq!(m.range("a".."b").count(), 4);
        assert_eq!(m.range("a"..="b").count(), 4);
        assert_eq!(m.range("a"..="ba").count(), 5);
        assert_eq!(m.range("ab"..="ba").count(), 4);
        assert_eq!(m.range("ae"..).count(), 4);

        for ((k1, v1), (k2, v2)) in m.range("a"..).zip(m.iter().take(4)) {
            assert_eq!(k1.as_ref(), k2.as_ref());
            assert_eq!(*v1, *v2);
        }

        for ((k1, v1), (k2, v2)) in m.range("ab"..="ba").zip(m.iter().skip(1).take(4)) {
            assert_eq!(k1.as_ref(), k2.as_ref());
            assert_eq!(*v1, *v2);
        }

        assert_eq!(m.range("b"..).count(), 4);
        assert_eq!(m.range("b".."b").count(), 0);
        assert_eq!(m.range("b"..="b").count(), 0);
        assert_eq!(m.range("b"..="be").count(), 4);
        assert_eq!(m.range("bb".."bc").count(), 1);
        assert_eq!(m.range("bb"..="bc").count(), 2);

        for ((k1, v1), (k2, v2)) in m.range("bb"..="bc").zip(m.iter().skip(5).take(2)) {
            assert_eq!(k1.as_ref(), k2.as_ref());
            assert_eq!(*v1, *v2);
        }

        assert_eq!(m.range("be"..).count(), 0);
        assert_eq!(m.range("c"..).count(), 0);
    }

    #[test]
    fn test_range_mut() {
        let mut m = RadixMap::new();
        m.insert("aa", 1);
        m.insert("ab", 2);
        m.insert("ac", 3);
        m.insert("ad", 4);
        m.insert("ba", 5);
        m.insert("bb", 6);
        m.insert("bc", 7);
        m.insert("bd", 8);

        assert_eq!(m.range_mut::<&[u8], _>(..).count(), 8);
        assert_eq!(m.range_mut("a"..).count(), 8);
        assert_eq!(m.range_mut("a".."b").count(), 4);
        assert_eq!(m.range_mut("a"..="b").count(), 4);
        assert_eq!(m.range_mut("a"..="ba").count(), 5);
        assert_eq!(m.range_mut("ab"..="ba").count(), 4);
        assert_eq!(m.range_mut("ae"..).count(), 4);

        for ((k1, v1), (k2, v2)) in m.range("a"..).zip(m.iter().take(4)) {
            assert_eq!(k1.as_ref(), k2.as_ref());
            assert_eq!(*v1, *v2);
        }

        for ((k1, v1), (k2, v2)) in m.range("ab"..="ba").zip(m.iter().skip(1).take(4)) {
            assert_eq!(k1.as_ref(), k2.as_ref());
            assert_eq!(*v1, *v2);
        }

        assert_eq!(m.range_mut("b"..).count(), 4);
        assert_eq!(m.range_mut("b".."b").count(), 0);
        assert_eq!(m.range_mut("b"..="b").count(), 0);
        assert_eq!(m.range_mut("b"..="be").count(), 4);
        assert_eq!(m.range_mut("bb".."bc").count(), 1);
        assert_eq!(m.range_mut("bb"..="bc").count(), 2);

        for ((k1, v1), (k2, v2)) in m.range("bb"..="bc").zip(m.iter().skip(5).take(2)) {
            assert_eq!(k1.as_ref(), k2.as_ref());
            assert_eq!(*v1, *v2);
        }

        assert_eq!(m.range_mut("be"..).count(), 0);
        assert_eq!(m.range_mut("c"..).count(), 0);

        let (_, v) = m.range_mut("bb".."bc").next().unwrap();
        *v = 66;

        assert_eq!(m.get("bb"), Some(&66));
    }
}

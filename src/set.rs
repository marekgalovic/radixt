use crate::iter::{Iter, MapK, MapV};
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

    #[inline(always)]
    pub fn intersection<'a, 'b>(&'a self, other: &'b RadixSet) -> Intersection<'a, 'b> {
        Intersection::new(self, other)
    }

    #[inline(always)]
    pub fn union<'a, 'b>(&'a self, other: &'b RadixSet) -> Union<'a, 'b> {
        Union::new(self, other)
    }
}

pub struct Intersection<'a, 'b> {
    left: Iter<'a, (), MapV<'a, ()>>,
    right: Iter<'b, (), MapV<'b, ()>>,
}

impl<'a, 'b> Intersection<'a, 'b> {
    fn new(left: &'a RadixSet, right: &'b RadixSet) -> Self {
        Intersection {
            left: Iter::new(Some(left.inner.root()), vec![]),
            right: Iter::new(Some(right.inner.root()), vec![]),
        }
    }
}

impl<'a, 'b> Iterator for Intersection<'a, 'b> {
    type Item = Box<[u8]>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.left.next().is_none() {
            return None;
        }
        if self.right.next().is_none() {
            return None;
        }
        let mut lk = self.left.curr_key();
        let mut rk = self.right.curr_key();
        // Advance the left iterator until it's key is smaller than
        // right's iterator key.
        while lk < rk {
            if self.left.next().is_none() {
                return None;
            }
            lk = self.left.curr_key();
        }
        // Advance the right iterator until it's key is smaller than
        // left's iterator key.
        while rk < lk {
            if self.right.next().is_none() {
                return None;
            }
            rk = self.right.curr_key();
        }
        Some(lk.into())
    }
}

pub struct Union<'a, 'b> {
    left: Iter<'a, (), MapV<'a, ()>>,
    left_key: Option<Box<[u8]>>,
    right: Iter<'b, (), MapV<'b, ()>>,
    right_key: Option<Box<[u8]>>,
}

impl<'a, 'b> Union<'a, 'b> {
    fn new(left: &'a RadixSet, right: &'b RadixSet) -> Self {
        Union {
            left: Iter::new(Some(left.inner.root()), vec![]),
            left_key: None,
            right: Iter::new(Some(right.inner.root()), vec![]),
            right_key: None,
        }
    }
}

impl<'a, 'b> Iterator for Union<'a, 'b> {
    type Item = Box<[u8]>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(lk) = self.left_key.as_ref() {
            if self.right.next().is_some() {
                let rk = self.right.curr_key();
                if rk < lk {
                    return Some(rk.into());
                }
                if rk > lk {
                    self.right_key = Some(rk.into());
                }
            }
            return Some(self.left_key.take().unwrap());
        }
        if let Some(rk) = self.right_key.as_ref() {
            if self.left.next().is_some() {
                let lk = self.left.curr_key();
                if lk < rk {
                    return Some(lk.into());
                }
                if lk > rk {
                    self.left_key = Some(lk.into());
                }
            }
            return Some(self.right_key.take().unwrap());
        }

        if self.left.next().is_none() {
            if self.right.next().is_some() {
                return Some(self.right.curr_key().into());
            }
            return None;
        }
        let lk = self.left.curr_key();

        if self.right.next().is_none() {
            return Some(lk.into());
        }
        let rk = self.right.curr_key();

        if lk < rk {
            self.right_key = Some(rk.into());
            return Some(lk.into());
        }
        if rk < lk {
            self.left_key = Some(lk.into());
            return Some(rk.into());
        }
        Some(lk.into())
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

    #[test]
    fn test_intersection_partial() {
        // Left then right
        let mut left = RadixSet::new();
        left.insert("aa");
        left.insert("ab");
        left.insert("ac");

        let mut right = RadixSet::new();
        right.insert("ab");
        right.insert("ac");
        right.insert("ad");

        let intersection: Vec<Box<[u8]>> = left.intersection(&right).collect();
        assert_eq!(intersection.len(), 2);
        assert_eq!(intersection[0].as_ref(), b"ab");
        assert_eq!(intersection[1].as_ref(), b"ac");

        // Left right then left
        let intersection: Vec<Box<[u8]>> = right.intersection(&left).collect();
        assert_eq!(intersection.len(), 2);
        assert_eq!(intersection[0].as_ref(), b"ab");
        assert_eq!(intersection[1].as_ref(), b"ac");
    }

    #[test]
    fn test_intersection_full() {
        // Left then right
        let mut left = RadixSet::new();
        left.insert("aa");
        left.insert("ab");
        left.insert("ac");

        let mut right = RadixSet::new();
        right.insert("aa");
        right.insert("ab");
        right.insert("ac");

        let intersection: Vec<Box<[u8]>> = left.intersection(&right).collect();
        assert_eq!(intersection.len(), 3);
        assert_eq!(intersection[0].as_ref(), b"aa");
        assert_eq!(intersection[1].as_ref(), b"ab");
        assert_eq!(intersection[2].as_ref(), b"ac");

        let intersection: Vec<Box<[u8]>> = right.intersection(&left).collect();
        assert_eq!(intersection.len(), 3);
        assert_eq!(intersection[0].as_ref(), b"aa");
        assert_eq!(intersection[1].as_ref(), b"ab");
        assert_eq!(intersection[2].as_ref(), b"ac");
    }

    #[test]
    fn test_intersection_empty() {
        // Left then right
        let mut left = RadixSet::new();
        left.insert("aa");
        left.insert("ab");
        left.insert("ac");

        let mut right = RadixSet::new();
        right.insert("ad");
        right.insert("ae");
        right.insert("af");

        let intersection: Vec<Box<[u8]>> = left.intersection(&right).collect();
        assert_eq!(intersection.len(), 0);

        let intersection: Vec<Box<[u8]>> = right.intersection(&left).collect();
        assert_eq!(intersection.len(), 0);
    }

    #[test]
    fn test_union_partial_overlap() {
        let mut left = RadixSet::new();
        left.insert("aa");
        left.insert("ab");
        left.insert("ac");

        let mut right = RadixSet::new();
        right.insert("ab");
        right.insert("ac");
        right.insert("ad");

        let union: Vec<Box<[u8]>> = left.union(&right).collect();
        assert_eq!(union.len(), 4);
        assert_eq!(union[0].as_ref(), b"aa");
        assert_eq!(union[1].as_ref(), b"ab");
        assert_eq!(union[2].as_ref(), b"ac");
        assert_eq!(union[3].as_ref(), b"ad");

        let union: Vec<Box<[u8]>> = right.union(&left).collect();
        assert_eq!(union.len(), 4);
        assert_eq!(union[0].as_ref(), b"aa");
        assert_eq!(union[1].as_ref(), b"ab");
        assert_eq!(union[2].as_ref(), b"ac");
        assert_eq!(union[3].as_ref(), b"ad");
    }

    #[test]
    fn test_union_interleaved() {
        let mut left = RadixSet::new();
        left.insert("aa");
        left.insert("ac");
        left.insert("ae");

        let mut right = RadixSet::new();
        right.insert("ab");
        right.insert("ad");
        right.insert("af");

        let union: Vec<Box<[u8]>> = left.union(&right).collect();
        assert_eq!(union.len(), 6);
        assert_eq!(union[0].as_ref(), b"aa");
        assert_eq!(union[1].as_ref(), b"ab");
        assert_eq!(union[2].as_ref(), b"ac");
        assert_eq!(union[3].as_ref(), b"ad");
        assert_eq!(union[4].as_ref(), b"ae");
        assert_eq!(union[5].as_ref(), b"af");

        let union: Vec<Box<[u8]>> = right.union(&left).collect();
        assert_eq!(union.len(), 6);
        assert_eq!(union[0].as_ref(), b"aa");
        assert_eq!(union[1].as_ref(), b"ab");
        assert_eq!(union[2].as_ref(), b"ac");
        assert_eq!(union[3].as_ref(), b"ad");
        assert_eq!(union[4].as_ref(), b"ae");
        assert_eq!(union[5].as_ref(), b"af");
    }

    #[test]
    fn test_union_full_overlap() {
        let mut left = RadixSet::new();
        left.insert("aa");
        left.insert("ab");
        left.insert("ac");

        let mut right = RadixSet::new();
        right.insert("aa");
        right.insert("ab");
        right.insert("ac");

        let union: Vec<Box<[u8]>> = left.union(&right).collect();
        assert_eq!(union.len(), 3);
        assert_eq!(union[0].as_ref(), b"aa");
        assert_eq!(union[1].as_ref(), b"ab");
        assert_eq!(union[2].as_ref(), b"ac");
    }

    #[test]
    fn test_union_no_overlap() {
        let mut left = RadixSet::new();
        left.insert("aa");
        left.insert("ab");
        left.insert("ac");

        let mut right = RadixSet::new();
        right.insert("ae");
        right.insert("af");
        right.insert("ag");

        let union: Vec<Box<[u8]>> = left.union(&right).collect();
        assert_eq!(union.len(), 6);
        assert_eq!(union[0].as_ref(), b"aa");
        assert_eq!(union[1].as_ref(), b"ab");
        assert_eq!(union[2].as_ref(), b"ac");
        assert_eq!(union[3].as_ref(), b"ae");
        assert_eq!(union[4].as_ref(), b"af");
        assert_eq!(union[5].as_ref(), b"ag");

        let union: Vec<Box<[u8]>> = right.union(&left).collect();
        assert_eq!(union.len(), 6);
        assert_eq!(union[0].as_ref(), b"aa");
        assert_eq!(union[1].as_ref(), b"ab");
        assert_eq!(union[2].as_ref(), b"ac");
        assert_eq!(union[3].as_ref(), b"ae");
        assert_eq!(union[4].as_ref(), b"af");
        assert_eq!(union[5].as_ref(), b"ag");

        left.insert("ad");

        let union: Vec<Box<[u8]>> = left.union(&right).collect();
        assert_eq!(union.len(), 7);
        assert_eq!(union[0].as_ref(), b"aa");
        assert_eq!(union[1].as_ref(), b"ab");
        assert_eq!(union[2].as_ref(), b"ac");
        assert_eq!(union[3].as_ref(), b"ad");
        assert_eq!(union[4].as_ref(), b"ae");
        assert_eq!(union[5].as_ref(), b"af");
        assert_eq!(union[6].as_ref(), b"ag");

        let union: Vec<Box<[u8]>> = right.union(&left).collect();
        assert_eq!(union.len(), 7);
        assert_eq!(union[0].as_ref(), b"aa");
        assert_eq!(union[1].as_ref(), b"ab");
        assert_eq!(union[2].as_ref(), b"ac");
        assert_eq!(union[3].as_ref(), b"ad");
        assert_eq!(union[4].as_ref(), b"ae");
        assert_eq!(union[5].as_ref(), b"af");
        assert_eq!(union[6].as_ref(), b"ag");
    }
}

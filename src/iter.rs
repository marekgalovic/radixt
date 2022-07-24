use std::{
    marker::PhantomData,
    ops::{Bound, RangeBounds},
};

use crate::node::Node;

pub trait IterMap<'a, T> {
    type Output;

    fn map(prefix: &[u8], value: &'a T) -> Self::Output;
}

pub trait IterMapMut<'a, T> {
    type Output;

    fn map(prefix: &[u8], value: &'a mut T) -> Self::Output;
}

pub struct Iter<'a, T, M: IterMap<'a, T>> {
    stack: Vec<(usize, &'a Node<T>)>,
    prefix: Vec<u8>,
    _marker: std::marker::PhantomData<M>,
}

impl<'a, T, M: IterMap<'a, T>> Iter<'a, T, M> {
    pub(crate) fn new(root: Option<&'a Node<T>>, prefix: Vec<u8>) -> Self {
        let stack = match root {
            Some(root) => vec![(prefix.len(), root)],
            None => vec![],
        };

        Iter {
            stack,
            prefix,
            _marker: PhantomData::default(),
        }
    }

    /// Returns a reference to current key.
    /// This key is only valid until .next() is called again.
    pub(crate) fn curr_key(&self) -> &[u8] {
        &self.prefix
    }
}

impl<'a, T, M: IterMap<'a, T>> Iterator for Iter<'a, T, M> {
    type Item = M::Output;

    fn next(&mut self) -> Option<Self::Item> {
        match self.stack.pop() {
            Some((prefix_len, node)) => {
                // Update prefix
                self.prefix.truncate(prefix_len);
                self.prefix.extend(node.key());

                // Push node's children to stack
                for child in node.children().iter().rev() {
                    self.stack.push((self.prefix.len(), child));
                }

                // Return value
                match node.value() {
                    Some(v) => Some(M::map(&self.prefix, v)),
                    None => self.next(),
                }
            }
            None => None,
        }
    }
}

pub struct IterMut<'a, T, M: IterMapMut<'a, T>> {
    stack: Vec<(usize, &'a mut Node<T>)>,
    prefix: Vec<u8>,
    _marker: std::marker::PhantomData<M>,
}

impl<'a, T, M: IterMapMut<'a, T>> IterMut<'a, T, M> {
    pub(crate) fn new(root: Option<&'a mut Node<T>>, prefix: Vec<u8>) -> Self {
        let stack = match root {
            Some(root) => vec![(prefix.len(), root)],
            None => vec![],
        };

        IterMut {
            stack,
            prefix,
            _marker: PhantomData::default(),
        }
    }

    /// Returns a reference to current key.
    /// This key is only valid until .next() is called again.
    fn curr_key(&self) -> &[u8] {
        &self.prefix
    }
}

impl<'a, T, M: IterMapMut<'a, T>> Iterator for IterMut<'a, T, M> {
    type Item = M::Output;

    fn next(&mut self) -> Option<Self::Item> {
        match self.stack.pop() {
            Some((prefix_len, node)) => {
                // Update prefix
                self.prefix.truncate(prefix_len);
                self.prefix.extend(node.key());

                let value = node.value_mut().map(|v| v as *mut T);

                // Push node's children to stack
                for child in node.children_mut().iter_mut().rev() {
                    self.stack.push((self.prefix.len(), child));
                }

                // Return value
                match value {
                    Some(v) => Some(M::map(&self.prefix, unsafe {
                        // SAFETY
                        // We are giving out mutable references to node's value
                        // while holding a mutable reference to the node itself
                        // so this is OK.
                        &mut *v
                    })),
                    None => self.next(),
                }
            }
            None => None,
        }
    }
}

pub struct IntoIter<T> {
    stack: Vec<(usize, Node<T>)>,
    prefix: Vec<u8>,
}

impl<T> IntoIter<T> {
    pub(crate) fn new(root: Node<T>) -> Self {
        IntoIter {
            stack: vec![(0, root)],
            prefix: vec![],
        }
    }
}

impl<T> Iterator for IntoIter<T> {
    type Item = (Box<[u8]>, T);

    fn next(&mut self) -> Option<Self::Item> {
        match self.stack.pop() {
            Some((prefix_len, mut node)) => {
                // Update prefix
                self.prefix.truncate(prefix_len);
                self.prefix.extend(node.key());

                // Push node's children to stack
                for child in node.take_children().rev() {
                    self.stack.push((self.prefix.len(), child));
                }

                // Return value
                match node.take_value() {
                    Some(v) => Some((self.prefix.as_slice().into(), v)),
                    None => self.next(),
                }
            }
            None => None,
        }
    }
}

#[inline(always)]
fn in_range_left<K: AsRef<[u8]>>(bound: Bound<&K>, key: &[u8]) -> bool {
    match bound {
        Bound::Excluded(lb) => lb.as_ref() < key,
        Bound::Included(lb) => lb.as_ref() <= key,
        Bound::Unbounded => true,
    }
}

#[inline(always)]
fn in_range_right<K: AsRef<[u8]>>(bound: Bound<&K>, key: &[u8]) -> bool {
    match bound {
        Bound::Excluded(ub) => ub.as_ref() > key,
        Bound::Included(ub) => ub.as_ref() >= key,
        Bound::Unbounded => true,
    }
}

pub struct Range<'a, T, K: AsRef<[u8]>, B: RangeBounds<K>> {
    iter: Iter<'a, T, MapV<'a, T>>,
    bounds: B,
    done: bool,
    _marker: PhantomData<K>,
}

impl<'a, T, K: AsRef<[u8]>, B: RangeBounds<K>> Range<'a, T, K, B> {
    pub(crate) fn new(iter: Iter<'a, T, MapV<'a, T>>, bounds: B) -> Self {
        Range {
            iter,
            bounds,
            done: false,
            _marker: PhantomData::default(),
        }
    }
}

impl<'a, T, K: AsRef<[u8]>, B: RangeBounds<K>> Iterator for Range<'a, T, K, B> {
    type Item = <MapKV<'a, T> as IterMap<'a, T>>::Output;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        match self.iter.next() {
            Some(v) => {
                let k = self.iter.curr_key();
                if !in_range_left(self.bounds.start_bound(), k) {
                    return self.next();
                }
                if !in_range_right(self.bounds.end_bound(), k) {
                    self.done = true;
                    return None;
                }

                Some(MapKV::map(k, v))
            }
            None => None,
        }
    }
}

pub struct RangeMut<'a, T, K: AsRef<[u8]>, B: RangeBounds<K>> {
    iter: IterMut<'a, T, MapVMut<'a, T>>,
    bounds: B,
    done: bool,
    _marker: PhantomData<K>,
}

impl<'a, T, K: AsRef<[u8]>, B: RangeBounds<K>> RangeMut<'a, T, K, B> {
    pub(crate) fn new(iter: IterMut<'a, T, MapVMut<'a, T>>, bounds: B) -> Self {
        RangeMut {
            iter,
            bounds,
            done: false,
            _marker: PhantomData::default(),
        }
    }
}

impl<'a, T, K: AsRef<[u8]>, B: RangeBounds<K>> Iterator for RangeMut<'a, T, K, B> {
    type Item = <MapKVMut<'a, T> as IterMapMut<'a, T>>::Output;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        match self.iter.next() {
            Some(v) => {
                let k = self.iter.curr_key();
                if !in_range_left(self.bounds.start_bound(), k) {
                    return self.next();
                }
                if !in_range_right(self.bounds.end_bound(), k) {
                    self.done = true;
                    return None;
                }

                Some(MapKVMut::map(k, v))
            }
            None => None,
        }
    }
}

pub struct MapKV<'a, T> {
    _marker: PhantomData<&'a T>,
}

impl<'a, T> IterMap<'a, T> for MapKV<'a, T> {
    type Output = (Box<[u8]>, &'a T);

    #[inline(always)]
    fn map(prefix: &[u8], value: &'a T) -> Self::Output {
        (prefix.into(), value)
    }
}

pub struct MapV<'a, T> {
    _marker: PhantomData<&'a T>,
}

impl<'a, T> IterMap<'a, T> for MapV<'a, T> {
    type Output = &'a T;

    #[inline(always)]
    fn map(_prefix: &[u8], value: &'a T) -> Self::Output {
        value
    }
}

pub struct MapK<'a, T> {
    _marker: PhantomData<&'a T>,
}

impl<'a, T> IterMap<'a, T> for MapK<'a, T> {
    type Output = Box<[u8]>;

    #[inline(always)]
    fn map(prefix: &[u8], _value: &'a T) -> Self::Output {
        prefix.into()
    }
}

pub struct MapKVMut<'a, T> {
    _marker: PhantomData<&'a T>,
}

impl<'a, T> IterMapMut<'a, T> for MapKVMut<'a, T> {
    type Output = (Box<[u8]>, &'a mut T);

    #[inline(always)]
    fn map(prefix: &[u8], value: &'a mut T) -> Self::Output {
        (prefix.into(), value)
    }
}

pub struct MapVMut<'a, T> {
    _marker: PhantomData<&'a T>,
}

impl<'a, T> IterMapMut<'a, T> for MapVMut<'a, T> {
    type Output = &'a mut T;

    #[inline(always)]
    fn map(_prefix: &[u8], value: &'a mut T) -> Self::Output {
        value
    }
}

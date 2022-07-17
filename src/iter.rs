use std::marker::PhantomData;

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
    pub(crate) fn new(root: Option<&'a Node<T>>, prefix: &[u8]) -> Self {
        let stack = match root {
            Some(root) => root
                .children()
                .iter()
                .rev()
                .map(|e| (prefix.len(), e))
                .collect(),
            None => vec![],
        };

        Iter {
            stack,
            prefix: prefix.to_vec(),
            _marker: PhantomData::default(),
        }
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
    pub(crate) fn new(root: Option<&'a mut Node<T>>, prefix: &[u8]) -> Self {
        let stack = match root {
            Some(root) => root
                .children_mut()
                .iter_mut()
                .rev()
                .map(|e| (prefix.len(), e))
                .collect(),
            None => vec![],
        };

        IterMut {
            stack,
            prefix: prefix.to_vec(),
            _marker: PhantomData::default(),
        }
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

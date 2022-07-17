use crate::map::RadixMap;
use crate::node::Node;

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

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline(always)]
    pub fn insert<K: AsRef<[u8]>>(&mut self, key: K) -> bool {
        self.inner.insert(key.as_ref(), ()).is_none()
    }

    #[inline(always)]
    pub fn remove<K: AsRef<[u8]>>(&mut self, key: K) -> bool {
        self.inner.remove(key.as_ref()).is_some()
    }

    #[inline(always)]
    pub fn contains<K: AsRef<[u8]>>(&self, key: K) -> bool {
        self.inner.contains_key(key)
    }

    #[inline(always)]
    pub fn iter(&self) -> RadixSetIter<'_> {
        RadixSetIter::new(&self.inner.root())
    }
}

pub struct RadixSetIter<'a> {
    stack: Vec<(usize, &'a Node<()>)>,
    prefix: Vec<u8>,
}

impl<'a> RadixSetIter<'a> {
    fn new(root: &'a Node<()>) -> Self {
        let prefix = root.key().to_vec();
        let stack = root
            .children()
            .iter()
            .rev()
            .map(|e| (prefix.len(), e))
            .collect();

        RadixSetIter { stack, prefix }
    }
}

impl<'a> Iterator for RadixSetIter<'a> {
    type Item = Vec<u8>;

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
                    Some(_) => Some(self.prefix.clone()),
                    None => self.next(),
                }
            }
            None => None,
        }
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

    #[test]
    fn test_iter() {
        let mut set = RadixSet::new();

        set.insert("cad");
        set.insert("abc;0");
        set.insert("c");
        set.insert("abb;0");
        set.insert("ab");

        let mut it = set.iter();

        let k = it.next().unwrap();
        assert_eq!(k, b"ab");
        let k = it.next().unwrap();
        assert_eq!(k, b"abb;0");
        let k = it.next().unwrap();
        assert_eq!(k, b"abc;0");
        let k = it.next().unwrap();
        assert_eq!(k, b"c");
        let k = it.next().unwrap();
        assert_eq!(k, b"cad");

        assert_eq!(it.next(), None);
    }
}

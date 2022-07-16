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

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.size
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline(always)]
    pub fn insert<K: AsRef<[u8]>>(&mut self, key: K, value: T) -> Option<T> {
        let old = self.root.insert(key.as_ref(), value);
        self.size += old.is_none() as usize;
        old
    }

    #[inline(always)]
    pub fn remove<K: AsRef<[u8]>>(&mut self, key: K) -> Option<T> {
        let removed = self.root.remove(key.as_ref());
        self.size -= removed.is_some() as usize;
        removed
    }

    #[inline(always)]
    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> Option<&T> {
        self.root.get(key.as_ref())
    }

    #[inline(always)]
    pub fn get_mut<K: AsRef<[u8]>>(&mut self, key: K) -> Option<&mut T> {
        self.root.get_mut(key.as_ref())
    }

    #[inline(always)]
    pub fn iter(&self) -> RadixMapIter<'_, T> {
        RadixMapIter::new(&self.root)
    }
}

pub struct RadixMapIter<'a, T> {
    stack: Vec<(usize, &'a Node<T>)>,
    prefix: Vec<u8>,
}

impl<'a, T> RadixMapIter<'a, T> {
    fn new(root: &'a Node<T>) -> Self {
        let prefix = root.key().to_vec();
        let stack = root
            .children()
            .iter()
            .rev()
            .map(|e| (prefix.len(), e))
            .collect();

        RadixMapIter { stack, prefix }
    }
}

impl<'a, T> Iterator for RadixMapIter<'a, T> {
    type Item = (Vec<u8>, &'a T);

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
                    Some(v) => Some((self.prefix.clone(), v)),
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
    fn test_iter() {
        let mut m = RadixMap::new();

        m.insert("cad", 5);
        m.insert("abc;0", 1);
        m.insert("c", 4);
        m.insert("abb;0", 2);
        m.insert("ab", 3);

        let mut it = m.iter();

        let (k, v) = it.next().unwrap();
        assert_eq!(k, b"ab");
        assert_eq!(v, &3);
        let (k, v) = it.next().unwrap();
        assert_eq!(k, b"abb;0");
        assert_eq!(v, &2);
        let (k, v) = it.next().unwrap();
        assert_eq!(k, b"abc;0");
        assert_eq!(v, &1);
        let (k, v) = it.next().unwrap();
        assert_eq!(k, b"c");
        assert_eq!(v, &4);
        let (k, v) = it.next().unwrap();
        assert_eq!(k, b"cad");
        assert_eq!(v, &5);

        assert_eq!(it.next(), None);
    }
}

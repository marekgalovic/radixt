use crate::node::Node;

pub(crate) struct NodeIter<'a, V> {
    stack: Vec<&'a Node<V>>,
}

impl<'a, V> NodeIter<'a, V> {
    pub(crate) fn new(root: &'a Node<V>) -> Self {
        NodeIter { stack: vec![root] }
    }
}

impl<'a, V> Iterator for NodeIter<'a, V> {
    type Item = &'a Node<V>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.stack.pop() {
            Some(node) => {
                for child in node.children().iter().rev() {
                    self.stack.push(child);
                }
                Some(node)
            }
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    // use super::*;
    // TODO:
}

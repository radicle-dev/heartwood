use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    fmt,
    hash::Hash,
    ops::{Deref, Index},
};

/// A node in the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node<K: Eq + Hash, V> {
    /// The node value, stored by the user.
    pub value: V,
    /// Nodes depended on.
    pub dependencies: HashSet<K>,
    /// Nodes depending on this node.
    pub dependents: HashSet<K>,
}

impl<K: Eq + Hash, V> Node<K, V> {
    fn new(value: V) -> Self {
        Self {
            value,
            dependencies: HashSet::new(),
            dependents: HashSet::new(),
        }
    }
}

impl<K: Eq + Hash, V> Borrow<V> for &Node<K, V> {
    fn borrow(&self) -> &V {
        &self.value
    }
}

impl<K: Eq + Hash, V> Deref for Node<K, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// A directed acyclic graph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dag<K: Eq + Hash, V> {
    graph: HashMap<K, Node<K, V>>,
    tips: HashSet<K>,
    roots: HashSet<K>,
}

impl<K: Eq + Copy + Hash, V> Dag<K, V> {
    /// Create a new empty DAG.
    pub fn new() -> Self {
        Self {
            graph: HashMap::new(),
            tips: HashSet::new(),
            roots: HashSet::new(),
        }
    }

    pub fn root(key: K, value: V) -> Self {
        Self {
            graph: HashMap::from_iter([(key, Node::new(value))]),
            tips: HashSet::from_iter([key]),
            roots: HashSet::from_iter([key]),
        }
    }

    /// Check whether there are any nodes in the graph.
    pub fn is_empty(&self) -> bool {
        self.graph.is_empty()
    }

    /// Return the number of nodes in the graph.
    pub fn len(&self) -> usize {
        self.graph.len()
    }

    /// Add a node to the graph.
    pub fn node(&mut self, key: K, value: V) -> Option<Node<K, V>> {
        self.tips.insert(key);
        self.roots.insert(key);
        self.graph.insert(
            key,
            Node {
                value,
                dependencies: HashSet::new(),
                dependents: HashSet::new(),
            },
        )
    }

    /// Add a dependency from one node to the other.
    pub fn dependency(&mut self, from: K, to: K) {
        if let Some(node) = self.graph.get_mut(&from) {
            node.dependencies.insert(to);
            self.roots.remove(&from);
        }
        if let Some(node) = self.graph.get_mut(&to) {
            node.dependents.insert(from);
            self.tips.remove(&to);
        }
    }

    /// Get a node.
    pub fn get(&self, key: &K) -> Option<&Node<K, V>> {
        self.graph.get(key)
    }

    /// Check whether there is a dependency between two nodes.
    pub fn has_dependency(&self, from: &K, to: &K) -> bool {
        self.graph
            .get(from)
            .map(|n| n.dependencies.contains(to))
            .unwrap_or_default()
    }

    /// Get the graph's root nodes, ie. nodes which don't depend on other nodes.
    pub fn roots(&self) -> impl Iterator<Item = (&K, &Node<K, V>)> + '_ {
        self.roots
            .iter()
            .filter_map(|k| self.graph.get(k).map(|n| (k, n)))
    }

    /// Get the graph's tip nodes, ie. nodes which aren't depended on by other nodes.
    pub fn tips(&self) -> impl Iterator<Item = (&K, &Node<K, V>)> + '_ {
        self.tips
            .iter()
            .filter_map(|k| self.graph.get(k).map(|n| (k, n)))
    }

    /// Merge a DAG into this one.
    ///
    /// If a key exists in both graphs, its value is set to that of the other graph.
    pub fn merge(&mut self, other: Self) {
        for k in other.tips.into_iter() {
            self.tips.insert(k);
        }
        for k in other.roots.into_iter() {
            self.roots.insert(k);
        }
        for (k, v) in other.graph.into_iter() {
            self.graph.insert(k, v);
        }
    }

    /// Return a topological ordering of the graph's nodes, using the given RNG.
    /// Graphs with more than one partial order will return an arbitrary topological ordering.
    ///
    /// Calling this function over and over will eventually yield all possible orderings.
    pub fn sorted(&self, rng: fastrand::Rng) -> Vec<K> {
        let mut order = Vec::new(); // Stores the topological order.
        let mut visited = HashSet::new(); // Nodes that have been visited.
        let mut keys = self.graph.keys().collect::<Vec<_>>();

        rng.shuffle(&mut keys);

        for node in keys {
            self.visit(node, &mut visited, &mut order);
        }
        order
    }

    /// Add nodes recursively to the topological order, starting from the given node.
    fn visit(&self, key: &K, visited: &mut HashSet<K>, order: &mut Vec<K>) {
        if visited.contains(key) {
            return;
        }
        visited.insert(*key);

        // Recursively visit all of the node's dependencies.
        if let Some(node) = self.graph.get(key) {
            for dependency in &node.dependencies {
                self.visit(dependency, visited, order);
            }
        }
        // Add the node to the topological order.
        order.push(*key);
    }
}

impl<K: Eq + Copy + Hash + fmt::Debug, V> Index<&K> for Dag<K, V> {
    type Output = Node<K, V>;

    fn index(&self, key: &K) -> &Self::Output {
        self.get(key)
            .unwrap_or_else(|| panic!("Dag::index: node {key:?} not found in graph"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_len() {
        let mut dag = Dag::new();

        dag.node(0, ());
        dag.node(1, ());
        dag.node(2, ());

        assert_eq!(dag.len(), 3);
    }

    #[test]
    fn test_is_empty() {
        let mut dag = Dag::new();
        assert!(dag.is_empty());

        dag.node(0, ());
        assert!(!dag.is_empty());
    }

    #[test]
    fn test_dependencies() {
        let mut dag = Dag::new();

        dag.node(0, ());
        dag.node(1, ());
        dag.dependency(0, 1);

        assert!(dag.has_dependency(&0, &1));
        assert!(!dag.has_dependency(&1, &0));
    }

    #[test]
    fn test_get() {
        let mut dag = Dag::new();

        dag.node(0, "rad");
        dag.node(1, "dar");

        assert_eq!(dag[&0].value, "rad");
        assert_eq!(dag[&1].value, "dar");
        assert!(dag.get(&2).is_none());
    }

    #[test]
    fn test_cycle() {
        let mut dag = Dag::new();

        dag.node(0, ());
        dag.node(1, ());

        dag.dependency(0, 1);
        dag.dependency(1, 0);

        let sorted = dag.sorted(fastrand::Rng::new());
        let expected: &[&[i32]] = &[&[0, 1], &[1, 0]];

        assert!(expected.contains(&sorted.as_slice()));
    }

    #[test]
    fn test_merge() {
        let mut a = Dag::new();
        let mut b = Dag::new();
        let mut c = Dag::new();

        a.node(0, ());
        a.node(1, ());
        a.dependency(1, 0);

        b.node(0, ());
        b.node(2, ());
        b.dependency(2, 0);

        c.merge(a);
        c.merge(b);

        assert!(c.get(&0).is_some());
        assert!(c.get(&1).is_some());
        assert!(c.get(&2).is_some());
        assert!(c.has_dependency(&1, &0));
        assert!(c.has_dependency(&2, &0));
    }

    #[test]
    fn test_diamond() {
        let mut dag = Dag::new();

        dag.node(0, ());
        dag.node(1, ());
        dag.node(2, ());
        dag.node(3, ());

        dag.dependency(1, 0);
        dag.dependency(2, 0);
        dag.dependency(3, 1);
        dag.dependency(3, 2);

        assert_eq!(dag.tips().map(|(k, _)| *k).collect::<Vec<_>>(), vec![3]);
        assert_eq!(dag.roots().map(|(k, _)| *k).collect::<Vec<_>>(), vec![0]);

        // All of the possible sort orders for the above graph.
        let expected: &[&[i32]] = &[&[0, 1, 2, 3], &[0, 2, 1, 3]];
        let actual = dag.sorted(fastrand::Rng::new());

        assert!(expected.contains(&actual.as_slice()), "{actual:?}");
    }

    #[test]
    fn test_complex() {
        let mut dag = Dag::new();

        dag.node(0, ());
        dag.node(1, ());
        dag.node(2, ());
        dag.node(3, ());
        dag.node(4, ());
        dag.node(5, ());

        dag.dependency(3, 2);
        dag.dependency(1, 3);
        dag.dependency(2, 5);
        dag.dependency(0, 5);
        dag.dependency(0, 4);
        dag.dependency(1, 4);

        assert_eq!(
            dag.tips().map(|(k, _)| *k).collect::<HashSet<_>>(),
            HashSet::from_iter([1, 0])
        );
        assert_eq!(
            dag.roots().map(|(k, _)| *k).collect::<HashSet<_>>(),
            HashSet::from_iter([4, 5])
        );

        // All of the possible sort orders for the above graph.
        let expected = &[
            [4, 5, 0, 2, 3, 1],
            [4, 5, 2, 0, 3, 1],
            [4, 5, 2, 3, 0, 1],
            [4, 5, 2, 3, 1, 0],
            [5, 2, 3, 4, 0, 1],
            [5, 2, 3, 4, 1, 0],
            [5, 2, 4, 0, 3, 1],
            [5, 2, 4, 3, 0, 1],
            [5, 2, 4, 3, 1, 0],
            [5, 4, 0, 2, 3, 1],
            [5, 4, 2, 0, 3, 1],
            [5, 4, 2, 3, 0, 1],
            [5, 4, 2, 3, 1, 0],
        ];
        let rng = fastrand::Rng::new();
        let mut sorts = HashSet::new();

        while sorts.len() < expected.len() {
            sorts.insert(dag.sorted(rng.clone()));
        }
        for e in expected {
            assert!(sorts.remove(e.to_vec().as_slice()));
        }
        assert!(sorts.is_empty());
    }
}

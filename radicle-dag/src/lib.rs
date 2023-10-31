//! Directed-acyclic graph implementation.
#![warn(missing_docs)]
use std::{
    borrow::Borrow,
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    fmt::Write,
    ops::{ControlFlow, Deref, Index},
};

/// A node in the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node<K, V> {
    /// The node value, stored by the user.
    pub value: V,
    /// Nodes depended on.
    pub dependencies: BTreeSet<K>,
    /// Nodes depending on this node.
    pub dependents: BTreeSet<K>,
}

impl<K, V> Node<K, V> {
    fn new(value: V) -> Self {
        Self {
            value,
            dependencies: BTreeSet::new(),
            dependents: BTreeSet::new(),
        }
    }
}

impl<K, V> Borrow<V> for &Node<K, V> {
    fn borrow(&self) -> &V {
        &self.value
    }
}

impl<K, V> Deref for Node<K, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// A directed acyclic graph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dag<K, V> {
    graph: BTreeMap<K, Node<K, V>>,
    tips: BTreeSet<K>,
    roots: BTreeSet<K>,
}

impl<K: Ord + Copy, V> Dag<K, V> {
    /// Create a new empty DAG.
    pub fn new() -> Self {
        Self {
            graph: BTreeMap::new(),
            tips: BTreeSet::new(),
            roots: BTreeSet::new(),
        }
    }

    /// Create a DAG with a root node.
    pub fn root(key: K, value: V) -> Self {
        Self {
            graph: BTreeMap::from_iter([(key, Node::new(value))]),
            tips: BTreeSet::from_iter([key]),
            roots: BTreeSet::from_iter([key]),
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
                dependencies: BTreeSet::new(),
                dependents: BTreeSet::new(),
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

    /// Check if the graph contains a node.
    pub fn contains(&self, key: &K) -> bool {
        self.graph.contains_key(key)
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
    pub fn merge(&mut self, mut other: Self) {
        let Some((root, _)) = other.roots().next() else {
            return;
        };
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::<K>::from([*root]);

        while let Some(next) = queue.pop_front() {
            if !visited.insert(next) {
                continue;
            }
            if let Some(node) = other.graph.remove(&next) {
                if !self.contains(&next) {
                    self.node(next, node.value);
                }
                for k in &node.dependents {
                    self.dependency(*k, next);
                }
                for k in &node.dependencies {
                    self.dependency(next, *k);
                }
                queue.extend(node.dependents.iter());
            }
        }
    }

    /// Return a topological ordering of the graph's nodes.
    pub fn sorted(&self) -> VecDeque<K> {
        self.sorted_by(Ord::cmp)
    }

    /// Return a topological ordering of the graph's nodes.
    /// Uses a comparison function to sort partially ordered nodes.
    pub fn sorted_by<F>(&self, mut compare: F) -> VecDeque<K>
    where
        F: FnMut(&K, &K) -> Ordering,
    {
        let mut order = VecDeque::new(); // Stores the topological order.
        let mut visited = BTreeSet::new(); // Nodes that have been visited.
        let mut keys = self.graph.keys().collect::<Vec<_>>();

        // The `visit` function builds the list in reverse order, so we counter-act
        // that here.
        keys.sort_by(|a, b| compare(a, b).reverse());

        for node in keys {
            self.visit(node, &mut visited, &mut order);
        }
        order
    }

    /// Fold over the graph in topological order, pruning branches along the way.
    /// This is a depth-first traversal.
    ///
    /// To continue traversing a branch, return [`ControlFlow::Continue`] from the
    /// filter function. To stop traversal of a branch and prune it,
    /// return [`ControlFlow::Break`].
    pub fn prune<F>(&mut self, roots: &[K], mut filter: F)
    where
        F: for<'r> FnMut(&'r K, &'r Node<K, V>) -> ControlFlow<()>,
    {
        let mut visited = BTreeSet::new();
        let mut result = VecDeque::new();

        for root in roots {
            self.visit(root, &mut visited, &mut result);
        }

        for next in result {
            if let Some(node) = self.graph.get(&next) {
                match filter(&next, node) {
                    ControlFlow::Continue(()) => {}
                    ControlFlow::Break(()) => {
                        // When pruning a node, we remove all transitive dependents on
                        // that node.
                        self.remove(&next);
                    }
                }
            }
        }
    }

    /// Fold over the graph in topological order, skipping certain branches.
    /// This is a depth-first traversal.
    ///
    /// To continue traversing a branch, return [`ControlFlow::Continue`] from the
    /// filter function. To stop traversal of a branch, return [`ControlFlow::Break`].
    pub fn fold<A, F>(&self, roots: &[K], mut acc: A, mut filter: F) -> A
    where
        F: for<'r> FnMut(A, &'r K, &'r Node<K, V>) -> ControlFlow<A, A>,
    {
        let mut visited = BTreeSet::new();
        let mut result = VecDeque::new();
        let mut skip = BTreeSet::new();

        assert!(
            roots.windows(2).all(|w| w[0] < w[1]),
            "The roots must be sorted in ascending order"
        );

        for root in roots.iter().rev() {
            self.visit(root, &mut visited, &mut result);
        }

        for next in result {
            if skip.contains(&next) {
                continue;
            }
            if let Some(node) = self.graph.get(&next) {
                match filter(acc, &next, node) {
                    ControlFlow::Continue(a) => {
                        acc = a;
                    }
                    ControlFlow::Break(a) => {
                        // When filtering out a node, we filter out all transitive dependents on
                        // that node by adding them to the already visited list.
                        skip.extend(self.descendants_of(node));

                        acc = a;
                    }
                }
            }
        }
        acc
    }

    /// Remove a node from the graph, and all its dependents.
    pub fn remove(&mut self, key: &K) -> Option<Node<K, V>> {
        if let Some(node) = self.graph.remove(key) {
            self.tips.remove(key);
            self.roots.remove(key);

            for k in &node.dependencies {
                if let Some(dependency) = self.graph.get_mut(k) {
                    dependency.dependents.remove(key);

                    if dependency.dependents.is_empty() {
                        self.tips.insert(*k);
                    }
                }
            }
            for k in &node.dependents {
                self.remove(k);
            }
            Some(node)
        } else {
            None
        }
    }

    fn descendants_of(&self, from: &Node<K, V>) -> Vec<K> {
        let mut visited = BTreeSet::new();
        let mut stack = VecDeque::new();
        let mut nodes = Vec::new();

        stack.extend(from.dependents.iter());

        while let Some(key) = stack.pop_front() {
            if let Some(node) = self.graph.get(&key) {
                if visited.insert(key) {
                    nodes.push(key);

                    for &neighbour in &node.dependents {
                        stack.push_back(neighbour);
                    }
                }
            }
        }
        nodes
    }

    /// Add nodes recursively to the topological order, starting from the given node.
    fn visit(&self, key: &K, visited: &mut BTreeSet<K>, order: &mut VecDeque<K>) {
        if visited.insert(*key) {
            // Recursively visit all of the node's dependents.
            if let Some(node) = self.graph.get(key) {
                for dependent in node.dependents.iter().rev() {
                    self.visit(dependent, visited, order);
                }
            }
            // Add the node to the topological order.
            order.push_front(*key);
        }
    }
}

impl<K: Ord + Copy + fmt::Display, V> Dag<K, V> {
    /// Return the graph in "dot" format.
    pub fn to_dot(&self) -> String {
        let mut output = String::new();

        writeln!(output, "digraph G {{").ok();
        for (k, v) in self.graph.iter() {
            for d in &v.dependencies {
                writeln!(output, "\t\"{k}\" -> \"{d}\";").ok();
            }
        }
        writeln!(output, "}}").ok();

        output
    }
}

impl<K: Ord + Copy + fmt::Debug, V> Index<&K> for Dag<K, V> {
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

        let mut sorted = dag.sorted();
        let expected: &[&[i32]] = &[&[0, 1], &[1, 0]];

        assert!(expected.contains(&&*sorted.make_contiguous()));
    }

    #[test]
    fn test_merge_1() {
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
    fn test_merge_2() {
        let mut a = Dag::new();
        let mut b = Dag::new();

        a.node(0, ());
        a.node(1, ());
        a.node(2, ());
        a.dependency(1, 0);
        a.dependency(2, 0);

        b.node(0, ());
        b.node(1, ());
        b.node(2, ());
        b.node(3, ());
        b.node(4, ());
        b.dependency(1, 0);
        b.dependency(2, 0);
        b.dependency(3, 0);
        b.dependency(4, 2);

        assert!(a.tips.contains(&2));

        a.merge(b);

        assert!(a.get(&0).is_some());
        assert!(a.get(&1).is_some());
        assert!(a.get(&2).is_some());
        assert!(a.get(&3).is_some());
        assert!(a.get(&4).is_some());
        assert!(a.has_dependency(&4, &2));
        assert!(a.get(&2).unwrap().dependents.contains(&4));
        assert!(a.get(&0).unwrap().dependents.contains(&3));
        assert!(a.tips.contains(&1));
        assert!(!a.tips.contains(&2));
        assert!(a.tips.contains(&3));
        assert!(a.tips.contains(&4));
        assert!(a.roots.contains(&0));
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
        let mut actual = dag.sorted();

        assert!(expected.contains(&&*actual.make_contiguous()), "{actual:?}");
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
            dag.tips().map(|(k, _)| *k).collect::<BTreeSet<_>>(),
            BTreeSet::from_iter([1, 0])
        );
        assert_eq!(
            dag.roots().map(|(k, _)| *k).collect::<BTreeSet<_>>(),
            BTreeSet::from_iter([4, 5])
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
        let mut sorts = BTreeSet::new();
        let mut rng = fastrand::Rng::new();

        while sorts.len() < expected.len() {
            sorts.insert(
                dag.sorted_by(|a, b| if rng.bool() { a.cmp(b) } else { b.cmp(a) })
                    .make_contiguous()
                    .to_vec(),
            );
        }
        for e in expected {
            assert!(sorts.remove(e.to_vec().as_slice()));
        }
        assert!(sorts.is_empty());
    }

    #[test]
    fn test_fold_sorting_1() {
        let mut dag = Dag::new();

        dag.node("R", ());
        dag.node("A1", ());
        dag.node("A2", ());
        dag.node("A3", ());
        dag.node("B1", ());
        dag.node("B2", ());
        dag.node("B3", ());
        dag.node("C1", ());

        dag.dependency("A1", "R");
        dag.dependency("A2", "R");
        dag.dependency("A3", "R");

        dag.dependency("B1", "A1");
        dag.dependency("B2", "A1");
        dag.dependency("B3", "A2");
        dag.dependency("B3", "A3");

        dag.dependency("C1", "B1");
        dag.dependency("C1", "B2");
        dag.dependency("C1", "B3");

        let acc = dag.fold(&["R"], Vec::new(), |mut acc, key, _| {
            acc.push(*key);
            ControlFlow::Continue(acc)
        });
        assert_eq!(acc, vec!["R", "A1", "B1", "B2", "A2", "A3", "B3", "C1"]);
    }

    #[test]
    fn test_fold_sorting_2() {
        let mut dag = Dag::new();

        dag.node("R", ());
        dag.node("A1", ());
        dag.node("A2", ());
        dag.node("A3", ());
        dag.node("B1", ());
        dag.node("C1", ());
        dag.node("C2", ());
        dag.node("C3", ());

        dag.dependency("A1", "R");
        dag.dependency("A2", "A1");
        dag.dependency("A3", "A2");

        dag.dependency("B1", "R");

        dag.dependency("C1", "B1");
        dag.dependency("C1", "A3");
        dag.dependency("C2", "B1");
        dag.dependency("C2", "A3");
        dag.dependency("C3", "B1");
        dag.dependency("C3", "A3");

        let acc = dag.fold(&["R"], Vec::new(), |mut acc, key, _| {
            acc.push(*key);
            ControlFlow::Continue(acc)
        });

        assert_eq!(acc, vec!["R", "A1", "A2", "A3", "B1", "C1", "C2", "C3"]);
        assert_eq!(dag.sorted(), acc);
    }

    #[test]
    fn test_fold_diamond() {
        let mut dag = Dag::new();

        dag.node("R", ());
        dag.node("A1", ());
        dag.node("A2", ());
        dag.node("B", ());

        dag.dependency("A1", "R");
        dag.dependency("A2", "R");
        dag.dependency("B", "A1");
        dag.dependency("B", "A2");

        let acc = dag.fold(&["R"], Vec::new(), |mut acc, key, _| {
            acc.push(*key);
            ControlFlow::Continue(acc)
        });
        assert_eq!(acc, vec!["R", "A1", "A2", "B"]);

        let sorted = dag.sorted();
        assert_eq!(sorted, acc);
    }

    #[test]
    fn test_fold_multiple_roots() {
        let mut dag = Dag::new();

        dag.node("R", ());
        dag.node("A1", ());
        dag.node("A2", ());

        dag.dependency("A1", "R");
        dag.dependency("A2", "R");

        let acc = dag.fold(&["A1", "A2"], Vec::new(), |mut acc, key, _| {
            acc.push(*key);
            ControlFlow::Continue(acc)
        });
        assert_eq!(acc, &["A1", "A2"]);
    }

    #[test]
    fn test_fold_reject() {
        let mut dag = Dag::new();

        dag.node("R", ());
        dag.node("A1", ());
        dag.node("A2", ());
        dag.node("B1", ());
        dag.node("C1", ());
        dag.node("D1", ());

        dag.dependency("A1", "R");
        dag.dependency("A2", "R");
        dag.dependency("B1", "A1");
        dag.dependency("C1", "B1");
        dag.dependency("D1", "C1");
        dag.dependency("D1", "A2");

        let a1 = dag.get(&"A1").unwrap();
        assert_eq!(dag.descendants_of(a1), vec!["B1", "C1", "D1"]);

        let acc = dag.fold(&["R"], Vec::new(), |mut acc, key, _| {
            if *key == "A1" {
                ControlFlow::Break(acc)
            } else {
                acc.push(*key);
                ControlFlow::Continue(acc)
            }
        });
        assert_eq!(acc, vec!["R", "A2"]);

        let acc = dag.fold(&["R"], Vec::new(), |mut acc, key, _| {
            if *key == "A2" {
                ControlFlow::Break(acc)
            } else {
                acc.push(*key);
                ControlFlow::Continue(acc)
            }
        });
        assert_eq!(acc, vec!["R", "A1", "B1", "C1"]);
    }

    #[test]
    fn test_remove() {
        let mut dag = Dag::new();

        dag.node("R", ());
        dag.node("A1", ());
        dag.node("A2", ());
        dag.node("A3", ());
        dag.node("B1", ());
        dag.node("C1", ());
        dag.node("D1", ());

        dag.dependency("A1", "R");
        dag.dependency("A2", "R");
        dag.dependency("A3", "A2");
        dag.dependency("B1", "A1");
        dag.dependency("B1", "A2");
        dag.dependency("C1", "B1");
        dag.dependency("C1", "A3");
        dag.dependency("D1", "C1");
        dag.dependency("D1", "A2");

        dag.remove(&"C1");
        assert!(dag.get(&"C1").is_none());
        assert!(dag.get(&"D1").is_none());
        assert!(!dag.tips.contains(&"D1"));
        assert_eq!(dag.tips.iter().collect::<Vec<_>>(), vec![&"A3", &"B1"]);

        dag.remove(&"A3");
        assert_eq!(dag.tips.iter().collect::<Vec<_>>(), vec![&"B1"]);

        dag.remove(&"A1");
        assert!(dag.get(&"A1").is_none());
        assert!(dag.get(&"B1").is_none());
        assert!(dag.get(&"A2").is_some());
        assert_eq!(dag.tips.iter().collect::<Vec<_>>(), vec![&"A2"]);

        dag.remove(&"R");
        assert!(dag.is_empty());
        assert!(dag.tips.is_empty());
        assert!(dag.roots.is_empty());
    }

    #[test]
    fn test_prune_1() {
        let mut dag = Dag::new();

        dag.node("R", ());
        dag.node("A1", ());
        dag.node("A2", ());
        dag.node("B1", ());
        dag.node("C1", ());
        dag.node("D1", ());

        dag.dependency("A1", "R");
        dag.dependency("A2", "R");
        dag.dependency("B1", "A1");
        dag.dependency("C1", "B1");
        dag.dependency("D1", "C1");
        dag.dependency("D1", "A2");

        let a1 = dag.get(&"A1").unwrap();
        assert_eq!(dag.descendants_of(a1), vec!["B1", "C1", "D1"]);

        dag.prune(&["R"], |key, _| {
            if key == &"B1" {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        assert_eq!(dag.sorted(), vec!["R", "A1", "A2"]);
    }

    #[test]
    fn test_prune_2() {
        let mut dag = Dag::new();

        dag.node("R", ());
        dag.node("A1", ());
        dag.node("A2", ());
        dag.node("A3", ());
        dag.node("B1", ());
        dag.node("C1", ());
        dag.node("C2", ());
        dag.node("C3", ());

        dag.dependency("A1", "R");
        dag.dependency("A2", "A1");
        dag.dependency("A3", "A2");

        dag.dependency("B1", "R");

        dag.dependency("C1", "B1");
        dag.dependency("C1", "A3");
        dag.dependency("C2", "B1");
        dag.dependency("C2", "A3");
        dag.dependency("C3", "B1");
        dag.dependency("C3", "A3");

        let mut order = VecDeque::new();

        dag.prune(&["R"], |key, _| {
            order.push_back(*key);
            ControlFlow::Continue(())
        });
        assert_eq!(order, dag.sorted());
    }
}

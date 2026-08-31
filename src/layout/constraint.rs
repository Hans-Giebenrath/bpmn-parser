use crate::common::graph::EdgeId;
use crate::common::graph::NodeId;
use crate::common::node::Node;
use crate::common::vecset::VecSet;

#[derive(Default, Debug, Clone)]
pub struct LayoutConstraints {
    pub before: Vec<Before>,
    pub above: Vec<Above>,
    pub same_layer: Vec<SameLayer>,
    /// Note: These are _only_ the user provided back edge constraints. The results of the back edge
    /// detection phase are stored somewhere else.
    pub back_edge: Vec<BackEdge>,

    /// A mix of `Above` and `SameLayer` constraints which form a cluster, computed in the
    /// `.finish()` function.
    pub same_layer_clusters: Vec<VecSet<NodeId>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Before {
    pub left: NodeId,
    pub right: NodeId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Above {
    pub above: NodeId,
    pub below: NodeId,
}
#[derive(Debug, Clone, PartialEq)]
pub struct SameLayer(pub NodeId, pub NodeId);
#[derive(Debug, Clone, PartialEq)]
/// This tells the layer organization ILP to not treat this edge as a forward-facing edge, i.e.
/// it does not move into the list of constraints.
pub struct BackEdge(pub EdgeId);

impl LayoutConstraints {
    /// Removes references to blackbox nodes, and computes the same layer clusters.
    pub fn finish(&mut self, nodes: &[Node]) {
        self.remove_references_to_blackboxes(nodes);
        self.compute_same_layer_clusters();
    }

    pub fn remove_references_to_blackboxes(&mut self, nodes: &[Node]) {
        self.before
            .retain(|c| !nodes[c.left].is_blackbox_node() && !nodes[c.right].is_blackbox_node());
        self.above
            .retain(|c| !nodes[c.above].is_blackbox_node() && !nodes[c.below].is_blackbox_node());
        self.same_layer
            .retain(|c| !nodes[c.0].is_blackbox_node() && !nodes[c.1].is_blackbox_node());
    }

    fn compute_same_layer_clusters(&mut self) {
        'outer: for (a, b) in self
            .above
            .iter()
            .map(|c| (c.above, c.below))
            .chain(self.same_layer.iter().map(|SameLayer(a, b)| (*a, *b)))
        {
            let c = &mut self.same_layer_clusters;
            for cluster_idx_0 in 0..c.len() {
                let contains_a = c[cluster_idx_0].contains(&a);
                let contains_b = c[cluster_idx_0].contains(&b);
                let needle = match (contains_a, contains_b) {
                    (true, true) => continue 'outer,
                    (true, false) => b,
                    (false, true) => a,
                    (false, false) => continue,
                };
                c[cluster_idx_0].insert(needle);
                // Check if any other cluster already contains `needle`, in which case the two
                // clusters must be merged into one.
                for cluster_idx_1 in cluster_idx_0 + 1..c.len() {
                    let contains_needle = c[cluster_idx_1].contains(&needle);
                    if contains_needle {
                        let to_be_merged = c.swap_remove(cluster_idx_1);
                        let target = &mut c[cluster_idx_0];
                        to_be_merged.iter().for_each(|n| {
                            target.insert(*n);
                        });
                    }
                }
                continue 'outer;
            }
            // Haven't encountered a nor b, so make them a new cluster.
            let new_cluster = c.push_mut(Default::default());
            new_cluster.insert(a);
            new_cluster.insert(b);
        }
    }
}

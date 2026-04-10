use proc_macros::{e, edges, follow, n};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::common::direction::Direction;
use crate::common::graph::{Graph, NodeId};
use crate::common::node::Node;
use crate::layout::all_crossing_minimization_sweep::{
    EdgeConnection, INCOMING, OUTGOING, Pull, Slice, SweepGraph, SweepNode, SweepNodeId, aux,
};

#[derive(Default)]
pub struct P3Layer {
    constrained_list: Vec<usize>, // V, indices into `merge_nodes`
    merge_nodes: Vec<MergeNode>,  // L(.)
}

pub fn run(graph: &Graph, sweep_graph: &mut SweepGraph, layer: usize, direction: Direction) {
    let state = P3Layer::default();

    let mut rest: Vec<usize> = Vec::new(); // V'
    state.create_merge_nodes(graph, sweep_graph, layer, direction, &mut rest);

    while let Some((above_idx, below_idx)) = self.find_violated_constraint() {
        self.absorb(above_idx, below_idx);

        self.constrained_list.retain(|&idx| idx != below_idx);

        debug_assert!(self.constrained_list.contains(&above_idx));

        if self.merge_nodes[above_idx].has_incident_constraints() {
            // stays in constrained_list
        } else {
            self.constrained_list.retain(|&idx| idx != above_idx);
            rest.push(above_idx);
        }
    }

    rest.extend(self.constrained_list.iter().copied());
    self.constrained_list.clear();

    rest.sort_by(|&a, &b| {
        self.merge_nodes[a]
            .barycenter
            .partial_cmp(&self.merge_nodes[b].barycenter)
            .unwrap_or(Ordering::Equal)
    });

    self.reorder_nodes(layer, &rest);
}

impl P3Layer {
    fn reorder_nodes(&self, layer: &mut Layer, ordered_merge_nodes: &[usize]) {
        let mut i = 0usize;
        let mut p_in = 0usize;
        let mut p_out = 0usize;

        let mut new_nodes: Vec<Node> = Vec::with_capacity(layer.nodes.len());

        for &mn_idx in ordered_merge_nodes {
            let mn = &self.merge_nodes[mn_idx];
            if !mn.alive {
                continue;
            }

            for &node_id in &mn.ordered_nodes {
                let mut n = layer.take_node(node_id);

                n.idx_in_layer = i;

                for p in n.ports_mut(PortSide::In) {
                    p.layer_order = p_in;
                    p_in += 1;
                }

                for p in n.ports_mut(PortSide::Out) {
                    p.layer_order = p_out;
                    p_out += 1;
                }

                new_nodes.push(n);
                i += 1;
            }
        }

        debug_assert_eq!(i, layer.nodes.len());
        layer.nodes = new_nodes;
    }

    fn create_merge_nodes(
        &mut self,
        graph: &Graph,
        sweep_graph: &SweepGraph,
        layer: usize,
        is_right_sweep: bool,
        mind_the_pull: bool,
        rest: &mut Vec<usize>,
    ) {
        let mut map = HashMap::new();
        let mut all: Vec<usize> = Vec::new();

        let current_layer = sweep_graph.layers[layer].clone();
        for (idx, sweep_node) in sweep_graph.nodes[current_layer.as_range()]
            .iter()
            .enumerate()
        {
            let sweep_node_idx = idx + current_layer.start as usize;
            let mn = MergeNode::new(
                graph,
                sweep_graph,
                sweep_node,
                SweepNodeId(sweep_node_idx),
                is_right_sweep,
                mind_the_pull,
            );
            let idx = self.merge_nodes.len();
            self.merge_nodes.push(mn);
            map.insert(n.id, idx);
            all.push(idx);
        }

        // Second pass: wire constraints
        for &mn_idx in &all {
            let node_id = self.merge_nodes[mn_idx].ordered_nodes[0];
            let node = layer.get_node(node_id);

            for a in &node.in_uc_above {
                debug_assert_eq!(node.id, a.lower);
                let higher_idx = map[&a.higher];
                self.merge_nodes[mn_idx].above_of_this.insert(higher_idx);
            }

            for a in &node.out_uc_above {
                debug_assert_eq!(node.id, a.higher);
                let lower_idx = map[&a.lower];
                self.merge_nodes[mn_idx].below_of_this.insert(lower_idx);
            }

            if self.merge_nodes[mn_idx].has_incident_constraints() {
                self.constrained_list.push(mn_idx);
            } else {
                rest.push(mn_idx);
            }
        }
    }

    fn find_violated_constraint(&mut self) -> Option<(usize, usize)> {
        let mut s: VecDeque<usize> = VecDeque::new();

        // fill the queue
        for &mn_idx in &self.constrained_list {
            self.merge_nodes[mn_idx].i.clear();
            if self.merge_nodes[mn_idx].above_of_this.is_empty() {
                s.push_back(mn_idx);
            }
        }

        while let Some(v_idx) = s.pop_front() {
            let v_barycenter = self.merge_nodes[v_idx].barycenter;
            let i_snapshot = self.merge_nodes[v_idx].i.clone();

            for s_idx in i_snapshot {
                if self.merge_nodes[s_idx].barycenter >= v_barycenter {
                    return Some((s_idx, v_idx));
                }
            }

            let below_snapshot: Vec<usize> = self.merge_nodes[v_idx]
                .below_of_this
                .iter()
                .copied()
                .collect();

            for t_idx in below_snapshot {
                self.merge_nodes[t_idx].i.insert(0, v_idx);
                if self.merge_nodes[t_idx].i.len() == self.merge_nodes[t_idx].above_of_this.len() {
                    s.push_back(t_idx);
                }
            }
        }

        None
    }

    fn absorb(&mut self, winner_idx: usize, victim_idx: usize) {
        if winner_idx == victim_idx {
            return;
        }

        let winner_degree = self.merge_nodes[winner_idx].degree;
        let victim_degree = self.merge_nodes[victim_idx].degree;
        let new_degree = winner_degree + victim_degree;

        let new_barycenter = if new_degree != 0.0 {
            (self.merge_nodes[winner_idx].barycenter * winner_degree
                + self.merge_nodes[victim_idx].barycenter * victim_degree)
                / new_degree
        } else {
            0.0
        };

        let victim_ordered_nodes = self.merge_nodes[victim_idx].ordered_nodes.clone();
        let victim_above: Vec<usize> = self.merge_nodes[victim_idx]
            .above_of_this
            .iter()
            .copied()
            .collect();
        let victim_below: Vec<usize> = self.merge_nodes[victim_idx]
            .below_of_this
            .iter()
            .copied()
            .collect();

        self.merge_nodes[winner_idx]
            .ordered_nodes
            .extend(victim_ordered_nodes);

        let removed_above = self.merge_nodes[winner_idx]
            .above_of_this
            .remove(&victim_idx);
        debug_assert!(!removed_above);

        let removed_below = self.merge_nodes[winner_idx]
            .below_of_this
            .remove(&victim_idx);
        debug_assert!(removed_below);

        for above_idx in victim_above {
            if above_idx == winner_idx {
                continue;
            }

            self.merge_nodes[above_idx]
                .below_of_this
                .remove(&victim_idx);
            self.merge_nodes[above_idx].below_of_this.insert(winner_idx);
            self.merge_nodes[winner_idx].above_of_this.insert(above_idx);
        }

        for below_idx in victim_below {
            if below_idx == winner_idx {
                continue;
            }

            self.merge_nodes[below_idx]
                .above_of_this
                .remove(&victim_idx);
            self.merge_nodes[below_idx].above_of_this.insert(winner_idx);
            self.merge_nodes[winner_idx].below_of_this.insert(below_idx);
        }

        self.merge_nodes[winner_idx].barycenter = new_barycenter;
        self.merge_nodes[winner_idx].degree = new_degree;

        self.merge_nodes[victim_idx].alive = false;
        self.merge_nodes[victim_idx].above_of_this.clear();
        self.merge_nodes[victim_idx].below_of_this.clear();
        self.merge_nodes[victim_idx].i.clear();
    }
}

#[derive(Debug, Clone)]
struct MergeNode {
    ordered_nodes: Vec<SweepNodeId>,
    barycenter: f32,
    degree: f32,
    above_of_this: HashSet<usize>,
    below_of_this: HashSet<usize>,
    i: Vec<usize>,
    alive: bool,
    // Should be the sum from the previous analysis (which generated Pull) instead.
    pull: Vec<Pull>,
}

impl MergeNode {
    fn new(
        graph: &Graph,
        sweep_graph: &SweepGraph,
        sweep_node: &SweepNode,
        sweep_node_id: SweepNodeId,
        is_right_sweep: bool,
        mind_the_pull: bool,
    ) -> Self {
        let direction = if is_right_sweep { INCOMING } else { OUTGOING };
        let pull = if mind_the_pull && let Some(pull) = sweep_node.pull.clone() {
            vec![pull]
        } else {
            Vec::new()
        };
        let mut accum: f32 = 0.0;
        let it = sweep_graph.edges[sweep_node.ports_start[direction] as usize
            ..(sweep_node.ports_start[direction] as usize
                + sweep_node.ports_len[direction] as usize)]
            .iter()
            .flat_map(|(edge_id, edge_connection)| {
                if matches!(edge_connection, EdgeConnection::BothConnected) {
                    let node_id = if is_right_sweep {
                        e!(*edge_id).from
                    } else {
                        e!(*edge_id).to
                    };
                    Some(sweep_graph.nodes[aux(&n!(node_id))].layer_position as f32)
                } else {
                    // Lane-crossing edges are not added here, those are incorporated via
                    // `Pull`.
                    // Non-lane-crossing edges should be handled by being merged immediately in the
                    // first few sweep rounds and afterwards ignored, so
                    // ignore them here as well.
                    None
                }
            });
        let mut degree: f32 = 0.0;
        for layer_position in it {
            accum += layer_position;
            degree += 1.0;
        }

        let barycenter = if degree == 0.0 { 0.0 } else { accum / degree };

        Self {
            ordered_nodes: vec![sweep_node_id],
            barycenter,
            degree: degree as f32,
            above_of_this: HashSet::new(),
            below_of_this: HashSet::new(),
            i: Vec::new(),
            alive: true,
            pull,
        }
    }

    fn has_incident_constraints(&self) -> bool {
        !self.above_of_this.is_empty() || !self.below_of_this.is_empty()
    }
}

use proc_macros::edges;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::common::direction::Direction;
use crate::common::graph::NodeId;
use crate::common::node::Node;
use crate::layout::all_crossing_minimization_sweep::SweepGraph;

pub struct P3Layer {
    constrained_list: Vec<usize>, // indices into `merge_nodes`
    merge_nodes: Vec<MergeNode>,
    map: HashMap<NodeId, usize>, // original node -> merge node index
}

pub fn run(&mut self, layer: &mut Layer, direction: Direction) {
    self.constrained_list.clear();
    self.merge_nodes.clear();
    self.map.clear();

    let mut rest: Vec<usize> = Vec::new(); // V'
    self.create_merge_nodes(layer, port_side, &mut rest);

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
    pub fn new() -> Self {
        Self {
            constrained_list: Vec::new(), // V
            merge_nodes: Vec::new(),      // L(.)
            map: HashMap::new(),
        }
    }

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
        sweep_graph: &SweepGraph,
        layer: &Layer,
        port_side: PortSide,
        rest: &mut Vec<usize>,
    ) -> (Vec<MergeNode>, HashMap<NodeId, usize>) {
        let mut merge_nodes = Vec::new();
        let mut map = HashMap::new();
        let mut all: Vec<usize> = Vec::new();

        for n in &layer.nodes {
            let mn = MergeNode::new(n, port_side);
            let idx = self.merge_nodes.len();
            self.merge_nodes.push(mn);
            self.map.insert(n.id, idx);
            all.push(idx);
        }

        // Second pass: wire constraints
        for &mn_idx in &all {
            let node_id = self.merge_nodes[mn_idx].ordered_nodes[0];
            let node = layer.get_node(node_id);

            for a in &node.in_uc_above {
                debug_assert_eq!(node.id, a.lower);
                let higher_idx = self.map[&a.higher];
                self.merge_nodes[mn_idx].above_of_this.insert(higher_idx);
            }

            for a in &node.out_uc_above {
                debug_assert_eq!(node.id, a.higher);
                let lower_idx = self.map[&a.lower];
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
    ordered_nodes: Vec<NodeId>,
    barycenter: f32,
    degree: f32,
    above_of_this: HashSet<usize>,
    below_of_this: HashSet<usize>,
    i: Vec<usize>,
    alive: bool,
}

impl MergeNode {
    fn new(sweep_graph: &SweepGraph, node: &Node, direction: Direction) -> Self {
        let mut accum: usize = 0;
        let mut degree: usize = 0;

        for p in &edges!(node) {
            for e in &p.edges {
                let order = match port_side {
                    PortSide::Out => e.to_port_layer_order,
                    PortSide::In => e.from_port_layer_order,
                };
                accum += order;
                degree += 1;
            }
        }

        let barycenter = if degree == 0 {
            0.0
        } else {
            accum as f32 / degree as f32
        };

        Self {
            ordered_nodes: vec![node.id],
            barycenter,
            degree: degree as f32,
            above_of_this: HashSet::new(),
            below_of_this: HashSet::new(),
            i: Vec::new(),
            alive: true,
        }
    }

    fn has_incident_constraints(&self) -> bool {
        !self.above_of_this.is_empty() || !self.below_of_this.is_empty()
    }
}

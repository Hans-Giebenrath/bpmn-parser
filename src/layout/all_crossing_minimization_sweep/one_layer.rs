use crate::common::graph::{Coord3, Graph, PoolAndLane};
use crate::common::lane::Lane;
use crate::layout::all_crossing_minimization_sweep::{
    EdgeConnection, INCOMING, OUTGOING, PullBalance, SweepGraph, SweepNode, SweepNodeId, aux,
};
use crate::layout::constraint::Above;
use proc_macros::{e, n};
use std::collections::{HashSet, VecDeque};

#[derive(Default)]
pub struct P3Layer {
    constrained_list: Vec<usize>, // V, indices into `merge_nodes`
    merge_nodes: Vec<MergeNode>,  // L(.)
}

pub fn run(
    graph: &Graph,
    sweep_graph: &mut SweepGraph,
    current_location: &Coord3,
    is_right_sweep: bool,
    mind_the_pull: bool,
    constraints: &[Above],
) {
    let mut state = P3Layer::default();

    let mut unconstrained: Vec<usize> = Vec::new(); // V' (or `rest`)
    state.create_merge_nodes(
        graph,
        sweep_graph,
        constraints,
        current_location,
        is_right_sweep,
        mind_the_pull,
        &mut unconstrained,
    );

    while let Some((above_idx, below_idx)) = state.find_violated_constraint() {
        state.absorb(/* winner */ above_idx, /* victim */ below_idx);

        state.constrained_list.retain(|&idx| idx != below_idx);

        debug_assert!(state.constrained_list.contains(&above_idx));

        if state.merge_nodes[above_idx].has_incident_constraints() {
            // stays in constrained_list
        } else {
            state.constrained_list.retain(|&idx| idx != above_idx);
            unconstrained.push(above_idx);
        }
    }

    unconstrained.extend(state.constrained_list.iter().copied());
    state.constrained_list.clear();

    unconstrained.sort_by(|&a, &b| {
        // DFs have the least priority, SFs the most.
        let (a, b) = (&state.merge_nodes[a], &state.merge_nodes[b]);
        (
            a.pull_balance.sf_balance,
            a.pull_balance.mf_balance,
            a.pull_balance.df_balance,
            a.barycenter,
        )
            .partial_cmp(&(
                b.pull_balance.sf_balance,
                b.pull_balance.mf_balance,
                b.pull_balance.df_balance,
                b.barycenter,
            ))
            .unwrap()
    });

    // Finished - now just assign the positions.
    state
        .merge_nodes
        .iter()
        .filter(|mn| mn.alive)
        .flat_map(|mn| mn.ordered_nodes.iter())
        .cloned()
        .enumerate()
        .for_each(|(position, sweep_node_id)| {
            sweep_graph.nodes[sweep_node_id].layer_position = position.try_into().unwrap()
        });
}

impl P3Layer {
    fn create_merge_nodes(
        &mut self,
        graph: &Graph,
        sweep_graph: &SweepGraph,
        constraints: &[Above],
        current_location: &Coord3,
        is_right_sweep: bool,
        mind_the_pull: bool,
        unconstrained: &mut Vec<usize>,
    ) {
        let current_layer = sweep_graph.layers[current_location.layer.0].clone();
        let current_lane = &graph.pools[current_location.pool_and_lane.pool].lanes
            [current_location.pool_and_lane.lane]
            .nodes;
        for (idx, sweep_node) in sweep_graph.nodes[current_layer.as_range()]
            .iter()
            .enumerate()
        {
            let mn = MergeNode::new(
                sweep_graph,
                sweep_node,
                SweepNodeId(idx + current_layer.start as usize),
                is_right_sweep,
                mind_the_pull,
            );
            self.merge_nodes.push(mn);
        }

        // Second pass: wire constraints. Since those should be *very* few, then doing it suboptimal
        // is probably OK (BUT! the partitioning per Coord3 should ideally be done outside.)
        for above_constraint in constraints {
            // `above` and `below` are indices into both sweep_graph.nodes[current_layer.as_range()]
            // and `self.merge_nodes`.
            let above = sweep_graph.nodes[current_layer.as_range()]
                .iter()
                .position(|sn| current_lane[sn.in_lane_idx as usize] == above_constraint.above)
                .unwrap();
            let below = sweep_graph.nodes[current_layer.as_range()]
                .iter()
                .position(|sn| current_lane[sn.in_lane_idx as usize] == above_constraint.below)
                .unwrap();
            self.merge_nodes[above].below_of_this.insert(below);
            self.merge_nodes[below].above_of_this.insert(above);
        }
        for (mn_idx, mn) in self.merge_nodes.iter().enumerate() {
            if mn.has_incident_constraints() {
                self.constrained_list.push(mn_idx);
            } else {
                unconstrained.push(mn_idx);
            }
        }
    }

    fn find_violated_constraint(&mut self) -> Option<(usize, usize)> {
        // TODO My Java code uses a Queue here with `add` and `poll` but the paper just uses a set.
        // So a `Vec` with `push` and `pop` should be sufficient?
        let mut s: VecDeque<usize> = VecDeque::new();

        // fill the queue
        for &mn_idx in &self.constrained_list {
            self.merge_nodes[mn_idx].incoming_constraints.clear();
            if self.merge_nodes[mn_idx].above_of_this.is_empty() {
                s.push_back(mn_idx);
            }
        }

        while let Some(v_idx) = s.pop_front() {
            let v_barycenter = self.merge_nodes[v_idx].barycenter;

            for &s_idx in &self.merge_nodes[v_idx].incoming_constraints {
                if self.merge_nodes[s_idx].barycenter >= v_barycenter {
                    return Some((s_idx, v_idx));
                }
            }

            // Borrow-checker workaround, is reassigned at the end.
            // TODO maybe add a Vec based VecSet here, so we can do index based iteration.
            let below_snapshot = std::mem::take(&mut self.merge_nodes[v_idx].below_of_this);

            for &t_idx in &below_snapshot {
                self.merge_nodes[t_idx]
                    .incoming_constraints
                    .insert(0, v_idx);
                if self.merge_nodes[t_idx].incoming_constraints.len()
                    == self.merge_nodes[t_idx].above_of_this.len()
                {
                    s.push_back(t_idx);
                }
            }

            self.merge_nodes[v_idx].below_of_this = below_snapshot;
        }

        None
    }

    fn absorb(&mut self, winner_idx: usize, victim_idx: usize) {
        assert_ne!(winner_idx, victim_idx);

        let mns = &mut self.merge_nodes;
        let winner_degree = mns[winner_idx].degree;
        let victim_degree = mns[victim_idx].degree;
        let new_degree = winner_degree + victim_degree;

        let new_barycenter = if new_degree != 0.0 {
            (mns[winner_idx].barycenter * winner_degree
                + mns[victim_idx].barycenter * victim_degree)
                / new_degree
        } else {
            0.0
        };

        let victim_ordered_nodes = mns[victim_idx].ordered_nodes.clone();
        // TODO this copying around could be avoided if we used a VecSet and then iterated over indices.
        let victim_above: Vec<usize> = mns[victim_idx].above_of_this.iter().copied().collect();
        let victim_below: Vec<usize> = mns[victim_idx].below_of_this.iter().copied().collect();

        mns[winner_idx].ordered_nodes.extend(victim_ordered_nodes);

        let removed_above = mns[winner_idx].above_of_this.remove(&victim_idx);
        debug_assert!(!removed_above);

        let removed_below = mns[winner_idx].below_of_this.remove(&victim_idx);
        debug_assert!(removed_below);

        for above_idx in victim_above {
            if above_idx == winner_idx {
                continue;
            }

            mns[above_idx].below_of_this.remove(&victim_idx);
            mns[above_idx].below_of_this.insert(winner_idx);
            mns[winner_idx].above_of_this.insert(above_idx);
        }

        for below_idx in victim_below {
            if below_idx == winner_idx {
                continue;
            }

            mns[below_idx].above_of_this.remove(&victim_idx);
            mns[below_idx].above_of_this.insert(winner_idx);
            mns[winner_idx].below_of_this.insert(below_idx);
        }

        mns[winner_idx].barycenter = new_barycenter;
        mns[winner_idx].degree = new_degree;
        mns[winner_idx].pull_balance.sf_balance += mns[victim_idx].pull_balance.sf_balance;
        mns[winner_idx].pull_balance.mf_balance += mns[victim_idx].pull_balance.mf_balance;
        mns[winner_idx].pull_balance.df_balance += mns[victim_idx].pull_balance.df_balance;

        mns[victim_idx].deactivate();
    }
}

#[derive(Debug, Clone)]
struct MergeNode {
    ordered_nodes: Vec<SweepNodeId>,
    barycenter: f32,
    degree: f32,
    above_of_this: HashSet</* merge node idx */ usize>,
    below_of_this: HashSet</* merge node idx */ usize>,
    incoming_constraints: Vec</* merge node idx */ usize>,
    alive: bool,
    /// Should be the sum from the previous analysis (which generated Pull) instead.
    pull_balance: PullBalance,
}

impl MergeNode {
    fn new(
        sweep_graph: &SweepGraph,
        sweep_node: &SweepNode,
        sweep_node_id: SweepNodeId,
        is_right_sweep: bool,
        mind_the_pull: bool,
    ) -> Self {
        let direction = if is_right_sweep { INCOMING } else { OUTGOING };
        let mut accum: f32 = 0.0;
        let it = sweep_graph.edge_targets[sweep_node.ports_start[direction] as usize
            ..(sweep_node.ports_start[direction] as usize
                + sweep_node.ports_len[direction] as usize)]
            .iter()
            .flat_map(|(target_node, edge_connection)| {
                if matches!(edge_connection, EdgeConnection::Both) {
                    Some(sweep_graph.nodes[*target_node].layer_position as f32)
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
            degree,
            above_of_this: HashSet::new(),
            below_of_this: HashSet::new(),
            incoming_constraints: Vec::new(),
            alive: true,
            pull_balance: if mind_the_pull {
                sweep_node.pull_balance.clone()
            } else {
                Default::default()
            },
        }
    }

    fn has_incident_constraints(&self) -> bool {
        !self.above_of_this.is_empty() || !self.below_of_this.is_empty()
    }

    fn deactivate(&mut self) {
        self.ordered_nodes.clear();
        self.above_of_this.clear();
        self.below_of_this.clear();
        self.incoming_constraints.clear();
    }
}

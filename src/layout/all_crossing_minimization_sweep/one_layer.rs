use crate::common::graph::{Coord3, Graph, PoolAndLane};
use crate::common::index_iter::IterIndices;
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
    unconstrained: Vec<usize>,    // V' (or `rest`)
}

// Updated algorithm outline:
// * same old: Create the initial merge node list as usual
// * new: put together the same-lane-vertical-edge node clumps, i.e. merge (possibly flipped if allowed and the barycenter favors it) their merge nodes.
// * same old: do the regular violated constrained search until finished
// * new (TODO implement): For each of those clumped vertices, check whose vertex is the most offset with its
//   barycenter value (if that is even possible to define?) - take it and move it into its favored
//   direction, across long edge dummies, until it hits some non-long-edge dummy node. (Repeat, as
//   mentioned, for each of the clumped vertices.)
// Now it should be done.

/// The original algorithm comes from the paper:
/// Forster, M. (2005). A Fast and Simple Heuristic for Constrained Two-Level Crossing Reduction.
/// In: Pach, J. (eds) Graph Drawing. GD 2004. Lecture Notes in Computer Science, vol 3383.
/// Springer, Berlin, Heidelberg. https://doi.org/10.1007/978-3-540-31843-9_22
///
/// Implementation history: I first implemented it for my bachelor's thesis in Java. Then, for BPMD,
/// I used ChatGPT 5.3 (?maybe) to transpile it to Rust. Did heavy editing. Then added the support
/// for vertical edges.
pub fn run(
    graph: &Graph,
    sweep_graph: &mut SweepGraph,
    current_location: &Coord3,
    is_right_sweep: bool,
    mind_the_pull: bool,
    mind_the_vertical_chains: bool,
    constraints: &[Above],
) {
    let mut state = P3Layer::default();

    state.create_merge_nodes(
        graph,
        sweep_graph,
        constraints,
        current_location,
        is_right_sweep,
        mind_the_pull,
    );

    if mind_the_vertical_chains {
        for vertical_sequence in sweep_graph.vertical_edge_chains.iter() {
            if vertical_sequence.coord3 != *current_location {
                continue;
            }
            let flip = if vertical_sequence.can_be_flipped {
                let mut sorted_pairs = 0;
                let mut rev_sorted_pairs = 0;
                for [id0, id1] in vertical_sequence.top_to_bottom_node_list.array_windows() {
                    let mn0 = state
                        .merge_nodes
                        .iter()
                        .find(|mn| mn.ordered_nodes[0] == *id0)
                        .unwrap();
                    let mn1 = state
                        .merge_nodes
                        .iter()
                        .find(|mn| mn.ordered_nodes[0] == *id1)
                        .unwrap();
                    match mn0.barycenter.partial_cmp(&mn1.barycenter).unwrap() {
                        std::cmp::Ordering::Less => sorted_pairs += 1,
                        std::cmp::Ordering::Greater => rev_sorted_pairs += 1,
                        _ => (),
                    }
                }
                rev_sorted_pairs > sorted_pairs
            } else {
                false
            };
            let mut sweepnode_it = vertical_sequence
                .top_to_bottom_node_list
                .len()
                .iter_indices(flip)
                .map(|index| vertical_sequence.top_to_bottom_node_list[index]);
            let top_most_sn_id = sweepnode_it.next().unwrap();
            let top_most_mn_id = state
                .merge_nodes
                .iter()
                .position(|mn| mn.ordered_nodes.first() == Some(&top_most_sn_id))
                .unwrap();
            for sn_id in sweepnode_it {
                let mn = state
                    .merge_nodes
                    .iter()
                    .position(|mn| mn.ordered_nodes.first() == Some(&sn_id))
                    .unwrap();
                state.absorb(
                    /* remaining to-be above: */ top_most_mn_id,
                    /* consumed to-be below: */ mn,
                );
            }
        }
    }

    while let Some((above_idx, below_idx)) = state.find_violated_constraint() {
        // `above_idx` has currently a larger barycenter value, but due to constraints that
        // sweep node must come above sweep node `below_idx`. Hence, the names refer to the to-be
        // situation, not how they are currently ordered.
        state.absorb(/* winner */ above_idx, /* victim */ below_idx);
    }

    state
        .unconstrained
        .extend(state.constrained_list.iter().copied());
    state.constrained_list.clear();

    state.unconstrained.sort_by(|&a, &b| {
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
    let mut i = 0;
    for merge_node_idx in state.unconstrained.iter().cloned() {
        for sweep_node_id in state.merge_nodes[merge_node_idx]
            .ordered_nodes
            .iter()
            .cloned()
        {
            sweep_graph.nodes[sweep_node_id].layer_position = i;
            i += 1;
        }
    }
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
                self.unconstrained.push(mn_idx);
            }
        }
    }

    fn find_violated_constraint(&mut self) -> Option<(usize, usize)> {
        // TODO My Java code uses a Queue here with `add` and `poll` but the paper just uses a set.
        // So a `Vec` with `push` and `pop` should be sufficient?
        // TODO move the allocation out. Does not make sense to alloc+free all the time.
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
                    // `s` must come above `v`, but `s.barycenter` is larger than `v.barycenter`.
                    // So report in the order as they *should* appear.
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
        mns[winner_idx].above_of_this.remove(&victim_idx);
        mns[winner_idx].below_of_this.remove(&victim_idx);

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

        // Fix the lists.
        self.constrained_list.retain(|&idx| idx != victim_idx);

        if self.merge_nodes[winner_idx].has_incident_constraints() {
            // stays in constrained_list. But during the vertical edge phase it could be that
            // the winner did not have any constraints to begin with, but now ingested some from the
            // victim node.
            if !self.constrained_list.contains(&winner_idx) {
                self.constrained_list.push(winner_idx);
            }
        } else {
            self.constrained_list.retain(|&idx| idx != winner_idx);
            self.unconstrained.push(winner_idx);
        }
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

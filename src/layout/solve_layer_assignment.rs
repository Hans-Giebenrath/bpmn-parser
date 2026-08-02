use std::collections::{HashMap, HashSet};

use crate::common::graph::{EdgeId, same_layer_lane_crossings_within_cluster};
use crate::common::node::NodePhaseAuxData;
use proc_macros::{e, n};

use crate::common::graph::Graph;
use crate::common::node::Node;
use good_lp::*;

pub fn solve_layer_assignment(graph: &mut Graph) {
    solve_layers(graph);

    solve_data_object_layers_via_arithmetic_mean(graph);
}

#[derive(Debug)]
pub struct LayerAssignmentData(Variable);

#[track_caller]
fn aux(node: &Node) -> Variable {
    match node.aux {
        NodePhaseAuxData::LayerAssignmentData(LayerAssignmentData(variable)) => variable,
        _ => unreachable!(),
    }
}

const DEBUG_ILP_CONSTRUCTION: bool = false;

macro_rules! d {
    ($($tt:tt)*) => {{
        if DEBUG_ILP_CONSTRUCTION {
            $($tt)*
        }
    }};
}

fn solve_layers(graph: &mut Graph) {
    d!(dbg!(&graph););
    let mut vars = variables!();

    let num_nodes = graph.nodes.len();
    for node in graph.nodes.iter_mut().filter(|node| !node.is_data()) {
        node.aux = NodePhaseAuxData::LayerAssignmentData(LayerAssignmentData(
            vars.add(variable().integer().min(0).max(num_nodes as f64)),
        ));
        d!(eprintln!("0 <= n({}) <= {num_nodes}", node.id.0));
    }

    let mut objective = Expression::from(0.0);

    // Try to pull start nodes to the left. But only starts, let the rest be placed however the
    // algorithm thinks. Not sure yet whether this is good.
    for node in graph.nodes.iter().filter(|node| !node.is_data()) {
        if node
            .incoming
            .iter()
            // A start node has no incoming sequence flows, or its only incoming sequence flows are
            // back edges.
            .all(|edge_id| {
                !e!(*edge_id).is_sequence_flow() || graph.computed_back_edges.contains(edge_id)
            })
        {
            objective += 0.1 * aux(node);
            d!(eprintln!("pull left n({})", node.id.0));
        }
    }

    let mut constraints = Vec::new();
    handle_vertical_lane_crossings(graph, &mut vars, &mut constraints);
    //let mut problem = problem.set_verbose(true);
    //problem.set_parameter("loglevel", "0");

    graph
        .edges
        .iter_mut()
        .enumerate()
        .filter(|(edge_idx, edge)| {
            edge.is_sequence_flow() && !graph.computed_back_edges.contains(&EdgeId(*edge_idx))
        })
        .map(|(_, edge)| (edge.from, edge.to, true, "regular edge"))
        .chain(graph.layout_constraints.left_of.iter().map(|constraint| {
            (
                constraint.left,
                constraint.right,
                false,
                "left-of constraint",
            )
        }))
        .for_each(|(left, right, minimize, msg)| {
            if minimize {
                // Favor short edges
                objective += aux(&n!(right)) - aux(&n!(left));
                d!(eprintln!("minimize n({}) -> n({})", left.0, right.0));
            }
            let from_var = aux(&n!(left));
            let to_var = aux(&n!(right));
            d!(eprintln!(
                "constraint n({}) leftof n({}) ({msg})",
                left.0, right.0
            ));
            constraints.push((to_var - from_var).geq(1));
        });

    graph
        .layout_constraints
        .above
        .iter()
        .map(|constraint| (constraint.above, constraint.below, "above constraint"))
        .chain(
            graph
                .layout_constraints
                .same_layer
                .iter()
                .map(|constraint| (constraint.0, constraint.1, "same layer constraint")),
        )
        .for_each(|(n0, n1, msg)| {
            d!(eprintln!(
                "constraint n({}) same layer as n({}) ({msg})",
                n0.0, n1.0
            ));
            constraints.push((aux(&n!(n0)) - aux(&n!(n1))).eq(0));
        });

    let mut problem = vars.minimise(objective).using(default_solver);
    for c in constraints {
        problem.add_constraint(c);
    }

    let solution = problem.solve().unwrap();
    graph.num_layers = usize::MIN;

    for node in graph.nodes.iter_mut().filter(|node| !node.is_data()) {
        node.layer_id.0 = solution.value(aux(node)) as usize;
        graph.num_layers = graph.num_layers.max(node.layer_id.0 + 1);
    }
}

fn solve_data_object_layers_via_arithmetic_mean(graph: &mut Graph) {
    let mut incoming_buffer = Vec::new();
    let mut outgoing_buffer = Vec::new();

    for node_idx in 0..graph.nodes.len() {
        let data_node = &graph.nodes[node_idx];
        if !data_node.is_data() {
            continue;
        }
        let incomings = data_node.incoming.iter().map(|e| graph.edges[e.0].from);
        let outgoings = data_node.outgoing.iter().map(|e| graph.edges[e.0].to);
        let (sum, count) = incomings
            .chain(outgoings)
            .map(|n| &graph.nodes[n.0])
            .filter(|n| n.lane == data_node.lane)
            .map(|n| n.layer_id.0 as f64)
            .fold((0.0, 0), |(s, c), x| (s + x, c + 1));
        let avg = if count > 0 { sum / count as f64 } else { 0.0 };

        // Mutable reborrow.
        let data_node = &mut graph.nodes[node_idx];

        // avg falls somewhere into the range [x, x + 1).
        // It is mapped kinda logically to:
        //  (1) x in the interval [x, x+0.25),
        //  (2) x+0.5 in the interval [x+0.25, x+0.75]
        //  (2) x+1 in the interval (x+0.75, x+1)
        (data_node.layer_id.0, data_node.uses_half_layer) = {
            let avg_floor = avg.floor();
            let layer_id: usize = avg_floor as usize;
            match avg - avg_floor {
                d if d < 0.25 => (layer_id, false),
                // We *only* allow data nodes to move into the half layer if they have at most two
                // data associations. Otherwise there is some really complicated situation what to
                // do if it is in the halflayer, and the data edges must be routed. They then need
                // to leave at the top or bottom maybe, but this is just something which I don't
                // want to solve at the moment, and I wonder whether it is actually worth it.
                // The usual case for half layers is the simple one-in-one-out-same-flow situation.
                // Also, this is not guaranteed to happen. There is a late check which verifies
                // that there is room at all.
                d if d <= 0.75 && data_node.incoming.len() + data_node.outgoing.len() <= 2 => {
                    (layer_id, true)
                }
                d if d <= 0.5 => (layer_id, false),
                _ => (layer_id + 1, false),
            }
        };

        // When we reverse an edge, then we e.g. move it from incoming to outgoing, or from
        // outgoing to incoming. To ensure that we don't look at edges that we just turned around,
        // or actually invalidate the iterator, we just move the values into two other buffers.
        // Note: We could also use an index and iterate, but accessing
        // graph.nodes[node_ix].incoming all the time in the loop seems like a bit wasteful,
        // so maybe the buffered variant is even faster.
        incoming_buffer.clear();
        incoming_buffer.extend_from_slice(&data_node.incoming);
        outgoing_buffer.clear();
        outgoing_buffer.extend_from_slice(&data_node.outgoing);

        let layer_id = data_node.layer_id.0;
        let uses_half_layer = data_node.uses_half_layer;

        for &edge_id in &incoming_buffer {
            let other_layer_id = graph.nodes[graph.edges[edge_id.0].from.0].layer_id.0;
            if other_layer_id > layer_id {
                graph.reverse_edge(edge_id);
            }
        }

        for &edge_id in &outgoing_buffer {
            let other_layer_id = graph.nodes[graph.edges[edge_id.0].to.0].layer_id.0;
            if other_layer_id < layer_id || other_layer_id == layer_id && uses_half_layer {
                graph.reverse_edge(edge_id);
            }
        }
    }

    // TODO Handle MAX_NODES_PER_LAYER. When too many data objects pile up in the same layer, they
    // need to be spread to the left and right. This is not trivial, however. If there are 10
    // parallel sequence flows in the lane at that layer, then having 2 per flow = 20 data objects
    // in total in that layer is ok. However, if there is just one sequence flow, then this needs
    // to be spread out. Now, the complexity is to determine how many data objects are truly
    // assigned to a specific sequence flow lane, or whether it is just placed here as the two
    // recipients are spread far away. Probably it makes sense to allow 2 per sequence flow and
    // then two "floating" ones. Or one could dictate that floating ones actually don't spread
    // across gateways, but this seems like a rather random restriction.
}

fn handle_vertical_lane_crossings(
    graph: &Graph,
    vars: &mut ProblemVariables,
    constraints: &mut Vec<Constraint>,
) {
    let mut all_same_layer_lane_crossings = graph
        .layout_constraints
        .same_layer_clusters
        .iter()
        .flat_map(|cluster| same_layer_lane_crossings_within_cluster(graph, cluster))
        .collect::<Vec<_>>();
    all_same_layer_lane_crossings.sort_unstable();

    let mut already_constrained = HashSet::new();
    for crossing in &all_same_layer_lane_crossings {
        let in_between_lane_range =
            crossing.top_pool_lane.lane.0 + 1..crossing.bot_pool_lane.lane.0;
        for in_between_lane in in_between_lane_range {
            for node_id in &graph.pools[crossing.top_pool_lane.pool].lanes[in_between_lane].nodes {
                let node_id_1 = (*node_id).min(crossing.top_node_id);
                let node_id_2 = (*node_id).max(crossing.top_node_id);
                assert_ne!(node_id_1, node_id_2);

                if !already_constrained.insert((node_id_1, node_id_2)) {
                    // Duplicate.
                    continue;
                }

                force_different_layers(
                    &n!(node_id_1),
                    &n!(node_id_2),
                    vars,
                    constraints,
                    graph.nodes.len(),
                );
            }
        }
    }
    for [left, right] in all_same_layer_lane_crossings.array_windows() {
        assert_ne!(left, right); // Make sure that the construction is correct.

        if left.top_pool_lane == left.bot_pool_lane {
            // This cannot result in a problem. The `right_*` could be on the same
            // pool_lane, or just on some next.
            continue;
        }
        if !(left.top_pool_lane == right.top_pool_lane && left.bot_pool_lane == right.bot_pool_lane)
        {
            // The other case is covered by the above lane-crossing check already.
            // So in here we are only left with lane crossings that span from the same lane
            // to the same other lane.
            continue;
        }

        let node_id_1 = left.top_node_id.min(right.top_node_id);
        let node_id_2 = left.top_node_id.max(right.top_node_id);
        if !already_constrained.insert((node_id_1, node_id_2)) {
            // Duplicate.
            continue;
        }
        force_different_layers(
            &n!(node_id_1),
            &n!(node_id_2),
            vars,
            constraints,
            graph.nodes.len(),
        );
    }
}

fn force_different_layers(
    a: &Node,
    b: &Node,
    vars: &mut ProblemVariables,
    constraints: &mut Vec<Constraint>,
    total_num_nodes: usize,
) {
    // We want: a != b.
    //  <==> a < b || a > b
    // So we have a boolean `z`, and an `M` which is larger than any value which `a` or `b` can ever
    // become (total number of nodes):
    //   a < b + M * z
    //   b < a + M * (1 - z)
    //
    // z == 0:           z == 1:
    //   a < b             (a < b + M)
    //   (b < a + M)       b < a
    //
    // Since there is only `<=` and `>=` in the solver, no `<` or `>`, rewrite it:
    //   a + 1 <= b + M * z
    //   b + 1 <= a + M * (1 - z)
    //
    //
    d!(eprintln!("different layers: {} - {}", a.id.0, b.id.0));
    let z = vars.add(variable().binary());
    constraints.push((aux(a) + 1).leq(aux(b) + total_num_nodes as f64 * z));
    constraints.push((aux(b) + 1).leq(aux(a) + total_num_nodes as f64 * (1 - z)));
}

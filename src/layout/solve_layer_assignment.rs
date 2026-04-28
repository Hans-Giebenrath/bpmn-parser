use crate::common::graph::EdgeId;
use crate::common::node::NodePhaseAuxData;
use crate::layout::constraint::{Above, LeftOf, SameLayer};
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
    for edge in graph
        .edges
        .iter_mut()
        .filter(|edge| edge.is_sequence_flow())
    {
        let from_var = aux(&n!(edge.from));
        let to_var = aux(&n!(edge.to));

        // Favor short edges
        objective += to_var - from_var;
        d!(eprintln!("minimize n({}) -> n({})", edge.from.0, edge.to.0));
    }

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

    let mut problem = vars.minimise(objective).using(default_solver);
    //let mut problem = problem.set_verbose(true);
    //problem.set_parameter("loglevel", "0");

    graph
        .edges
        .iter_mut()
        .enumerate()
        .filter(|(edge_idx, edge)| {
            edge.is_sequence_flow() && !graph.computed_back_edges.contains(&EdgeId(*edge_idx))
        })
        .map(|(_, edge)| (edge.from, edge.to, "regular edge"))
        .chain(
            graph
                .layout_constraints
                .left_of
                .iter()
                .map(|constraint| (constraint.left, constraint.right, "left-of constraint")),
        )
        .for_each(|(left, right, msg)| {
            let from_var = aux(&n!(left));
            let to_var = aux(&n!(right));
            d!(eprintln!(
                "constraint n({}) leftof n({}) ({msg})",
                left.0, right.0
            ));
            problem.add_constraint((to_var - from_var).geq(1));
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
            problem.add_constraint((aux(&n!(n0)) - aux(&n!(n1))).eq(0));
        });

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
                d if d <= 0.75 => (layer_id, true),
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

use crate::common::node::LayerId;
use std::collections::HashMap;

use crate::common::edge::{Edge, FlowType};
use crate::common::graph::Graph;
use crate::common::lane::Lane;
use crate::common::node::Node;
use good_lp::*;

pub fn solve_layer_assignment(graph: &mut Graph) {
    dbg!(&graph);
    for pool in &mut graph.pools {
        for lane in &mut pool.lanes {
            solve_layers(&mut graph.nodes, &graph.edges, lane, &mut graph.num_layers);
        }
    }

    solve_data_object_layers_via_arithmetic_mean(graph);
}

// TODO Imagine this situation:
//   Task1 in Lane1 -> TaskA in Lane2 -> TaskB in Lane2 -> Task2 in LaneA
// There needs to be a left-right constraint between Task1 and Task2, even though
// they have no direct connection. Probably through some reachability analysis.
// Not yet done as this may be a rare situation.
fn solve_layers(nodes: &mut [Node], edges: &[Edge], lane: &mut Lane, num_layers: &mut usize) {
    let mut vars = variables!();
    let mut layer_vars = HashMap::new();
    // Sequence flows, within the same
    let nodes_iter = lane
        .nodes
        .iter()
        .cloned()
        .map(|node_id| &nodes[node_id])
        .filter(|node| !node.is_data());
    let edges = nodes_iter.clone().flat_map(|from| {
        from.outgoing
            .iter()
            .cloned()
            .map(|edge_id| &edges[edge_id])
            .filter(|edge| edge.flow_type == FlowType::SequenceFlow)
            .map(|edge| &nodes[dbg!(&edge).to.0])
            .map(|to| (from.id, to.id))
    });

    let max_layer = lane.nodes.len() as f64;

    nodes_iter.clone().for_each(|n| {
        let n = n.id;
        let layer_var = vars.add(variable().integer().min(0).max(max_layer));
        layer_vars.insert(n, layer_var);
    });

    let mut objective = Expression::from(0.0);
    for (from, to) in edges.clone() {
        let from_var = *layer_vars.get(&from).unwrap();
        let to_var = *layer_vars.get(&to).unwrap();

        objective += to_var - from_var;
    }

    let mut problem = vars.minimise(objective).using(default_solver);
    problem.set_parameter("loglevel", "0");

    for (from, to) in edges {
        let from_var = *layer_vars.get(&from).unwrap();
        let to_var = *layer_vars.get(&to).unwrap();

        problem = problem.with((to_var - from_var).geq(1));
    }

    let solution = problem.solve().unwrap();
    let mut smallest_layer = usize::MAX;
    for layer_var in layer_vars.values() {
        let layer_value = solution.value(*layer_var) as usize;
        smallest_layer = smallest_layer.min(layer_value);
    }

    for (node_id, layer_var) in layer_vars {
        let layer_value = solution.value(layer_var) as usize;
        nodes[node_id].layer_id =
            LayerId(layer_value.checked_sub(smallest_layer).expect(
                "Did the solver change the value of the returned values between iterations?",
            ));
        smallest_layer = smallest_layer.min(layer_value);
    }

    if let Some(largest_layer) = lane.nodes.iter().map(|n| nodes[n.0].layer_id.0).max() {
        *num_layers = (*num_layers).max(largest_layer + 1);
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

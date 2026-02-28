use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::Edge;
use crate::common::edge::EdgeType;
use crate::common::graph::EdgeId;
use crate::common::graph::{Graph, NodeId};
use crate::common::node::LayerId;
use crate::common::node::NodeType;

pub(crate) struct Undo {
    original_num_nodes: usize,
    original_edges: Vec<(EdgeId, Edge)>,
}

pub(crate) fn temporarily_add_dummy_nodes_for_edges_within_same_layer(graph: &mut Graph) -> Undo {
    let original_edge_count = graph.edges.len();
    let mut undo = Undo {
        original_num_nodes: graph.nodes.len(),
        original_edges: Vec::new(),
    };

    for edge_id in (0..original_edge_count).map(EdgeId) {
        let edge = &mut graph.edges[edge_id];
        let from = &graph.nodes[edge.from];
        let to = &graph.nodes[edge.to];
        if edge.is_replaced_by_dummies() {
            continue;
        }
        if from.layer_id != to.layer_id {
            continue;
        }
        if from.pool != to.pool {
            dbg!(&"TODO");
            continue;
        }
        if from.is_data_with_only_one_edge() || to.is_data_with_only_one_edge() {
            // These are processed specially.
            continue;
        }
        let layer_id = from.layer_id;
        let pool_and_lane = from.pool_and_lane();

        undo.original_edges.push((edge_id, edge.clone()));
        let right_node_id = graph.add_node(
            NodeType::LongEdgeDummy,
            pool_and_lane,
            Some(LayerId(layer_id.0 + 1)),
        );
        let left_node_id = if layer_id.0 > 0 {
            Some(graph.add_node(
                NodeType::LongEdgeDummy,
                pool_and_lane,
                Some(LayerId(layer_id.0 - 1)),
            ))
        } else {
            // No room to the left. In that case we anyway don't gain anything from having
            // additional dummy nodes there, since it wouldn't cross with anything (or at least it
            // would not be meaningful I think).
            None
        };
        reroute_vertical_edge(graph, edge_id, left_node_id, right_node_id);
    }

    undo
}

pub(crate) fn remove_temporarily_added_dummy_nodes_for_edges_within_same_layer(
    graph: &mut Graph,
    undo: Undo,
) {
    // also fix the `node_below_in_same_lane` etc properties
    let mut num_edges_to_retain: usize = graph.edges.len() - undo.original_edges.len();
    let num_nodes_to_retain: usize = undo.original_num_nodes;
    for pool in &mut graph.pools {
        for lane in &mut pool.lanes {
            lane.nodes.retain(|node_id| node_id.0 < num_nodes_to_retain);
        }
    }

    // Undo the changes in reverse order, important! Otherwise,,, this will crash for snake nodes.
    for (rerouted_edge_id, original_edge_value) in undo.original_edges.into_iter().rev() {
        // I have a feeling that this can be written more cleanly, need to ask an AI.
        let to_node_id = original_edge_value.to;
        let from_node_id = original_edge_value.from;
        graph.edges[rerouted_edge_id] = original_edge_value;
        // XXX don't move this down, as there is a `to_node.incoming.push(.)`.
        if graph.nodes[to_node_id].layer_id.0 > 0 {
            graph.nodes[to_node_id].incoming.pop();
            graph.nodes[from_node_id].incoming.pop();
            num_edges_to_retain -= 2;
        }
        let to_node = &mut graph.nodes[to_node_id];
        to_node.outgoing.pop();
        to_node.incoming.push(rerouted_edge_id);
    }

    for node_id in (num_nodes_to_retain..graph.nodes.len()).map(NodeId) {
        let intermediate_node = &graph.nodes[node_id];
        let node_above_in_same_lane = intermediate_node.node_above_in_same_lane;
        let node_below_in_same_lane = intermediate_node.node_below_in_same_lane;
        match (node_above_in_same_lane, node_below_in_same_lane) {
            (Some(above), Some(below)) => {
                let node_above = &graph.nodes[above];
                let node_below = &graph.nodes[below];
                if node_above.pool == node_below.pool && node_above.lane == node_below.lane {
                    graph.nodes[above].node_below_in_same_lane = node_below_in_same_lane;
                    graph.nodes[below].node_above_in_same_lane = node_above_in_same_lane;
                } else {
                    graph.nodes[above].node_below_in_same_lane = None;
                    graph.nodes[below].node_above_in_same_lane = None;
                }
            }
            (Some(above), None) => {
                graph.nodes[above].node_below_in_same_lane = None;
            }
            (None, Some(below)) => {
                graph.nodes[below].node_above_in_same_lane = None;
            }
            (None, None) => (),
        }
    }

    graph.nodes.truncate(num_nodes_to_retain);
    graph.edges.truncate(num_edges_to_retain);
}

fn reroute_vertical_edge(
    graph: &mut Graph,
    to_be_rerouted_edge_id: EdgeId,
    left_intermediate_node_id: Option<NodeId>,
    right_intermediate_node_id: NodeId,
) {
    let edge = &mut graph.edges[to_be_rerouted_edge_id];
    let from_node_id = edge.from;
    let to_node_id = std::mem::replace(&mut edge.to, right_intermediate_node_id);
    let flow_type = edge.flow_type.clone();
    // These value are not used within this phase, so just put something random here.
    let is_reversed = false;
    let stays_within_lane = false;

    let right_second_edge_id = EdgeId(graph.edges.len());
    graph.edges.push(Edge {
        from: to_node_id,
        to: right_intermediate_node_id,
        edge_type: EdgeType::DummyEdge {
            original_edge: to_be_rerouted_edge_id,
            bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
        },
        flow_type: flow_type.clone(),
        is_reversed,
        stays_within_lane,
        stroke_color: None,
        is_vertical: false,
        attached_to_boundary_event: None,
    });

    let to_node = &mut graph.nodes[to_node_id];
    let Some(i) = to_node
        .incoming
        .iter()
        .position(|i| *i == to_be_rerouted_edge_id)
    else {
        // TODO add some debug printing
        unreachable!(
            "to-be-rerouted edge: {}, from node: {}, to node: {}",
            to_be_rerouted_edge_id.0, from_node_id.0, to_node_id.0
        );
    };
    to_node.incoming.remove(i);
    to_node.outgoing.push(right_second_edge_id);

    let right_intermediate_node = &mut graph.nodes[right_intermediate_node_id];
    right_intermediate_node
        .incoming
        .push(to_be_rerouted_edge_id);
    right_intermediate_node.incoming.push(right_second_edge_id);

    if let Some(left_intermediate_node_id) = left_intermediate_node_id {
        let left_first_edge_id = EdgeId(graph.edges.len());
        graph.edges.push(Edge {
            from: left_intermediate_node_id,
            to: from_node_id,
            edge_type: EdgeType::DummyEdge {
                original_edge: to_be_rerouted_edge_id,
                bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
            },
            flow_type: flow_type.clone(),
            is_reversed,
            stays_within_lane,
            stroke_color: None,
            is_vertical: false,
            attached_to_boundary_event: None,
        });

        let left_second_edge_id = EdgeId(graph.edges.len());
        graph.edges.push(Edge {
            from: left_intermediate_node_id,
            to: to_node_id,
            edge_type: EdgeType::DummyEdge {
                original_edge: to_be_rerouted_edge_id,
                bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
            },
            flow_type,
            is_reversed,
            stays_within_lane,
            stroke_color: None,
            is_vertical: false,
            attached_to_boundary_event: None,
        });

        let left_intermediate_node = &mut graph.nodes[left_intermediate_node_id];
        left_intermediate_node.outgoing.push(left_first_edge_id);
        left_intermediate_node.outgoing.push(left_second_edge_id);

        graph.nodes[from_node_id].incoming.push(left_first_edge_id);
        graph.nodes[to_node_id].incoming.push(left_second_edge_id);
    }
}

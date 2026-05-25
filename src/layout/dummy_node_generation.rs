use std::collections::HashSet;

use crate::common::bpmn_node::BoundaryEvent;
use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::EdgeType;
use crate::common::edge::FlowType;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::graph::NodeId;
use crate::common::graph::PoolAndLane;
use crate::common::node::LayerId;
use crate::common::node::NodeType;

use proc_macros::n;

pub fn dummy_node_generation(graph: &mut Graph) {
    // After this function we will have a bunch of new temporary edges which make some of the
    // real edges "obsolete" - they will be marked as "replaced_by_dummies" but still kept.
    // New edges are added to graph.edges, so we can't iterate over it at the same time.
    // Hence, we store the number.

    let mut lone_looping_gateway_edges = HashSet::new();
    for node in &graph.nodes {
        if node.is_gateway() {
            let lone_edge = match (&node.incoming[..], &node.outgoing[..]) {
                ([lone_edge], _) | (_, [lone_edge])
                    if graph.computed_back_edges.contains(lone_edge) =>
                {
                    lone_edge
                }
                _ => continue,
            };
            lone_looping_gateway_edges.insert((node.id, *lone_edge));
        }
    }

    let num_real_edges = graph.edges.len();
    for edge_id in (0..num_real_edges).map(EdgeId) {
        let current_num_edges = graph.edges.len();
        let edge = &mut graph.edges[edge_id];
        let from_id = edge.from;
        let to_id = edge.to;
        let from = &graph.nodes[from_id];
        let to = &graph.nodes[to_id];
        let pool = from.pool;

        // The edge spans just a single layer -> ignore.
        if from.layer_id.0 + 1 == to.layer_id.0 {
            continue;
        } else if from.layer_id == to.layer_id {
            // This is definitely a vertical edge:
            // (1) Graph validation only allows moving those connected edges onto the same layer
            // which have only two such same-layer connected nodes, i.e. they form a list.
            // (2) Further, during the node reordering there is a check for whether no other nodes
            // are forced between two same-layer connected nodes via above constraints.
            edge.is_vertical = true;
            continue;
        }

        // The edge is a message edge that spans across pools, this is handled differently.
        if from.pool != to.pool {
            assert!(edge.is_message_flow());
            continue;
        }

        assert!(!edge.is_message_flow());
        let flow_type = edge.flow_type.clone();
        let boundary_event = edge.attached_to_boundary_event.clone();
        let from_coords = from.coord3();
        let to_coords = to.coord3();
        let EdgeType::Regular { text, .. } = &edge.edge_type else {
            unreachable!();
        };
        let text = text.clone();
        edge.edge_type = EdgeType::ReplacedByDummies {
            first_dummy_edge: EdgeId(current_num_edges),
            text,
        };

        if let Some(total_edge_count) = to.layer_id.0.checked_sub(from.layer_id.0)
            && total_edge_count > 0
        {
            let total_node_count = total_edge_count.strict_sub(1);
            insert_dummy_nodes(
                graph,
                total_node_count,
                flow_type,
                from_id,
                to_id,
                edge_id,
                boundary_event,
            );
        } else {
            // Transform it from the left to the right.
            // ```
            //                         to
            //  to                      ┌┐
            //   ┌┐                     └▲
            // ┌─►┘                      │           dummy_from_node
            // │                        ┌┼────►┬─────►┐
            // └───────────┐            └┘    └┘     └▲
            //          ┌┬─┘     dummy_to_node        │
            //          └┘                           ┌┤
            //          from                         └┘
            //                                      from
            // ```
            //
            // This way the `from` node has that edge still as an outgoing edge, and the `to` node
            // has it still as an incoming edge.

            // `equals` case checked already earlier.
            let total_node_count = from
                .layer_id
                .0
                .checked_sub(to.layer_id.0 + 1)
                .expect("`equals` case was checked already earlier");
            let from_does_not_need_back_edge_corner_dummy =
                from.is_gateway() && !lone_looping_gateway_edges.contains(&(from.id, edge_id));
            let to_does_not_need_back_edge_corner_dummy =
                to.is_gateway() && !lone_looping_gateway_edges.contains(&(to.id, edge_id));

            let dummy_from_node_id = if from_does_not_need_back_edge_corner_dummy {
                from_id
            } else {
                let dummy_node_id = graph.add_node(
                    NodeType::BackEdgeCornerDummy {
                        same_layer_real_node_id: from_id,
                        same_layer_edge_id: EdgeId(current_num_edges),
                        left_one: false,
                    },
                    PoolAndLane {
                        pool,
                        // Yes, `to.lane`, as I believe it is visually clearer to directly go to the
                        // target lane. But I might be wrong.
                        lane: to_coords.pool_and_lane.lane,
                    },
                    Some(from_coords.layer),
                );
                graph.add_edge(
                    from_id,
                    dummy_node_id,
                    EdgeType::DummyEdge {
                        original_edge: edge_id,
                        bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                    },
                    flow_type.clone(),
                    boundary_event,
                );
                dummy_node_id
            };

            let dummy_to_node_id = if to_does_not_need_back_edge_corner_dummy {
                to_id
            } else {
                let dummy_node_id = graph.add_node(
                    NodeType::BackEdgeCornerDummy {
                        same_layer_real_node_id: to_id,
                        same_layer_edge_id: EdgeId(current_num_edges + 1),
                        left_one: true,
                    },
                    PoolAndLane {
                        pool,
                        lane: to_coords.pool_and_lane.lane,
                    },
                    Some(to_coords.layer),
                );
                graph.add_edge(
                    dummy_node_id,
                    to_id,
                    EdgeType::DummyEdge {
                        original_edge: edge_id,
                        bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                    },
                    flow_type.clone(),
                    None,
                );
                dummy_node_id
            };

            let old_num_edges = graph.edges.len();
            // Need to add the nodes at the target node's lane. I hypothesize that this creates
            // better visual results compared to switching lanes on half the way as is done for the
            // forward-looking edges.
            insert_dummy_nodes(
                graph,
                total_node_count, /* possibly 0, but that is ok, just creates an edge */
                flow_type,
                // We are now going "backwards" wrt the original arrow, but left-to-right on the
                // diagram level.
                dummy_to_node_id,
                dummy_from_node_id,
                edge_id,
                None,
            );
            for edge in graph.edges.iter_mut().skip(old_num_edges) {
                edge.is_reversed = true;
            }
        }
        graph.nodes[from_id]
            .outgoing
            .retain(|outgoing_edge_idx| *outgoing_edge_idx != edge_id);
        graph.nodes[to_id]
            .incoming
            .retain(|incoming_edge_idx| *incoming_edge_idx != edge_id);
    }
}

fn insert_dummy_nodes(
    graph: &mut Graph,
    // May be zero.
    total_node_count: usize,
    flow_type: FlowType,
    from_id: NodeId,
    to_id: NodeId,
    original_edge_id: EdgeId,
    mut boundary_event: Option<BoundaryEvent>,
) {
    let from = &n!(from_id);
    let to = &n!(to_id);
    let node_lane_ids = {
        let node_count_in_from_lane = total_node_count / 2;
        let node_count_in_to_lane = total_node_count - node_count_in_from_lane;
        // If from and to are in the same lane, then they simply return the same value.
        std::iter::repeat_n(from.lane, node_count_in_from_lane)
            .chain(std::iter::repeat_n(to.lane, node_count_in_to_lane))
    };

    let mut previous_node_id = from_id;
    let mut layer_id = LayerId(from.layer_id.0 + 1);
    let pool = from.pool;
    let to_node_id = to_id;
    for lane_id in node_lane_ids {
        let dummy_node_id = graph.add_node(
            NodeType::LongEdgeDummy,
            PoolAndLane {
                pool,
                lane: lane_id,
            },
            Some(layer_id),
        );
        layer_id.0 += 1;

        graph.add_edge(
            previous_node_id,
            dummy_node_id,
            EdgeType::DummyEdge {
                original_edge: original_edge_id,
                bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
            },
            flow_type.clone(),
            boundary_event.take(),
        );
        previous_node_id = dummy_node_id;
    }
    graph.add_edge(
        previous_node_id,
        to_node_id,
        EdgeType::DummyEdge {
            original_edge: original_edge_id,
            bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
        },
        flow_type,
        None,
    );
}

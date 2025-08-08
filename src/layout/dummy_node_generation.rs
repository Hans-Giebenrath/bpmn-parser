use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::EdgeType;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::node::LayerId;
use crate::common::node::NodeType;

pub fn generate_dummy_nodes(graph: &mut Graph) {
    // After this function we will have a bunch of new temporary edges which make some of the
    // real edges "obsolete" - they will be marked as "replaced_by_dummies" but still kept.
    // New edges are added to graph.edges, so we can't iterate over it at the same time.
    // Hence we store the number.
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
        }

        if from.layer_id == to.layer_id {
            // This is handled in the crossing minimization phase. The respective
            // transformation is only useful for the ILP, but afterwards having direct connections
            // is actually simpler, compared to the decomposed version.
            continue;
        }

        // The edge is a message edge that spans across pools, this is handled differently.
        if from.pool != to.pool {
            continue;
        }
        assert!(!edge.is_message_flow());

        // Use strict_sub to catch problems with wrong arrow directions. They should always be
        // pointing right.
        let total_edge_count = to.layer_id.0.strict_sub(from.layer_id.0);
        let total_node_count = total_edge_count.strict_sub(1);

        let node_lane_ids = {
            let node_count_in_from_lane = total_node_count / 2;
            let node_count_in_to_lane = total_node_count - node_count_in_from_lane;
            // If from and to are in the same lane, then they simply return the same value.
            std::iter::repeat_n(from.lane, node_count_in_from_lane)
                .chain(std::iter::repeat_n(to.lane, node_count_in_to_lane))
        };

        let EdgeType::Regular { text, .. } = &edge.edge_type else {
            unreachable!();
        };
        let text = text.clone();
        let flow_type = edge.flow_type.clone();
        edge.edge_type = EdgeType::ReplacedByDummies {
            first_dummy_edge: EdgeId(current_num_edges),
            text,
        };

        let mut previous_node_id = from.id;
        let mut layer_id = LayerId(from.layer_id.0 + 1);
        let to_node_id = to.id;
        for lane_id in node_lane_ids {
            let dummy_node_id = graph.add_node(NodeType::DummyNode, pool, lane_id);
            graph.nodes[dummy_node_id].layer_id = layer_id;
            layer_id.0 += 1;

            graph.add_edge(
                previous_node_id,
                dummy_node_id,
                EdgeType::DummyEdge {
                    original_edge: edge_id,
                    bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                },
                flow_type.clone(),
            );
            previous_node_id = dummy_node_id;
        }
        graph.add_edge(
            previous_node_id,
            to_node_id,
            EdgeType::DummyEdge {
                original_edge: edge_id,
                bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
            },
            flow_type,
        );
        graph.nodes[from_id].outgoing.retain(|outgoing_edge_idx| *outgoing_edge_idx != edge_id);
        graph.nodes[to_id].incoming.retain(|incoming_edge_idx| *incoming_edge_idx != edge_id);
    }
}

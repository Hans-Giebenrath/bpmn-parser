use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::EdgeType;
use crate::common::edge::RegularEdgeBendPoints;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::graph::MAX_NODE_WIDTH;
use crate::common::node::AbsolutePort;
use crate::common::node::NodeIdOrEdgeId;
use proc_macros::e;

// Assigns bend points to the Regular edges. Afterwards, no more dummy nodes or edges are present.
pub fn dummy_node_removal(graph: &mut Graph) {
    for edge_id in (0..graph.edges.len()).map(EdgeId) {
        let edge = &mut graph.edges[edge_id];
        let EdgeType::ReplacedByDummies {
            first_dummy_edge,
            text,
        } = &mut edge.edge_type
        else {
            continue;
        };
        let text = text.clone();
        let first_dummy_edge_id = *first_dummy_edge;
        let AbsolutePort {
            x: from_x,
            y: from_y,
        } = graph.nodes[edge.from].port_of_outgoing(first_dummy_edge_id);
        let to = edge.to;
        let mut bend_points = vec![(from_x, from_y)];
        let mut cur_dummy_edge_id = first_dummy_edge_id;
        // This is used to access to `to` port.
        let mut next_node_id = e!(cur_dummy_edge_id).to;
        let mut loop_protector = graph.endless_graph_traversal_protector();
        // This loop hops along the edges via node.incoming/.outgoing, as dummy edges might
        // not necessarily be consecutive in `graph.edges`.
        loop {
            loop_protector(graph);

            let edge = &graph.edges[cur_dummy_edge_id];
            let dummy_bend_points = if let EdgeType::DummyEdge {
                original_edge,
                bend_points,
            } = &edge.edge_type
                && *original_edge == edge_id
            {
                bend_points
            } else {
                break;
            };
            match dummy_bend_points {
                DummyEdgeBendPoints::ToBeDeterminedOrStraight => {
                    // Nothing to do, as this is straight we don't add any bend points.
                    // So just go on jumping to the next edge.
                }
                DummyEdgeBendPoints::SegmentEndpoints(segment_from, segment_to) => {
                    bend_points.push(*segment_from);
                    bend_points.push(*segment_to);
                }
                DummyEdgeBendPoints::VerticalBendDummy(segment) => bend_points.push(*segment),
                DummyEdgeBendPoints::VerticalCollapsed => { /* empty */ }
            };

            let next_node = &graph.nodes[next_node_id];
            if !next_node.is_any_dummy() {
                break;
            }
            (cur_dummy_edge_id, next_node_id) =
                next_node.hop_to_next_node(graph, NodeIdOrEdgeId::EdgeId(cur_dummy_edge_id));
        }
        let AbsolutePort { x: to_x, y: to_y } = graph.nodes[to].port_of_incoming(cur_dummy_edge_id);
        bend_points.push((to_x, to_y));
        // The longer edges should be constructed in an ideal way. Vertical edge segments use
        // `VerticalBendDummy` where they just record the port of the bend dummy.
        // Fully vertical `SnakeEdgeBisectDummy`s are already stitched together in an earlier phase.
        debug_assert!(
            bend_points
                .array_windows()
                .all(|[left, right]| left != right),
            "bend_points: {bend_points:?}"
        );

        let edge = &mut graph.edges[edge_id];
        if edge.is_reversed {
            bend_points.reverse();
        }
        edge.edge_type = EdgeType::Regular {
            text,
            bend_points: RegularEdgeBendPoints::FullyRouted(bend_points),
        };
    }

    // Remove all the unneeded dummy nodes in the end. Otherwise, it becomes too noisy to filter
    // them away in the output phase. This basically reshapes the graph into the same thing which we
    // got after parsing.
    while graph.nodes.pop_if(|node| node.is_any_dummy()).is_some() {}
    // Then fix the dummy edge references in incoming and outgoing.
    for node in &mut graph.nodes {
        let incoming = std::mem::take(&mut node.incoming);
        let incoming = incoming
            .into_iter()
            .map(|edge_id| match graph.edges[edge_id].edge_type {
                EdgeType::DummyEdge { original_edge, .. } => original_edge,
                EdgeType::ReplacedByDummies { .. } => {
                    unreachable!("should be converted back to a Regular in the loop above")
                }
                EdgeType::Regular { .. } => edge_id,
            })
            .collect::<Vec<_>>();
        node.incoming = incoming;

        let outgoing = std::mem::take(&mut node.outgoing);
        let outgoing = outgoing
            .into_iter()
            .map(|edge_id| match graph.edges[edge_id].edge_type {
                EdgeType::DummyEdge { original_edge, .. } => original_edge,
                EdgeType::ReplacedByDummies { .. } => {
                    unreachable!("should be converted back to a Regular in the loop above")
                }
                EdgeType::Regular { .. } => edge_id,
            })
            .collect::<Vec<_>>();
        node.outgoing = outgoing;
    }

    // And now remove all the dummy edges, which should not be referenced any longer at this point.
    while graph.edges.pop_if(|edge| edge.is_dummy()).is_some() {}

    for pool in &mut graph.pools {
        for lane in &mut pool.lanes {
            lane.nodes.retain(|node_id| node_id.0 < graph.nodes.len());
        }
    }

    assert!(!graph.nodes.iter().any(|n| n.is_any_dummy()));
    assert!(graph.edges.iter().all(|e| e.is_regular()));
    // println!("Graph: {graph:?}");
}

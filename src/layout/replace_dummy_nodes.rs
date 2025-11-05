use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::EdgeType;
use crate::common::edge::RegularEdgeBendPoints;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::node::AbsolutePort;
use proc_macros::n;

// Assigns bend points to the Regular edges. Afterwards, no more dummy nodes or edges are present.
pub fn replace_dummy_nodes(graph: &mut Graph) {
    for edge_id in (0..graph.edges.len()).map(EdgeId) {
        let edge = &mut graph.edges[edge_id];
        match &mut edge.edge_type {
            EdgeType::DummyEdge { .. } => {
                // skipped - these are evaluated in the context of long edges / the
                // ReplacedByDummies match arm.
            }
            // TODO all the logic in here should be moved to `edge_routing` as this is not part of
            // dummy node replacement.
            EdgeType::Regular { bend_points, .. } => {
                // A lambda because in the FullyRouted case the port indexing doesn't work,
                // so compute it lazily.
                let from_and_to_xy = || {
                    let from_xy = n!(edge.from).port_of_outgoing(edge_id).as_pair();
                    let to_xy = n!(edge.to).port_of_incoming(edge_id).as_pair();
                    (from_xy, to_xy)
                };
                match bend_points {
                    RegularEdgeBendPoints::FullyRouted(_) => continue,
                    RegularEdgeBendPoints::ToBeDeterminedOrStraight => {
                        let (from_xy, to_xy) = from_and_to_xy();
                        let mut points = vec![from_xy, to_xy];
                        if edge.is_reversed {
                            points.reverse();
                        }
                        *bend_points = RegularEdgeBendPoints::FullyRouted(points);
                    }
                    RegularEdgeBendPoints::SegmentEndpoints(segment_from, segment_to) => {
                        let (from_xy, to_xy) = from_and_to_xy();
                        let mut points = vec![from_xy, *segment_from, *segment_to, to_xy];
                        if edge.is_reversed {
                            points.reverse();
                        }
                        *bend_points = RegularEdgeBendPoints::FullyRouted(points);
                    }
                }
            }
            EdgeType::ReplacedByDummies {
                first_dummy_edge,
                text,
            } => {
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
                let mut last_dummy_edge_id = cur_dummy_edge_id;
                // This loop hops along the edges via node.incoming/.outgoing, as dummy edges might
                // not necessarily be consecutive in `graph.edges`.
                loop {
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
                    last_dummy_edge_id = cur_dummy_edge_id;
                    match dummy_bend_points {
                        DummyEdgeBendPoints::ToBeDeterminedOrStraight => {
                            // Nothing to do, as this is straight we don't add any bend points.
                            // So just go on jumping to the next edge.
                        }
                        DummyEdgeBendPoints::SegmentEndpoints(segment_from, segment_to) => {
                            bend_points.push(*segment_from);
                            bend_points.push(*segment_to);
                        }
                        DummyEdgeBendPoints::VerticalBendDummy(segment) => {
                            bend_points.push(*segment)
                        }
                        DummyEdgeBendPoints::VerticalCollapsed => { /* empty */ }
                    };
                    let to_node = &graph.nodes[edge.to];
                    let mut next_edge_it = to_node
                        .incoming
                        .iter()
                        .chain(to_node.outgoing.iter())
                        .filter(|e| **e != cur_dummy_edge_id);
                    match (next_edge_it.next(), next_edge_it.next()) {
                        (Some(_), Some(_)) => {
                            // There are multiple other edges, which means we reached some target node
                            // which is not a dummy node (dummies only have two connected edges, one of
                            // them being filtered out). So we are done.
                            break;
                        }
                        (None, _) => {
                            // Done as well, probably reached an end node.
                            break;
                        }
                        (Some(e), None) => {
                            // Only one success edge is present, here. This could mean that the
                            // original ReplacedByDummies edge continues, but not necessarily.
                            // This is checked in the `match` in the beginning of the loop.
                            cur_dummy_edge_id = *e;
                        }
                    }
                }
                let AbsolutePort { x: to_x, y: to_y } =
                    graph.nodes[to].port_of_incoming(last_dummy_edge_id);
                bend_points.push((to_x, to_y));
                // Vertical lines ([Edge.is_vertical]) have their endpoints as their recorded
                // `bend_points`. This means that they will duplicate info in `from` and `to`.
                // So remove the duplicates.
                bend_points.dedup();

                let edge = &mut graph.edges[edge_id];
                if edge.is_reversed {
                    bend_points.reverse();
                }
                edge.edge_type = EdgeType::Regular {
                    text,
                    bend_points: RegularEdgeBendPoints::FullyRouted(bend_points),
                };
            }
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

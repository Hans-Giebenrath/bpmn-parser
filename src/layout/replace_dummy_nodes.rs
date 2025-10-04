use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::EdgeType;
use crate::common::edge::RegularEdgeBendPoints;
use crate::common::graph::Graph;
use crate::common::node::XY;

pub fn replace_dummy_nodes(graph: &mut Graph) {
    for edge_idx in 0..graph.edges.len() {
        let edge = &mut graph.edges[edge_idx];
        let XY {
            x: from_x,
            y: from_y,
        } = graph.nodes[edge.from.0].right_port();
        let XY { x: to_x, y: to_y } = graph.nodes[edge.to.0].left_port();
        let from = (from_x, from_y);
        let to = (to_x, to_y);
        match &mut edge.edge_type {
            EdgeType::DummyEdge { .. } => {
                // skipped - these are evaluated in the context of long edges / the
                // ReplacedByDummies match arm.
            }
            EdgeType::Regular { bend_points, .. } => match bend_points {
                RegularEdgeBendPoints::FullyRouted(_) => continue,
                RegularEdgeBendPoints::ToBeDeterminedOrStraight => {
                    let mut points = vec![from, to];
                    if edge.is_reversed {
                        points.reverse();
                    }
                    *bend_points = RegularEdgeBendPoints::FullyRouted(points);
                }
                RegularEdgeBendPoints::SegmentEndpoints(segment_from, segment_to) => {
                    let mut points = vec![from, *segment_from, *segment_to, to];
                    if edge.is_reversed {
                        points.reverse();
                    }
                    *bend_points = RegularEdgeBendPoints::FullyRouted(points);
                }
            },
            EdgeType::ReplacedByDummies {
                first_dummy_edge,
                text,
            } => {
                let mut bend_points = vec![from];
                let text = text.clone();
                let first_dummy_edge_id = *first_dummy_edge;

                let mut edge_id = first_dummy_edge_id;
                // This loop hops along the edges via node.incoming/.outgoing, as dummy edges might
                // not necessarily be consecutive in `graph.edges`.
                loop {
                    let edge = &graph.edges[first_dummy_edge_id];
                    match edge.edge_type {
                        EdgeType::DummyEdge {
                            original_edge,
                            bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                        } if original_edge.0 == edge_idx => {
                            continue;
                        }
                        EdgeType::DummyEdge {
                            original_edge,
                            bend_points:
                                DummyEdgeBendPoints::SegmentEndpoints(segment_from, segment_to),
                        } if original_edge.0 == edge_idx => {
                            bend_points.push(segment_from);
                            bend_points.push(segment_to);
                        }
                        _ => break,
                    }
                    let to_node = &graph.nodes[edge.to];
                    let mut next_edge_it = to_node
                        .incoming
                        .iter()
                        .filter(|e| **e != edge_id)
                        .chain(to_node.outgoing.iter().filter(|e| **e != edge_id));
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
                            edge_id = *e;
                        }
                    }
                }
                bend_points.push(to);

                let edge = &mut graph.edges[edge_idx];
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
    // them away in the output phase.
    while graph.nodes.pop_if(|node| node.is_dummy()).is_some() {}
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

    assert!(!graph.nodes.iter().any(|n| n.is_dummy()));
    assert!(graph.edges.iter().all(|e| e.is_regular()));
    // println!("Graph: {graph:?}");
}

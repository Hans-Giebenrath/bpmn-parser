use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::EdgeType;
use crate::common::edge::RegularEdgeBendPoints;
use crate::common::graph::Graph;
use crate::common::node::XY;

pub fn replace_dummy_nodes(graph: &mut Graph) {
    for edge_idx in 0..graph.edges.len() {
        let edge = &graph.edges[edge_idx];
        let XY {
            x: from_x,
            y: from_y,
        } = graph.nodes[edge.from.0].right_port();
        let XY { x: to_x, y: to_y } = graph.nodes[edge.to.0].left_port();
        let from = (from_x, from_y);
        let to = (to_x, to_y);
        let edge = &mut graph.edges[edge_idx];
        match &mut edge.edge_type {
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

                for edge in graph.edges.iter().skip(first_dummy_edge_id.0) {
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
            EdgeType::DummyEdge { .. } => {
                // skipped - these are evaluated in the context of long edges / the
                // ReplacedByDummies match arm.
            }
        };
    }
}

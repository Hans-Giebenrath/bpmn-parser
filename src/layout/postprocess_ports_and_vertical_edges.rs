use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::EdgeType;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::graph::NodeId;
use crate::common::node::RelativePort;
use proc_macros::e;
use proc_macros::n;

/// Gateway ports were assigned {x: node_width/2, y: node_height/2} since it is unclear at what
/// position the associated bend dummy will be, relative to the gateway node. But this is now known
/// after assigning specific y coordinates to all nodes. If the bend dummy if above/below the
/// gateway node, the port should be in the middle of the top/bottom border. If the bend dummy is
/// "within" the gateway, then the port should be at the left/right side.
/// This logic is actually applicable to all the vertical edges towards bend dummies (and corner
/// dummies which should be merged into the former type at some point). A perfectly vertical S bisect
/// edge (both sides `is_vertical`) will be fully routed within the `find_straight_edges` phase.
pub fn postprocess_ports_and_vertical_edges(graph: &mut Graph) {
    for node_id in (0..graph.nodes.len()).map(NodeId) {
        let node = &mut n!(node_id);
        if node.is_any_dummy() {
            continue;
        }
        // XXX make sure there is no `continue` down from here, otherwise these values are lost.
        let mut incoming_ports = std::mem::take(&mut node.incoming_ports);
        let mut outgoing_ports = std::mem::take(&mut node.outgoing_ports);
        let node = &n!(node_id);

        let top_border_y = node.y;
        let bottom_border_y = node.y + node.height;
        let left_side_x = 0;
        let right_side_x = node.width;
        if node.is_gateway() {
            // Set the port of vertical edges to the correct side of the gateway node, and
            // assign `VerticalBendDummy` or `VerticalCollapsed` to the vertical edge between
            // gateway and bend dummy.
            let mut process_gateway =
                |in_or_out: &[EdgeId], in_or_out_ports: &mut [RelativePort], relative_x: usize| {
                    for (edge_id, relative_port) in
                        in_or_out.iter().cloned().zip(in_or_out_ports.iter_mut())
                    {
                        let edge = &mut e!(edge_id);
                        let maybe_bend_points =
                            if let EdgeType::DummyEdge { bend_points, .. } = &mut edge.edge_type {
                                Some(bend_points)
                            } else {
                                None
                            };
                        use DummyEdgeBendPoints::*;
                        let other_node = &n!(if edge.from == node.id {
                            edge.to
                        } else {
                            edge.from
                        });
                        if !edge.is_vertical {
                            assert!(other_node.is_back_edge_corner_dummy());
                            // `y` stays the same, only `x` needs to be fixed.
                            // And `bend_points` will be assigned regularly later, we have a "full
                            // blown" edge here.
                            relative_port.x = relative_x;
                            continue;
                        }
                        let xy = (if edge.from == node.id {
                            other_node.port_of_incoming(edge_id)
                        } else {
                            other_node.port_of_outgoing(edge_id)
                        })
                        .as_pair();

                        if other_node.y < top_border_y {
                            // above
                            relative_port.y = 0;

                            if let Some(bend_points) = maybe_bend_points {
                                *bend_points = VerticalBendDummy(xy);
                            }
                        } else if other_node.y <= bottom_border_y {
                            // within
                            // This is only relevant for gateway bendpoints that can be put at the
                            // same height as the gateway, i.e. shall be vertically collapsed. The ILP
                            // construction with big M should have forced it to y/2 += rounding
                            // errors.
                            relative_port.x = relative_x;

                            if let Some(bend_points) = maybe_bend_points {
                                *bend_points = VerticalCollapsed;
                            }
                        } else {
                            // below
                            relative_port.y = node.height;

                            if let Some(bend_points) = maybe_bend_points {
                                *bend_points = VerticalBendDummy(xy);
                            }
                        }
                    }
                };

            process_gateway(&node.incoming, &mut incoming_ports, left_side_x);
            process_gateway(&node.outgoing, &mut outgoing_ports, right_side_x);
        } else {
            // Just create the `VerticalBendDummy` information for vertical boundary event edges.
            let mut process_non_gateway = |in_or_out: &[EdgeId]| {
                for edge_id in in_or_out.iter().cloned() {
                    let edge = &mut e!(edge_id);
                    if !edge.is_vertical {
                        continue;
                    }
                    let EdgeType::DummyEdge { bend_points, .. } = &mut edge.edge_type else {
                        continue;
                    };
                    use DummyEdgeBendPoints::*;
                    let other_node = &n!(if edge.from == node.id {
                        edge.to
                    } else {
                        edge.from
                    });
                    let xy = (if edge.from == node.id {
                        other_node.port_of_incoming(edge_id)
                    } else {
                        other_node.port_of_outgoing(edge_id)
                    })
                    .as_pair();

                    *bend_points = VerticalBendDummy(xy);
                }
            };

            process_non_gateway(&node.incoming);
            process_non_gateway(&node.outgoing);
        }
        // TODO do we need a version for regular nodes as well? For their bend points I think yes?
        // Need to check what was broken here.

        let node = &mut n!(node_id);
        node.incoming_ports = incoming_ports;
        node.outgoing_ports = outgoing_ports;
    }
}

use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::EdgeType;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::graph::NodeId;
use crate::common::node::RelativePort;
use proc_macros::e;
use proc_macros::n;

pub fn postprocess_ports_and_vertical_edges(graph: &mut Graph) {
    fixup_ports_and_bendpoints(graph);
}

/// Gateway ports were assigned {x: node_width/2, y: node_height/2} since it is unclear at what
/// position the associated bend dummy will be, relative to the gateway node. But this is now known
/// after assigning specific y coordinates to all nodes. If the bend dummy if above/below the
/// gateway node, the port should be in the middle of the top/bottom border. If the bend dummy is
/// "within" the gateway, then the port should be at the left/right side.
/// This logic is actually applicable to all the vertical edges towards bend dummies (and corner
/// dummies which should be merged into the former type at some point). A perfectly vertical S bisect
/// edge (both sides `is_vertical`) will be fully routed within the `find_straight_edges` phase.
fn fixup_ports_and_bendpoints(graph: &mut Graph) {
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
        let mut process =
            |in_or_out: &[EdgeId], in_or_out_ports: &mut [RelativePort], relative_x: usize| {
                for (edge_id, relative_port) in
                    in_or_out.iter().cloned().zip(in_or_out_ports.iter_mut())
                {
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
                    assert!(
                        other_node.is_bend_dummy()
                            || other_node.is_back_edge_corner_dummy()
                            || other_node.is_snake_edge_bisect_dummy()
                    );

                    if other_node.y < top_border_y {
                        // above
                        relative_port.y = 0;

                        *bend_points = VerticalBendDummy(xy);
                    } else if other_node.y <= bottom_border_y {
                        // within
                        // This is only relevant for gateway bendpoints that can be put at the
                        // same height as the gateway, i.e. shall be vertically collapsed. The ILP
                        // construction with big M should have forced it to y/2 += rounding
                        // errors. So due to the rounding errors we, just in case, assign that other
                        // y here.
                        dbg!(&node, &relative_port, &xy, relative_x);
                        relative_port.y = xy.1;
                        relative_port.x = relative_x;
                        *bend_points = VerticalCollapsed;
                    } else {
                        // below
                        relative_port.y = node.height;
                        *bend_points = VerticalBendDummy(xy);
                    }
                }
            };
        process(&node.incoming, &mut incoming_ports, left_side_x);
        process(&node.outgoing, &mut outgoing_ports, right_side_x);
        let node = &mut n!(node_id);
        node.incoming_ports = incoming_ports;
        node.outgoing_ports = outgoing_ports;
    }
}

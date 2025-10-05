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
pub fn fixup_gateway_ports(graph: &mut Graph) {
    for node_id in (0..graph.nodes.len()).map(NodeId) {
        let node = &mut n!(node_id);
        let mut incoming_ports = std::mem::take(&mut node.incoming_ports);
        let mut outgoing_ports = std::mem::take(&mut node.outgoing_ports);
        let node = &n!(node_id);
        if !node.is_gateway() {
            continue;
        }

        let top_border_y = node.y;
        let bottom_border_y = node.y + node.height;
        let left_side_x = 0;
        let right_side_x = node.width;
        let process = |incoming_ports: &mut [RelativePort], relative_x: usize| {
            for (edge_id, relative_port) in
                node.incoming.iter().cloned().zip(incoming_ports.iter_mut())
            {
                let edge = &e!(edge_id);
                if !edge.is_vertical {
                    continue;
                }
                let other_node = &n!(if edge.from == node.id {
                    edge.to
                } else {
                    edge.from
                });
                if other_node.y < top_border_y {
                    // above
                    relative_port.y = top_border_y;
                } else if other_node.y <= bottom_border_y {
                    // within
                    relative_port.y = other_node.y;
                    relative_port.x = relative_x;
                } else {
                    // below
                    relative_port.y = bottom_border_y;
                }
            }
        };
        process(&mut incoming_ports, left_side_x);
        process(&mut outgoing_ports, right_side_x);
        let node = &mut n!(node_id);
        node.incoming_ports = incoming_ports;
        node.outgoing_ports = outgoing_ports;
    }
}

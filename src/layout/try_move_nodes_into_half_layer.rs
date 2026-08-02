use crate::common::edge::FlowType;
use crate::common::graph::{DATAOBJECT_NODE_HEIGHT, EdgeId, Graph, NodeId};
use crate::common::index_iter::IterIndices;
use crate::common::node::{LayerId, Node};
use proc_macros::{from, n, to};
use std::collections::HashMap;
use std::ops::RangeInclusive;

// If the data object is within a half-layer, and is on a vertical edge segment,
// then it is moved instead into the regular layer (left to it). Right now the
// y-ILP treats the data objects as if they were within the regular layer (even if they stay in
// their half layer), so when they are moved, there is enough room between tasks.
pub fn try_move_nodes_into_half_layer(graph: &mut Graph) {
    // First build a series of vertical edge segments, and then check against that.

    // TODO we only need to iterate through the layers where there actually are vertical segments.
    // But probably this is not a performance problem at all.

    // These are not *exactly* the vertical segments, but added padding.
    // Also, the left value is always smaller than the right value.
    let mut vertical_segments_per_layer: HashMap<LayerId, Vec<RangeInclusive<usize>>> =
        HashMap::new();

    let white_spacing = 5; // air between the elements
    let data_node_height = DATAOBJECT_NODE_HEIGHT;
    // XXX Don't reserve additional space for the label, as that can simply be moved around to a
    // vacant location.
    let top_padding = data_node_height + white_spacing;
    let bottom_padding = white_spacing;

    // Go through the nodes on regular layers.
    // `left` part of the edges we inspect right now.
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        if edge.is_replaced_by_dummies() {
            continue;
        }
        let edge_id = EdgeId(edge_idx);
        let (left_node, right_node) = (&n!(edge.from), &n!(edge.to));
        match edge.flow_type {
            FlowType::DataFlow(..) => {
                // TODO edges of data nodes are currently ignored, meaning that we accept that data nodes
                // are placed in a halflayer even if there is a data edge crossing it. Thing is, there is
                // some complication here: data edges might be drawn straight altogether, in which case
                // the current crossing might disappear. So ignore it for now. In the future one should
                // record whether the *only* vertical edge segment conflicts come from data edges and then
                // check whether they have been replaced with straight lines. So it's a bit complicated
                // and needs experiments and further thinking. Right now it is ignored, as it should
                // (hopefully!) be an edge case anyway.
                continue;
            }
            FlowType::MessageFlow(..) => {
                let pool = &graph.pools[left_node.pool];
                // The midpoint is enforced by edge routing as well. It is an invariant of this tool
                // that message flows go horizontal (if necessary) in the very first inter-pool space,
                // i.e. after leaving the starting pool.
                let midpoint = if left_node.pool_and_lane() < right_node.pool_and_lane() {
                    pool.y + pool.height
                } else {
                    pool.y
                };
                'first: {
                    let port_of_outgoing = left_node.port_of_outgoing(edge_id);
                    let layer_id = if left_node.is_any_dummy() {
                        // This is a bend dummy, where the flow leaves the node at the top or
                        // bottom, but then must route around another node obstacle. So it will go
                        // into the next layer.
                        left_node.layer_id
                    } else {
                        if !left_node.port_is_left_or_right(port_of_outgoing.y) {
                            break 'first;
                        }
                        if port_of_outgoing.x == left_node.x {
                            if left_node.layer_id.0 == 0 {
                                // This is the left-most layer. We don't move data elements into the -0.5
                                // half layer (well, maybe we actually should? not sure how often this situation
                                // occurs that there is a data object coming in from nowhere at the start).
                                break 'first;
                            }
                            LayerId(left_node.layer_id.0.strict_sub(1))
                        } else {
                            left_node.layer_id
                        }
                    };
                    let vertical_segments =
                        vertical_segments_per_layer.entry(layer_id).or_default();
                    vertical_segments.push(
                        left_node.y.min(midpoint).saturating_sub(top_padding)
                            ..=(left_node.y + left_node.height)
                                .max(midpoint)
                                .saturating_add(bottom_padding),
                    );
                }
                'last: {
                    let port_of_incoming = right_node.port_of_incoming(edge_id);
                    let layer_id = if right_node.is_any_dummy() {
                        // This is a bend dummy, where the flow enters the node at the top or
                        // bottom, but to do so must route around another node obstacle. So it will
                        // come in from the left layer.
                        if right_node.layer_id.0 == 0 {
                            // This is the left-most layer. We don't move data elements into the -0.5
                            // half layer (well, maybe we actually should? not sure how often this situation
                            // occurs that there is a data object coming in from nowhere at the start).
                            break 'last;
                        }
                        LayerId(right_node.layer_id.0.strict_sub(1))
                    } else {
                        if !right_node.port_is_left_or_right(port_of_incoming.y) {
                            break 'last;
                        }
                        if port_of_incoming.x == right_node.x {
                            if right_node.layer_id.0 == 0 {
                                // This is the left-most layer. We don't move data elements into the -0.5
                                // half layer (well, maybe we actually should? not sure how often this situation
                                // occurs that there is a data object coming in from nowhere at the start).
                                break 'last;
                            }
                            LayerId(right_node.layer_id.0.strict_sub(1))
                        } else {
                            right_node.layer_id
                        }
                    };
                    let vertical_segments =
                        vertical_segments_per_layer.entry(layer_id).or_default();
                    vertical_segments.push(
                        right_node.y.min(midpoint).saturating_sub(top_padding)
                            ..=(right_node.y + right_node.height)
                                .max(midpoint)
                                .saturating_add(bottom_padding),
                    );
                }
                // Segments are added, the rest of the logic is for the case where we have a sequence flow.
            }
            FlowType::SequenceFlow => {
                if edge.is_vertical {
                    // This does not block the half-layer space.
                    continue;
                }
                // Approximately correct `y` value. It just needs to be *some* `y` value across the sides of
                // the node. Could also use the port.
                let start_top = left_node.y;
                let start_bottom = left_node.y + left_node.height;
                let vertical_segments = vertical_segments_per_layer
                    .entry(left_node.layer_id)
                    .or_default();
                let end_top = right_node.y;
                let end_bottom = right_node.y + right_node.height;
                vertical_segments.push(
                    start_top.min(end_top).saturating_sub(top_padding)
                        ..=start_bottom.max(end_bottom).saturating_add(bottom_padding),
                );
            }
        }
    }

    'outer: for node_id in graph.nodes.len().iter_indices(false).map(NodeId) {
        let node = &n!(node_id);
        if !node.uses_half_layer {
            continue;
        }

        // No other elements were able to be moved into half layers, not sure what logic is
        // appropriate for them.
        assert!(node.is_data());
        // #1: Check whether there is no conflict in the edge layer.
        let vertical_segments: &Vec<_> = vertical_segments_per_layer
            .entry(node.layer_id)
            .or_default();

        for vertical_segment in vertical_segments.iter() {
            if vertical_segment.contains(&node.y) {
                n!(node_id).uses_half_layer = false;
                continue 'outer;
            }
        }

        // #2: Check whether the next-level edge is either (1) a real node, or it is (2) a long edge
        // dummy and is at the same height as our node.
        let Some((next_layer_node, port)) = node
            .outgoing
            .iter()
            .map(|e| &to!(*e))
            .zip(node.outgoing_ports.iter())
            .chain(
                node.incoming
                    .iter()
                    .map(|e| &from!(*e))
                    .zip(node.incoming_ports.iter()),
            )
            .find(|(n, _)| n.layer_id.0 == node.layer_id.0 + 1)
        else {
            continue;
        };

        if next_layer_node.is_any_dummy() && next_layer_node.y != (port + node.xy()).y {
            // There is a y difference between the data node and the next layer's dummy node. This
            // means we cannot move the data node into the half layer, as edge routing would be too
            // complex atm.
            continue;
        }

        if let [incoming] = &node.incoming[..]
            && let [outgoing] = &node.outgoing[..]
            && (data_element_sandwiched(&from!(*incoming), node, &to!(*outgoing))
                || data_element_sandwiched(&from!(*incoming), node, &to!(*outgoing)))
        {
            // The data element is directly sandwhiched between its two connected nodes. This means
            // that we should position it not at the mathematical center of the half layer, but
            // move it to the visual center of the gap between the two nodes. One might be an event,
            // the other an activity, then the center of the half layer would look weird.
            let from = from!(*incoming).dimension();
            let to = to!(*outgoing).dimension();
            let node_width = node.width;
            if from.x < to.x {
                n!(node_id).x = (from.x + from.width).midpoint(to.x) - node_width / 2;
            } else {
                n!(node_id).x = (to.x + to.width).midpoint(from.x) - node_width / 2;
            }
        } else {
            n!(node_id).x += graph.config.layer_width() / 2;
        }
    }
}

fn data_element_sandwiched(left: &Node, data_node: &Node, right: &Node) -> bool {
    left.pool_and_lane() == data_node.pool_and_lane()
        && left.layer_id == data_node.layer_id
        && right.pool_and_lane() == data_node.pool_and_lane()
        && data_node.layer_id.0 + 1 == right.layer_id.0
}

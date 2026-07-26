use crate::common::graph::{DATAOBJECT_NODE_HEIGHT, Graph};
use crate::common::node::LayerId;
use proc_macros::to;
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
    for left_node in &graph.nodes {
        if left_node.uses_half_layer {
            continue;
        }

        // TODO edges of data nodes are currently ignored, meaning that we accept that data nodes
        // are placed in a halflayer even if there is a data edge crossing it. Thing is, there is
        // some complication here: data edges might be drawn straight altogether, in which case
        // the current crossing might disappear. So ignore it for now. In the future one should
        // record whether the *only* vertical edge segment conflicts come from data edges and then
        // check whether they have been replaced with straight lines. So it's a bit complicated
        // and needs experiments and further thinking. Right now it is ignored, as it should
        // (hopefully!) be an edge case anyway.
        if left_node.is_data() {
            continue;
        }

        // Approximately correct `y` value. It just needs to be *some* `y` value across the sides of
        // the node. Could also use the port.
        let start_top = left_node.y;
        let start_bottom = left_node.y + left_node.height;
        let vertical_segments: &mut Vec<_> = vertical_segments_per_layer
            .entry(left_node.layer_id)
            .or_default();
        for outgoing_edge_id in &left_node.outgoing {
            let to_node = &to!(*outgoing_edge_id);

            // Similar to the `left_node.is_data()` check. In the case where to_node is
            // actually just in the next half-layer, then we need to not record this edge here,
            // as otherwise the incoming data edge blocks the to_node (a data node) from
            // being put into the half-layer.
            if to_node.is_data() && to_node.layer_id == left_node.layer_id {
                continue;
            }

            let end_top = to_node.y;
            let end_bottom = to_node.y + to_node.height;
            vertical_segments.push(
                start_top.min(end_top).saturating_sub(top_padding)
                    ..=start_bottom.max(end_bottom).saturating_add(bottom_padding),
            );
        }
    }

    'outer: for node in &mut graph.nodes {
        if !node.uses_half_layer {
            continue;
        }

        let vertical_segments: &Vec<_> = vertical_segments_per_layer
            .entry(node.layer_id)
            .or_default();

        for vertical_segment in vertical_segments.iter() {
            if vertical_segment.contains(&node.y) {
                node.uses_half_layer = false;
                continue 'outer;
            }
        }

        // No conflict, so shift it right.
        node.x += graph.config.layer_width() / 2;
    }
}

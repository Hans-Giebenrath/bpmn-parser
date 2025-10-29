//! Edge routing is the process of specifying the x coordinates of bend points.
//!    ┌-->
//!    |
//! ---┘
//!
//!    ^ assigning this x coordinate is the hard part. The y
//!
//! These segments are grouped, left to right:
//! - Loop edges (coming from the left, going to the left)
//! - Downward-pointing message flows
//! - Upward-pointing edges
//! - X crossings (where it is like a line swap with same y coordinates)
//! - .. and then the mirror cases
//! -
//! TODO would be better if all those edges where indeed forward edges, i.e.
//! backwards edges are not mixed with forward edges (reversed with non-reversed). Can this be
//! guaranteed through the ILP?
//!
//! For now, data edges and regular edges are treated equally, but this may change when
//! problems are encountered.

use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::{EdgeType, RegularEdgeBendPoints};
use crate::common::graph::{EdgeId, Graph};
use crate::common::node::AbsolutePort;
use itertools::Itertools;

#[derive(Debug)]
struct VerticalSegment {
    id: EdgeId,
    start_y: usize,
    end_y: usize,
}

/// TODO Maybe the code becomes easier if `idx` is inlined into `VerticalSegment`.
/// This denormalizes the type as hyperedges contain the value rather duplicated,
/// but maubbe it makes code a bit more maintainable actually.
#[derive(Debug)]
struct SegmentLayer {
    idx: usize,
    /// For ixi situations. `idx` is left, `idx2` is right.
    /// Not using an enum here (`enum { Vertical(usize), Diagonal(usize, usize) }`) since this makes
    /// usage rather tiresome.
    idx2: Option<usize>,
}

#[derive(Default, Debug)]
struct SegmentsWithYOverlap {
    left_loops: Vec<RegularEdge>,
    up_edges: Vec<RegularEdge>,
    ixi_crossing: Vec<(RegularEdge, RegularEdge)>,
    down_edges: Vec<RegularEdge>,
    right_loops: Vec<RegularEdge>,

    total_count_of_segmen_layers: usize,
}

#[derive(Debug)]
struct RegularEdge {
    segment: VerticalSegment,
    layer: SegmentLayer,
}

impl RegularEdge {
    fn min_y(&self) -> usize {
        self.segment.start_y.min(self.segment.end_y)
    }

    fn max_y(&self) -> usize {
        self.segment.start_y.max(self.segment.end_y)
    }
}

pub fn edge_routing(graph: &mut Graph) {
    // Outer Vec: Per Layer across all lanes and pool,
    // Inner Vec: All the vertical edge segments within that layer.
    let mut layered_edges = get_layered_edges(graph);

    let mut buffer = Vec::new();
    layered_edges
        .iter_mut()
        .flatten()
        .for_each(|es| determine_segment_layers(es, &mut buffer));

    add_bend_points(graph, &layered_edges);
}

fn determine_segment_layers(
    routing_edges: &mut SegmentsWithYOverlap,
    max_y_per_layer_buffer: &mut Vec</* max_y */ usize>,
) {
    // At this point the edges in each Vec are sorted by (min_y, max_y).
    // Now we need to take this information and make it into a layer assignment,
    // i.e. set values into the SegmentLayer variables.
    routing_edges.total_count_of_segmen_layers = 0;
    routing_edges.total_count_of_segmen_layers += determine_segment_layers_left_or_right_loops(
        &mut routing_edges.left_loops,
        max_y_per_layer_buffer,
        routing_edges.total_count_of_segmen_layers,
        false,
    );
    routing_edges.total_count_of_segmen_layers += determine_segment_layers_up_or_down_edges(
        &mut routing_edges.up_edges,
        max_y_per_layer_buffer,
        routing_edges.total_count_of_segmen_layers,
        false,
    );
    routing_edges.total_count_of_segmen_layers += determine_segment_layers_ixi(
        &mut routing_edges.ixi_crossing,
        max_y_per_layer_buffer,
        routing_edges.total_count_of_segmen_layers,
    );
    routing_edges.total_count_of_segmen_layers += determine_segment_layers_up_or_down_edges(
        &mut routing_edges.down_edges,
        max_y_per_layer_buffer,
        routing_edges.total_count_of_segmen_layers,
        true,
    );
    routing_edges.total_count_of_segmen_layers += determine_segment_layers_left_or_right_loops(
        &mut routing_edges.right_loops,
        max_y_per_layer_buffer,
        routing_edges.total_count_of_segmen_layers,
        true,
    );
}

fn determine_segment_layers_left_or_right_loops(
    routing_edges: &mut [RegularEdge],
    max_y_per_layer_buffer: &mut Vec</* max_y */ usize>,
    base_segment_layer: usize,
    reverse: bool,
) -> usize {
    if routing_edges.is_empty() {
        return 0;
    }
    max_y_per_layer_buffer.clear();
    todo!();
    let total_count_of_segmen_layers = max_y_per_layer_buffer.len();
    // TODO do the fancy stuff as mentioned above. For now this will be a lot more primitive.
    if reverse {
        // 0 or 2 -> 2 of 2
        // 1 or 2 -> 1 of 2
        // 2 or 2 -> 0 of 2
        routing_edges
            .iter_mut()
            .for_each(|e| e.layer.idx = total_count_of_segmen_layers - e.layer.idx);
    }

    // Shift them all to the correct value.
    routing_edges
        .iter_mut()
        .for_each(|e| e.layer.idx += base_segment_layer);

    total_count_of_segmen_layers
}

fn determine_segment_layers_ixi(
    routing_edges: &mut [(RegularEdge, RegularEdge)],
    max_y_per_layer_buffer: &mut Vec</* max_y */ usize>,
    base_segment_layer: usize,
) -> usize {
    match &mut routing_edges[..] {
        [] => return 0,
        [(left, right)] => {
            left.layer.idx = base_segment_layer;
            left.layer.idx2 = Some(base_segment_layer + 1);
            right.layer.idx = base_segment_layer;
            right.layer.idx2 = Some(base_segment_layer + 1);
            return 2;
        }
        _ => (),
    }
    max_y_per_layer_buffer.clear();

    for (e, _) in routing_edges.iter_mut() {
        let min_y = e.min_y();
        let best_fit_layer_idx = max_y_per_layer_buffer
            .iter()
            .position(|previous_edge_max_y| *previous_edge_max_y < min_y);
        let max_y = e.max_y();
        let layer_idx = match best_fit_layer_idx {
            None => {
                // No good layer found, need to add a new one to the right.
                max_y_per_layer_buffer.push(max_y);
                max_y_per_layer_buffer.len() - 1
            }
            Some(layer_idx) => {
                max_y_per_layer_buffer[layer_idx] = max_y;
                layer_idx
            }
        };
        e.layer.idx = layer_idx;
    }

    // Now do some funny stuff: We store in the buffer how many vertical space is covered for each
    // layer. Then we sort them and use the lower byte to encode the original idx value assigned to
    // the nodes. After sorting, each index is then mapped to an index which alternates around a
    // middle point (like a mountain-shaped histogram) such that the ixies are layouted around a
    // middle. (Reusing bytes so we don't need to create an additional allocation.)
    // TODO heavy testing please

    // Count for each layer how many pixels of vertical edge segments there are.
    max_y_per_layer_buffer.iter_mut().for_each(|x| *x = 0);
    for (e, _) in routing_edges.iter() {
        max_y_per_layer_buffer[e.layer.idx] += e.max_y() - e.min_y();
    }

    const BITS: usize = 8;
    for (idx, max_y) in max_y_per_layer_buffer.iter_mut().enumerate() {
        assert!(idx < (1 << BITS)); // I am rather confident that this will never be surpassed. This
        // means that we need at least 1000 nodes which should be blocked in the beginning already.
        *max_y = (*max_y << BITS) | idx;
    }

    // Can be unstable since there are no duplicates anyway: the lower byte is unique.
    max_y_per_layer_buffer.sort_unstable();

    let mid = (max_y_per_layer_buffer.len() - 1) / 2;
    for (new_target_idx, &mut max_y) in std::iter::once(0isize)
        .chain((0..).flat_map(|i| [i as isize, -i as isize]))
        .map(|offset| mid.strict_add_signed(offset))
        .zip(&mut *max_y_per_layer_buffer)
    {
        let from_idx = max_y & ((1 << BITS) - 1);
        let new_idx = 2 * new_target_idx + base_segment_layer;
        let new_idx2 = new_idx + 1;
        for (left, right) in routing_edges.iter_mut() {
            if left.layer.idx == from_idx {
                // Can only set the `right` one here since `left` is still used as a needle.
                right.layer.idx = new_idx;
                right.layer.idx2 = Some(new_idx2);
            }
        }
    }

    for (left, right) in routing_edges.iter_mut() {
        left.layer.idx = right.layer.idx;
        left.layer.idx2 = right.layer.idx2;
    }

    max_y_per_layer_buffer.len() * 2
}

fn determine_segment_layers_up_or_down_edges(
    routing_edges: &mut [RegularEdge],
    max_y_per_layer_buffer: &mut Vec</* max_y */ usize>,
    base_segment_layer: usize,
    reverse: bool,
) -> usize {
    // At this point the edges in each Vec are sorted by (min_y, max_y).
    // Now we need to take this information and make it into a layer assignment,
    // i.e. set values into the SegmentLayer variables.

    max_y_per_layer_buffer.clear();
    for e in routing_edges.iter_mut() {
        let mut best_fit_layer_idx = None;
        let min_y = e.min_y();
        let max_y = e.max_y();
        for target_layer in (0..max_y_per_layer_buffer.len()).rev() {
            let previous_edge_max_y = max_y_per_layer_buffer[target_layer];
            assert!(previous_edge_max_y != min_y);
            assert!(previous_edge_max_y != max_y);
            if previous_edge_max_y < min_y {
                // There is room in the current `target_layer` for this vertical
                // segment.
                best_fit_layer_idx = Some(target_layer);
            } else if previous_edge_max_y < max_y {
                break;
            } else {
                assert!(previous_edge_max_y > max_y);
                // nothing to do. This layer is blocked, we go further left and
                // check if there is some more free space.
            }
        }
        let layer_idx = match best_fit_layer_idx.take() {
            None => {
                // No good layer found, need to add a new one to the right.
                max_y_per_layer_buffer.push(max_y);
                max_y_per_layer_buffer.len() - 1
            }
            Some(layer_idx) => {
                max_y_per_layer_buffer[layer_idx] = max_y;
                layer_idx
            }
        };
        e.layer.idx = layer_idx;
    }
    let total_count_of_segmen_layers = max_y_per_layer_buffer.len();
    // TODO do the fancy stuff as mentioned above. For now this will be a lot more primitive.
    if reverse {
        // 0 or 2 -> 2 of 2
        // 1 or 2 -> 1 of 2
        // 2 or 2 -> 0 of 2
        routing_edges
            .iter_mut()
            .for_each(|e| e.layer.idx = total_count_of_segmen_layers - e.layer.idx);
    }

    // Shift them all to the correct value.
    routing_edges
        .iter_mut()
        .for_each(|e| e.layer.idx += base_segment_layer);

    total_count_of_segmen_layers
}

struct PerLayer(Vec<SegmentsWithYOverlap>);

fn get_layered_edges(graph: &mut Graph) -> Vec<Vec<SegmentsWithYOverlap>> {
    let mut edge_layers = Vec::<Vec<RegularEdge>>::new();
    edge_layers.resize_with(graph.num_layers, Default::default);

    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        if edge.is_vertical {
            continue;
        }
        let edge_id = EdgeId(edge_idx);
        match edge.edge_type {
            EdgeType::Regular {
                bend_points: RegularEdgeBendPoints::FullyRouted(_),
                ..
            }
            | EdgeType::ReplacedByDummies { .. } => continue,
            _ => (),
        }
        let from_idx = edge.from.0;
        let to_idx = edge.to.0;
        let from_node = &graph.nodes[from_idx];
        let to_node = &graph.nodes[to_idx];
        // TODO in some variant this assert needs to come back, but it should only consider
        // SequenceFlows. Maybe this is actually a graph invariant and does not belong here?
        /*assert!(
            outgoing_len == 1 || incoming_len == 1,
            "Combining branches directly with joins has not been implemented, yet.\nEdge: {edge:?}\nFrom: {from_node:?}\nTo: {to_node:?}"
        );*/
        let Some(AbsolutePort { y: start_y, .. }) = from_node.port_of_outgoing(edge_id) else {
            eprintln!("WARNING an edge pointed to a node but the node did not know it");
            continue;
        };
        let Some(AbsolutePort { y: end_y, .. }) = to_node.port_of_incoming(edge_id) else {
            eprintln!("WARNING an edge pointed to a node but the node did not know it");
            continue;
        };
        if start_y == end_y {
            continue;
        }
        let segment_vec = &mut edge_layers[from_node.layer_id.0];
        segment_vec.push(RegularEdge {
            segment: VerticalSegment {
                id: edge_id,
                start_y,
                end_y,
            },
            layer: SegmentLayer { idx: 0, idx2: None },
        });
    }

    let mut result = vec![];
    for _ in 0..edge_layers.len() {
        result.push(vec![]);
    }
    for (edge_layer_idx, mut edge_layer) in edge_layers.into_iter().enumerate() {
        // sort by min_y: To identify groups
        // sort by max_y: To later be able to easily spot ixi crossings from the up_edges and
        // down_edges vectors.
        edge_layer.sort_unstable_by_key(|e| (e.min_y(), e.max_y()));
        let Some(mut max_y) = edge_layer.first().map(|re| re.max_y()) else {
            continue;
        };
        // Chunk as long as edges are overlapping.
        let result = &mut result[edge_layer_idx];
        for (_key, chunk) in &edge_layer.into_iter().chunk_by(move |re| {
            let m = max_y;
            std::mem::replace(&mut max_y, re.max_y().max(m)) >= re.min_y()
        }) {
            //let a = chunk.collect::<Vec<_>>();
            let mut segments = SegmentsWithYOverlap::default();
            chunk.into_iter().for_each(|edge| {
                // TODO for self loops, check if edges[_id].from or .to is a special loop helper node,
                // in which case this should become a right_loops or left_loops member,
                // respectively.
                match edge.segment.start_y.cmp(&edge.segment.end_y) {
                    std::cmp::Ordering::Less => segments.down_edges.push(edge),
                    std::cmp::Ordering::Greater => segments.up_edges.push(edge),
                    std::cmp::Ordering::Equal => {
                        unreachable!("start_y == end_y has been checked earlier.")
                    }
                }
            });
            let mut i = 0;
            let mut j = 0;
            while i < segments.up_edges.len() && j < segments.down_edges.len() {
                let up = (
                    segments.up_edges[i].segment.start_y,
                    segments.up_edges[i].segment.end_y,
                );
                let down = (
                    segments.down_edges[j].segment.end_y,
                    segments.down_edges[j].segment.start_y,
                );
                if up == down {
                    segments
                        .ixi_crossing
                        .push((segments.up_edges.remove(i), segments.down_edges.remove(j)));
                    // No need to alter i or j, as we removed the elements at that location, and
                    // need to check the new elements in the next iteration.
                } else if up < down {
                    i += 1;
                } else {
                    j += 1;
                }
            }
            segments.down_edges.reverse();
            result.push(segments);
        }
    }
    result
}

fn add_bend_points(graph: &mut Graph, groups: &[Vec<SegmentsWithYOverlap>]) {
    let x_of_first_nodes_layer = graph.config.pool_header_width + graph.config.lane_x_padding;
    groups
        .iter()
        .enumerate()
        .for_each(|(nodes_layer_idx, one_layer_full_of_segments)| {
            add_bend_points_one_layer(
                graph,
                one_layer_full_of_segments,
                x_of_first_nodes_layer + graph.config.layer_width() * (nodes_layer_idx + 1),
            )
        });
}

fn add_bend_points_one_layer(
    graph: &mut Graph,
    groups: &[SegmentsWithYOverlap],
    x_of_center_segment_layer: usize,
) {
    for group in groups {
        let segment_num_layers = group.total_count_of_segmen_layers;
        if segment_num_layers == 0 {
            continue;
        }
        let segment_layer_width = if segment_num_layers == 1 {
            0
        } else {
            (graph.config.space_between_layers_for_segments() / (segment_num_layers - 1))
                .clamp(2, graph.config.max_space_between_vertical_edge_segments)
        };
        let min_x = (x_of_center_segment_layer) // + (graph.config.space_between_layers_for_segments() / 2))
            .saturating_sub(segment_layer_width * (segment_num_layers / 2));

        add_bend_points_one_segment_group(graph, group, segment_layer_width, min_x);
    }
}

fn add_bend_points_one_segment_group(
    graph: &mut Graph,
    segment: &SegmentsWithYOverlap,
    segment_layer_width: usize,
    min_x: usize,
) {
    segment.left_loops.iter().for_each(|e| {
        add_bend_points_one_segment(graph, &e.segment, &e.layer, segment_layer_width, min_x)
    });
    segment.up_edges.iter().for_each(|e| {
        add_bend_points_one_segment(graph, &e.segment, &e.layer, segment_layer_width, min_x)
    });
    segment.ixi_crossing.iter().for_each(|(e1, e2)| {
        add_bend_points_one_segment(graph, &e1.segment, &e1.layer, segment_layer_width, min_x);
        add_bend_points_one_segment(graph, &e2.segment, &e2.layer, segment_layer_width, min_x);
    });
    segment.down_edges.iter().for_each(|e| {
        add_bend_points_one_segment(graph, &e.segment, &e.layer, segment_layer_width, min_x)
    });
    segment.right_loops.iter().for_each(|e| {
        add_bend_points_one_segment(graph, &e.segment, &e.layer, segment_layer_width, min_x)
    });
}

fn add_bend_points_one_segment(
    graph: &mut Graph,
    segment: &VerticalSegment,
    SegmentLayer { idx, idx2 }: &SegmentLayer,
    segment_layer_width: usize,
    min_x: usize,
) {
    let x = min_x + idx * segment_layer_width;
    let x2 = min_x + idx2.unwrap_or(*idx) * segment_layer_width;
    match &mut graph.edges[segment.id.0].edge_type {
        EdgeType::Regular { bend_points, .. } => {
            *bend_points =
                RegularEdgeBendPoints::SegmentEndpoints((x, segment.start_y), (x2, segment.end_y))
        }
        EdgeType::DummyEdge { bend_points, .. } => {
            *bend_points =
                DummyEdgeBendPoints::SegmentEndpoints((x, segment.start_y), (x2, segment.end_y))
        }
        EdgeType::ReplacedByDummies { .. } => {
            unreachable!("This edge kind was excluded at the beginning.")
        }
    }
}

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

// TODO message flows need to be cut into pieces, do I do that?

use crate::common::config::{EdgeSegmentSpace, EdgeSegmentSpaceLocation};
use crate::common::graph::PoolId;
use crate::common::node::LayerId;
use proc_macros::e;
use proc_macros::from;
use proc_macros::n;
use proc_macros::to;
use std::collections::HashMap;

use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::{EdgeType, RegularEdgeBendPoints};
use crate::common::graph::{EdgeId, Graph};
use crate::common::node::AbsolutePort;
use itertools::Itertools;

/// Created for one `SegmentsWithYOverlap`. Is meant to calculate left-to-right paths of overlapping
/// segment intervals.
#[derive(Default, Debug)]
struct SegmentGraph<'a> {
    // Note: this could move into VerticalSegment as a RefCell, that should be more efficient than hash
    // lookups?
    graph: HashMap<EdgeId, Vec<&'a VerticalSegment>>,
    roots: Vec<&'a VerticalSegment>,
}

impl<'a> SegmentGraph<'a> {
    fn add_edge(
        &mut self,
        new_edge: &'a VerticalSegment,
        left: Option<EdgeId>,
        right: Option<&'a VerticalSegment>,
    ) {
        fn remove<'a>(needle: &'a VerticalSegment, haystack: &mut Vec<&'a VerticalSegment>) {
            haystack.retain(|connection| !std::ptr::eq(*connection, needle));
        }
        let right_connections = self.graph.entry(new_edge.id).or_default();
        if let Some(right) = right {
            right_connections.push(right);
            if let Some(left) = left
                && let Some(left_connections) = self.graph.get_mut(&left)
            {
                // Cut the connection between `left` and `right`, since `new_edge` has entered the
                // room between them.
                remove(right, left_connections);
            }
        }
        if left.is_none() {
            if let Some(right) = right {
                // `right` cannot be a root, since `new_edge` is left from it.
                remove(right, &mut self.roots);
            }
            self.roots.push(new_edge);
        }
    }

    fn all_paths_longest_first(self) -> Vec<Vec<&'a VerticalSegment>> {
        let mut all_paths = Vec::new();
        for root in &self.roots {
            self.recursion(root, Vec::new(), &mut all_paths);
        }
        all_paths.sort_unstable_by_key(|path: &Vec<&'a VerticalSegment>| -(path.len() as isize));
        all_paths
    }

    fn recursion(
        &self,
        current: &'a VerticalSegment,
        mut parent_path: Vec<&'a VerticalSegment>,
        all_paths: &mut Vec<Vec<&'a VerticalSegment>>,
    ) {
        parent_path.push(current);
        if let Some(children) = self.graph.get(&current.id)
            && !children.is_empty()
        {
            for child in children {
                self.recursion(child, parent_path.clone(), all_paths);
            }
        } else {
            all_paths.push(parent_path);
        }
    }
}

struct MessageFlowBendPoints {
    bend_points: Vec<(usize, usize)>,
    bends_after_pool: PoolId,
    interpool_bendpoint1_x: Option<usize>,
    interpool_bendpoint2_x: Option<usize>,
}

#[derive(Default)]
struct MessageFlowBendPointStore {
    store: HashMap<EdgeId, MessageFlowBendPoints>,
}

impl MessageFlowBendPointStore {
    fn register_edge(&mut self, edge_id: EdgeId, bends_after_pool: PoolId) {
        self.store.insert(
            edge_id,
            MessageFlowBendPoints {
                bend_points: Vec::new(),
                bends_after_pool,
                interpool_bendpoint1_x: None,
                interpool_bendpoint2_x: None,
            },
        );
    }

    fn finish_layer(
        &mut self,
        edge_id: EdgeId,
        // If the edge goes straight up here, then no bend point is added.
        bend_point: Option<(usize, usize)>,
        interpool_bendpoint_x: usize,
    ) {
        let state = self.store.get_mut(&edge_id).unwrap();
        if let Some(bend_point) = bend_point {
            state.bend_points.push(bend_point);
        }
        assert_eq!(state.interpool_bendpoint2_x, None);
        if state.interpool_bendpoint1_x.is_none() {
            state.interpool_bendpoint1_x = Some(interpool_bendpoint_x);
        } else {
            state.interpool_bendpoint2_x = Some(interpool_bendpoint_x);
        }
    }

    fn iter_edges_for_break_after_pool(
        &self,
        after_pool: PoolId,
    ) -> impl Iterator<Item = (EdgeId, usize, usize)> {
        self.store
            .iter()
            .filter(move |(_, mfbp)| mfbp.bends_after_pool == after_pool)
            .map(|(edge_id, mfbp)| {
                (
                    *edge_id,
                    mfbp.interpool_bendpoint1_x.unwrap(),
                    mfbp.interpool_bendpoint2_x.unwrap(),
                )
            })
    }

    fn completely_route_the_path(
        &mut self,
        graph: &mut Graph,
        edge_id: EdgeId,
        interpool_bendpoints: [(usize, usize); 2],
    ) {
        let (start, end, rev) = {
            let from_port = from!(edge_id).port_of_outgoing(edge_id);
            let to_port = to!(edge_id).port_of_incoming(edge_id);
            if from_port.y < to_port.y {
                (from_port, to_port, false)
            } else {
                (to_port, from_port, true)
            }
        };
        let mut bend_points = self.store.remove(&edge_id).unwrap().bend_points;
        bend_points.insert(0, start.as_pair());
        bend_points.extend_from_slice(&interpool_bendpoints);
        bend_points.push(end.as_pair());
        bend_points.sort_unstable();
        for i in 0..bend_points.len() - 2 {
            let (correct_reference, next, next_next) =
                (bend_points[i], bend_points[i + 1], bend_points[i + 2]);
            if correct_reference.1 == next.1 {
                continue;
            }
            if correct_reference.1 == next_next.1 {
                bend_points.swap(i + 1, i + 2);
                continue;
            }
            if correct_reference.0 == next.0 {
                continue;
            }
            if correct_reference.0 == next_next.0 {
                bend_points.swap(i + 1, i + 2);
                continue;
            }
            unreachable!();
        }
        if rev {
            bend_points.reverse();
        }
        let EdgeType::Regular {
            bend_points: out_bend_points,
            ..
        } = &mut e!(edge_id).edge_type
        else {
            unimplemented!("Message flows are always regular edges.");
        };
        *out_bend_points = RegularEdgeBendPoints::FullyRouted(bend_points);
    }
}

#[derive(Default, Debug)]
struct SegmentsWithYOverlap {
    left_loops: Vec<VerticalSegment>,
    up_message_flows: Vec<VerticalSegment>,
    up_edges: Vec<VerticalSegment>,
    ixi_crossing: Vec<(VerticalSegment, VerticalSegment)>,
    down_edges: Vec<VerticalSegment>,
    down_message_flows: Vec<VerticalSegment>,
    right_loops: Vec<VerticalSegment>,
}

// TODO left and right loops are not handled at all right now.
#[derive(Debug, Clone, PartialEq)]
enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
struct VerticalSegment {
    id: EdgeId,
    start_y: usize,
    end_y: usize,
    idx: usize,
    /// For ixi situations. `idx` is left, `idx2` is right.
    /// Not using an enum here (`enum { Vertical(usize), Diagonal(usize, usize) }`) since this makes
    /// usage rather tiresome.
    idx2: Option<usize>,

    alignment: Alignment,
    // Oioioi sorry for the spaghetti. I just don't want to handle all those usize indexes.
    // But I create paths from left to right and store `&VerticalSegment`s, and I still want to mark
    x_coordinate: std::cell::Cell<Option<usize>>,
}

impl VerticalSegment {
    fn min_y(&self) -> usize {
        self.start_y.min(self.end_y)
    }

    fn max_y(&self) -> usize {
        self.start_y.max(self.end_y)
    }
}

pub fn edge_routing(graph: &mut Graph) {
    // Outer Vec: Per Layer across all lanes and pool,
    // Inner Vec: All the vertical edge segments within that layer.
    let (mut layered_edges, mut mf_store) = get_layered_edges(graph);

    // TODO when loops are added, then handle loops in the front and at the end by taking the first
    // and last element of the vector away (slicing), and use the
    // EdgeSegmentSpaceLocation::LeftBorder/AfterLast variants.
    let mut buffer = Vec::new();
    layered_edges
        .iter_mut()
        .enumerate()
        .flat_map(|(layer_idx, vec)| vec.iter_mut().map(move |segs| (LayerId(layer_idx), segs)))
        .for_each(|(layer_id, segments)| {
            determine_segment_layers(
                graph,
                &mut mf_store,
                segments,
                &mut buffer,
                EdgeSegmentSpaceLocation::After(layer_id),
            )
        });
}

fn determine_segment_layers(
    graph: &mut Graph,
    mf_store: &mut MessageFlowBendPointStore,
    routing_edges: &mut SegmentsWithYOverlap,
    max_y_per_layer_buffer: &mut Vec</* max_y */ usize>,
    location: EdgeSegmentSpaceLocation,
) {
    // At this point the edges in each Vec are sorted by (min_y, max_y).
    // Now we need to take this information and make it into a rough layer assignment,
    // i.e. set idx values into the SegmentLayer variables. This is required to understand what
    // is the left-to-right order of all edge segments.
    let mut total_count_of_segment_layers = 0;
    total_count_of_segment_layers += determine_segment_layers_left_or_right_loops(
        &mut routing_edges.left_loops,
        max_y_per_layer_buffer,
        total_count_of_segment_layers,
        false,
    );
    total_count_of_segment_layers += determine_segment_layers_up_or_down_edges(
        &mut routing_edges.up_edges,
        max_y_per_layer_buffer,
        total_count_of_segment_layers,
        false,
    );
    total_count_of_segment_layers += determine_segment_layers_ixi(
        &mut routing_edges.ixi_crossing,
        max_y_per_layer_buffer,
        total_count_of_segment_layers,
    );
    total_count_of_segment_layers += determine_segment_layers_up_or_down_edges(
        &mut routing_edges.down_edges,
        max_y_per_layer_buffer,
        total_count_of_segment_layers,
        true,
    );
    total_count_of_segment_layers += determine_segment_layers_left_or_right_loops(
        &mut routing_edges.right_loops,
        max_y_per_layer_buffer,
        total_count_of_segment_layers,
        true,
    );

    // Silence clippy.
    let _ = total_count_of_segment_layers;

    let mut segment_graph = SegmentGraph::default();
    let mut currently_active = Vec::<&VerticalSegment>::new();
    let (ixi_above, ixi_below): (Vec<_>, Vec<_>) =
        routing_edges.ixi_crossing.iter().cloned().unzip();

    itertools::kmerge_by(
        [
            routing_edges.left_loops.iter(),
            routing_edges.up_edges.iter(),
            ixi_above.iter(),
            ixi_below.iter(),
            routing_edges.down_edges.iter(),
            routing_edges.right_loops.iter(),
        ]
        .iter_mut(),
        |left: &&VerticalSegment, right: &&VerticalSegment| left.start_y < right.start_y,
    )
    .for_each(|new_edge| {
        currently_active.retain(|old_edge| old_edge.end_y > new_edge.start_y);
        currently_active.push(new_edge);
        currently_active.sort_unstable_by_key(|edge| edge.idx);
        let idx = currently_active
            .iter()
            .position(|edge| edge.id == new_edge.id)
            .unwrap();
        segment_graph.add_edge(
            new_edge,
            idx.checked_sub(1)
                .and_then(|idx| currently_active.get(idx))
                .map(|edge| edge.id),
            currently_active.get(idx + 1).copied(),
        );
    });
    let all_paths = segment_graph.all_paths_longest_first();
    let EdgeSegmentSpace {
        start_x,
        end_x,
        center_x,
    } = graph.config.edge_segment_space(location);

    enum State {
        Init,
        RunOfLayoutedSegments { latest_start: usize },
        RunOfFreshSegments { start: usize, start_idx: usize },
    }
    for path in all_paths.iter() {
        let mut state = State::Init;
        for (idx, segment) in path.iter().enumerate() {
            state = match (state, segment.x_coordinate.get()) {
                (State::Init, Some(x)) => State::RunOfLayoutedSegments { latest_start: x },
                (State::Init, None) => State::RunOfFreshSegments {
                    start: start_x,
                    start_idx: 0,
                },
                (State::RunOfLayoutedSegments { .. }, Some(x)) => {
                    State::RunOfLayoutedSegments { latest_start: x }
                }
                (State::RunOfLayoutedSegments { latest_start }, None) => {
                    State::RunOfFreshSegments {
                        start: latest_start,
                        start_idx: idx,
                    }
                }
                (State::RunOfFreshSegments { start, start_idx }, Some(x)) => {
                    assign_the_real_x_values(
                        graph,
                        mf_store,
                        &path[start_idx..idx],
                        EdgeSegmentSpace {
                            start_x: start,
                            end_x: x,
                            center_x,
                        },
                    );
                    State::RunOfLayoutedSegments { latest_start: x }
                }
                (state @ State::RunOfFreshSegments { .. }, None) => state,
            }
        }
        // The last run must also be processed.
        if let State::RunOfFreshSegments { start, start_idx } = state {
            assign_the_real_x_values(
                graph,
                mf_store,
                &path[start_idx..],
                EdgeSegmentSpace {
                    start_x: start,
                    end_x,
                    center_x,
                },
            );
        }
    }
}

fn assign_the_real_x_values(
    graph: &mut Graph,
    mf_store: &mut MessageFlowBendPointStore,
    path: &[&VerticalSegment],
    space: EdgeSegmentSpace,
) {
    let segspace = graph.config.max_space_between_vertical_edge_segments;
    let horizontal_space = space.end_x.strict_sub(space.start_x);
    let count_of_comfortably_fitting_edge_segments = horizontal_space / segspace;
    if path.len() > count_of_comfortably_fitting_edge_segments {
        // Must squeeze them into the space.
        let segment_width = (horizontal_space as f64) / ((path.len() + 1) as f64);
        for (idx, edge) in path.iter().enumerate() {
            add_bend_points_one_segment(graph, mf_store, edge, idx, segment_width, space.start_x);
        }
        return;
    }
    let (left, center, right) = {
        let i = path.partition_point(|t| t.alignment == Alignment::Left);
        let k = path[i..].partition_point(|t| t.alignment == Alignment::Center);
        (&path[..i], &path[i..i + k], &path[i + k..])
    };
    for (idx, edge) in left.iter().enumerate() {
        add_bend_points_one_segment(graph, mf_store, edge, idx, segspace as f64, space.start_x);
    }
    let right_start = space.end_x.strict_sub((right.len() + 1) * segspace);
    for (idx, edge) in right.iter().enumerate() {
        add_bend_points_one_segment(graph, mf_store, edge, idx, segspace as f64, right_start);
    }
    let leftmost_start_x = space.start_x + (left.len() * segspace);
    let rightmost_end_x = space.end_x.strict_sub(right.len() * segspace);
    let required_width = (center.len() + 1) * segspace;
    let leftmost_center_x = leftmost_start_x + required_width / 2;
    let rightmost_center_x = rightmost_end_x.strict_sub(required_width / 2);
    assert!(leftmost_center_x < rightmost_center_x);
    let start_x = match () {
        _ if space.center_x < leftmost_center_x => leftmost_start_x,
        _ if space.center_x > rightmost_center_x => rightmost_center_x,
        _ => leftmost_start_x + space.center_x.strict_sub(leftmost_center_x),
    };
    for (idx, edge) in center.iter().enumerate() {
        add_bend_points_one_segment(graph, mf_store, edge, idx, segspace as f64, start_x);
    }
}

fn determine_segment_layers_left_or_right_loops(
    routing_edges: &mut [VerticalSegment],
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
            .for_each(|e| e.idx = total_count_of_segmen_layers - e.idx);
    }

    // Shift them all to the correct value.
    routing_edges
        .iter_mut()
        .for_each(|e| e.idx += base_segment_layer);

    total_count_of_segmen_layers
}

fn determine_segment_layers_ixi(
    routing_edges: &mut [(VerticalSegment, VerticalSegment)],
    max_y_per_layer_buffer: &mut Vec</* max_y */ usize>,
    base_segment_layer: usize,
) -> usize {
    match &mut routing_edges[..] {
        [] => return 0,
        [(left, right)] => {
            left.idx = base_segment_layer;
            left.idx2 = Some(base_segment_layer + 1);
            right.idx = base_segment_layer;
            right.idx2 = Some(base_segment_layer + 1);
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
        e.idx = layer_idx;
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
        max_y_per_layer_buffer[e.idx] += e.max_y() - e.min_y();
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
            if left.idx == from_idx {
                // Can only set the `right` one here since `left` is still used as a needle.
                right.idx = new_idx;
                right.idx2 = Some(new_idx2);
            }
        }
    }

    for (left, right) in routing_edges.iter_mut() {
        left.idx = right.idx;
        left.idx2 = right.idx2;
    }

    max_y_per_layer_buffer.len() * 2
}

fn determine_segment_layers_up_or_down_edges(
    routing_edges: &mut [VerticalSegment],
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
        e.idx = layer_idx;
    }
    let total_count_of_segmen_layers = max_y_per_layer_buffer.len();
    // TODO do the fancy stuff as mentioned above. For now this will be a lot more primitive.
    if reverse {
        // 0 or 2 -> 2 of 2
        // 1 or 2 -> 1 of 2
        // 2 or 2 -> 0 of 2
        routing_edges
            .iter_mut()
            .for_each(|e| e.idx = total_count_of_segmen_layers - e.idx);
    }

    // Shift them all to the correct value.
    routing_edges
        .iter_mut()
        .for_each(|e| e.idx += base_segment_layer);

    total_count_of_segmen_layers
}

fn get_layered_edges(
    graph: &mut Graph,
) -> (Vec<Vec<SegmentsWithYOverlap>>, MessageFlowBendPointStore) {
    let mut mf_store = MessageFlowBendPointStore::default();
    let mut edge_layers = Vec::<Vec<VerticalSegment>>::new();
    edge_layers.resize_with(graph.num_layers, Default::default);

    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        if edge.is_vertical {
            // Should already be caught in the straight_edge_routing phase.
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
        let AbsolutePort { y: start_y, .. } = from_node.port_of_outgoing(edge_id);
        let AbsolutePort { y: end_y, .. } = to_node.port_of_incoming(edge_id);
        if start_y == end_y {
            continue;
        }
        let segment_vec = &mut edge_layers[from_node.layer_id.0];
        segment_vec.push(VerticalSegment {
            id: edge_id,
            start_y,
            end_y,
            idx: 0,
            idx2: None,
            alignment: Alignment::Center,
            x_coordinate: Default::default(),
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
                match edge.start_y.cmp(&edge.end_y) {
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
                let up = (segments.up_edges[i].start_y, segments.up_edges[i].end_y);
                let down = (segments.down_edges[j].end_y, segments.down_edges[j].start_y);
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
    (result, mf_store)
}

fn add_bend_points_one_segment(
    graph: &mut Graph,
    mf_store: &mut MessageFlowBendPointStore,
    segment: &VerticalSegment,
    idx: usize,
    segment_layer_width: f64,
    min_x: usize,
) {
    let ixi_diagonalizer = if segment.idx2.is_some() { 1 } else { 0 };
    // idx in the caller starts at 0, but `min_x` is actually expecting one additional padding.
    // Sorry for the spaghetti.
    let x = min_x + ((idx + 1) as f64 * segment_layer_width) as usize;
    let x2 = min_x + ((idx + 1 + ixi_diagonalizer) as f64 * segment_layer_width) as usize;
    match (segment.idx, segment.idx2) {
        (a, Some(b)) if a > b => segment.x_coordinate.set(Some(x2)),
        _ => segment.x_coordinate.set(Some(x)),
    }
    let edge = &mut e!(segment.id);
    let is_message_flow = edge.is_message_flow();
    let (p1, p2) = ((x, segment.start_y), (x2, segment.end_y));
    match &mut edge.edge_type {
        EdgeType::Regular { bend_points, .. } if is_message_flow => {
            //TODO this must check which of the points p1 / p2 is the actual bendpoint, as the other one is the floaitng inter-pool point whose x coordinate is the `interpool_bendpoint_x`.
            mf_store.finish_layer(segment.id, (p1, p2));
        }
        EdgeType::Regular {
            bend_points: out_bend_points,
            ..
        } => {
            // BACKLOG: inlining graph.start_and_end_ports due to borrow checker.
            let from_xy = n!(edge.from).port_of_outgoing(segment.id).as_pair();
            let to_xy = n!(edge.to).port_of_incoming(segment.id).as_pair();
            let bend_points = if edge.is_reversed {
                vec![to_xy, p2, p1, from_xy]
            } else {
                vec![from_xy, p1, p2, to_xy]
            };
            *out_bend_points = RegularEdgeBendPoints::FullyRouted(bend_points)
        }
        EdgeType::DummyEdge { bend_points, .. } => {
            *bend_points = DummyEdgeBendPoints::SegmentEndpoints(p1, p2)
        }
        EdgeType::ReplacedByDummies { .. } => {
            unreachable!("This edge kind was excluded at the beginning.")
        }
    }
}

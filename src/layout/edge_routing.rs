//! Edge routing is the process of specifying the x coordinates of bend points.
//!    ┌-->
//!    |
//! ---┘
//!
//!    ^ assigning this x coordinate is the hard part.
//!
//! Long sequence flows are split into dummy edges, and each dummy edge is solved separately.
//! For message flows which have multiple turns we instead use a separate message flow bend point
//! store, and reuse the x-coordinate algorithm for finding the y-coordinate of the horizonal
//! segments in the inter-pool space. Sadly you must be a big pasta fan if you want to dive into
//! this huge entangled spaghetti.
//!
//! These segments are grouped, left to right:
//! - Loop edges (coming from the left, going to the left)
//! - Upward-pointing message flows
//! - Upward-pointing edges
//! - IXI crossings (where it is like a line swap with same y coordinates)
//! - Downward-pointing edges
//! - Downward-pointing message flows
//! - Loop edges (coming from the right, going to the right)
//!
//! TODO Right now the following are not handled:
//! - JXI crossings where it is not a perfect swap
//! - Staircase crossing where only the ends are exactly overlapping
//!
//! Both can be handled in the same sphere as the IXI crossings, and it should be easy to do so,
//! just there are more relevant topics I believe. But they have the interesting property that if
//! there are just JXI/Staircase crossings of the same kind (read the sentence to the end and think
//! about it, then you'll understand what I mean with this), then one could avoid them by swapping
//! the upwards and downwards facing edges in the above list. So there is a bit more to it still.
//!
//! For now, data edges and regular edges are treated equally, but this may change when
//! problems are encountered.
//!
//! BUGS:
//! * If a message flow goes through a layer with a data-object in a half layer, then that should
//!   not be in the half layer.

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

/// Created for one `SegmentsOfSameLayer`. Is meant to calculate left-to-right paths of overlapping
/// segment intervals.
#[derive(Default, Debug)]
struct SegmentGraph<'a> {
    // Note: this could move into VerticalSegment as a RefCell, that should be more efficient than hash
    // lookups?
    graph: HashMap<EdgeId, Vec<&'a VerticalSegment>>,
    /// A `root` is a left-most segment, i.e. where no other segment if left of it which overlaps
    /// it.
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
        if let Some(left) = left {
            self.graph
                .get_mut(&left)
                .expect("must be present")
                .push(new_edge);
        } else {
            if let Some(right) = right {
                // `right` cannot be a root, since `new_edge` is left from it.
                remove(right, &mut self.roots);
            }
            // Nothing is left, so this must be a new root (at least at the moment, maybe undone in
            // if some more-left edge is added later).
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

#[derive(Clone, Debug)]
enum MessageFlowBendState {
    // Starts in layer `k`, goes right, then up or down, then again right and ends in layer `k + 1`.
    HorVerHor,
    VerHorVer {
        bends_after_pool: PoolId,
        interpool_bendpoint1_x: usize,
        interpool_bendpoint2_x: usize,
    },
    HorVerHorVer {
        bends_after_pool: PoolId,
        bend_point_1: Option<(usize, usize)>,
        interpool_bendpoint2_x: usize,
    },
    VerHorVerHor {
        bends_after_pool: PoolId,
        interpool_bendpoint1_x: usize,
        bend_point_2: Option<(usize, usize)>,
    },
    HorVerHorVerHor {
        bends_after_pool: PoolId,
        bend_point_1: Option<(usize, usize)>,
        bend_point_2: Option<(usize, usize)>,
    },
}

struct StartEnd {
    start: usize,
    end: usize,
}

impl MessageFlowBendState {
    fn bends_after_pool(&self) -> Option<(PoolId, StartEnd)> {
        use MessageFlowBendState::*;
        match self.clone() {
            VerHorVer {
                bends_after_pool,
                interpool_bendpoint1_x: start,
                interpool_bendpoint2_x: end,
            }
            | HorVerHorVer {
                bends_after_pool,
                bend_point_1: Some((start, _)),
                interpool_bendpoint2_x: end,
            }
            | VerHorVerHor {
                bends_after_pool,
                interpool_bendpoint1_x: start,
                bend_point_2: Some((end, _)),
            }
            | HorVerHorVerHor {
                bends_after_pool,
                bend_point_1: Some((start, _)),
                bend_point_2: Some((end, _)),
            } => Some((bends_after_pool, StartEnd { start, end })),
            HorVerHor => None,
            // Invalid as this means that there is some `None`. But at this point there should not
            // be any `None` left over.
            invalid_state => unreachable!("{invalid_state:?}"),
        }
    }
}

/// When the edge routing routine is reused for inter-pool message flow routing, then those
/// horizontal segments need to be turned around into vertical segments. Hence this transpose.
/// There is the complication that in the upwards facing case the bend points need to be swapped
/// as they otherwise would not be recognized as up/down but down/up.
fn transpose(
    (x1, y1): (usize, usize),
    (x2, y2): (usize, usize),
    is_down: bool,
) -> ((usize, usize), (usize, usize)) {
    if is_down {
        //   │     ┌►      │      ─┐
        // ┌─┘ ->  │       └─┐ ->  │
        // ▼      ─┘         ▼     └►
        ((y1, x1), (y2, x2))
    } else {
        // Need to swap p1 and p2, otherwise the transpose would result in wrong direction arrows:
        //   ▲     ┌─      ▲      ◄┐
        // ┌─┘ ->  │       └─┐ ->  │
        // │      ◄┘         │     └─
        ((y2, x2), (y1, x1))
    }
}

//struct MessageFlowBendPoints {
//    /// The bend points right next to the port, not the ones in the inter-pool area.
//    bend_points: Vec<(usize, usize)>,
//    /// If `None` then this just goes from one layer to the next and does not bend in the interpool
//    /// area. Actually combining the following three would be better typing, TODO.
//    bends_after_pool: Option<PoolId>,
//    /// Only the `x` value, as `y` is calculated in a second edge routing phase.
//    interpool_bendpoint1_x: Option<usize>,
//    /// Only the `x` value, as `y` is calculated in a second edge routing phase.
//    interpool_bendpoint2_x: Option<usize>,
//}

#[derive(Default, Debug)]
struct MessageFlowBendPointStore {
    store: HashMap<EdgeId, MessageFlowBendState>,
}

impl MessageFlowBendPointStore {
    fn register_edge(&mut self, edge_id: EdgeId, state: MessageFlowBendState) {
        self.store.insert(edge_id, state);
    }

    fn finish_layer(
        &mut self,
        graph: &mut Graph,
        edge_id: EdgeId,
        // segment bend point 1
        p1: (usize, usize),
        // segment bend point 2
        p2: (usize, usize),
    ) {
        let state = self.store.get_mut(&edge_id).unwrap();
        let from = &from!(edge_id);
        let to = &to!(edge_id);
        let from_xy = from.port_of_outgoing(edge_id).as_pair();
        let to_xy = to.port_of_incoming(edge_id).as_pair();
        let is_down = from.pool < to.pool;
        let is_right = from.layer_id < to.layer_id;
        let mut bend_points;

        use MessageFlowBendState::*;
        match state {
            HorVerHor => {
                assert!(is_right);
                bend_points = vec![from_xy, p1, p2, to_xy];
            }
            VerHorVer { .. } => {
                let (p1, p2) = transpose(p1, p2, is_down);
                bend_points = vec![from_xy, p1, p2, to_xy];
            }
            HorVerHorVer { bend_point_1, .. } => {
                if let Some(bend_point_1) = bend_point_1 {
                    // State is full, so this was the MF inter-pool routing.
                    let (p1, p2) = transpose(p1, p2, is_down);
                    bend_points = vec![from_xy, *bend_point_1, p1, p2, to_xy];
                } else {
                    *bend_point_1 = Some(p1);
                    return;
                }
            }
            VerHorVerHor { bend_point_2, .. } => {
                if let Some(bend_point_2) = bend_point_2 {
                    // State is full, so this was the MF inter-pool routing.
                    let (p1, p2) = transpose(p1, p2, is_down);
                    bend_points = vec![from_xy, p1, p2, *bend_point_2, to_xy];
                } else {
                    *bend_point_2 = Some(p2);
                    return;
                }
            }
            HorVerHorVerHor {
                bend_point_1: Some(bend_point_1),
                bend_point_2: Some(bend_point_2),
                ..
            } => {
                // State is full, so this was the MF inter-pool routing.
                let (p1, p2) = transpose(p1, p2, is_down);
                bend_points = vec![from_xy, *bend_point_1, p1, p2, *bend_point_2, to_xy];
            }
            HorVerHorVerHor {
                bend_point_1: Some(_),
                bend_point_2: bend_point_2 @ None,
                ..
            } => {
                *bend_point_2 = Some(p2);
                return;
            }
            HorVerHorVerHor {
                bend_point_1: bend_point_1 @ None,
                bend_point_2: Some(_),
                ..
            } => {
                *bend_point_1 = Some(p1);
                return;
            }
            HorVerHorVerHor {
                bend_point_1: bend_point_1 @ None,
                bend_point_2: bend_point_2 @ None,
                ..
            } => {
                if is_right {
                    *bend_point_1 = Some(p1);
                } else {
                    // The layers are solved left to right. So since the MF goes left, then the
                    // later part of the edge is actually visited first, and so that bend points is
                    // calculated first.
                    *bend_point_2 = Some(p2);
                }
                return;
            }
        }

        // Now comes the funny part. What just got routed was the long part of the edge. It could be
        // that the edge did not start and/or end at the real nodes, but that there was a bend dummy
        // in between. So we need to find this out as well.
        {
            let from = &from!(edge_id);
            if from.is_bend_dummy() {
                let &[vertical_edge_id] = &from.incoming[..] else {
                    unreachable!("{from:?},\n{graph:?}");
                };
                let real_node_from = &from!(vertical_edge_id);
                bend_points.insert(
                    0,
                    real_node_from.port_of_outgoing(vertical_edge_id).as_pair(),
                );
            }
        }
        {
            let to = &to!(edge_id);
            if to.is_bend_dummy() {
                let &[vertical_edge_id] = &to.outgoing[..] else {
                    unreachable!("{to:?},\n{graph:?}");
                };
                let real_node_to = &to!(vertical_edge_id);
                bend_points.push(real_node_to.port_of_incoming(vertical_edge_id).as_pair());
            }
        }

        let edge = &mut e!(edge_id);
        // At this point we know that the edge is really fully routed, so we can stop here.
        use RegularEdgeBendPoints::FullyRouted;
        match &mut edge.edge_type {
            EdgeType::Regular {
                bend_points: out_bend_points,
                ..
            } => *out_bend_points = FullyRouted(bend_points),
            EdgeType::DummyEdge { original_edge, .. } => {
                let original_edge = *original_edge;

                let edge_type = &mut e!(original_edge).edge_type;
                let EdgeType::ReplacedByDummies { text, .. } = edge_type else {
                    unreachable!();
                };

                *edge_type = EdgeType::Regular {
                    text: text.take(),
                    bend_points: FullyRouted(bend_points),
                }
            }
            _ => unreachable!(),
        };
        // Ensure that if we finished and would access this value again, it will crash.
        self.store.remove(&edge_id);
    }

    fn iter_edges_for_break_after_pool(&self) -> impl Iterator<Item = (PoolId, EdgeId, StartEnd)> {
        self.store.iter().filter_map(|(edge_id, mfbp)| {
            mfbp.bends_after_pool()
                .map(|(pool, start_end)| (pool, *edge_id, start_end))
        })
    }
}

#[derive(Default, Debug)]
struct SegmentsOfSameLayer {
    left_loops: Vec<VerticalSegment>,
    // Keep `xx_message_flows` and `xx_edges` separate (although in principle one would be tempted
    // to merge them) because in the future message flows or edges could be swapped up/down ->
    // down/up to avoid JXI crossings (they can be avoided sometimes if there are no two
    // conflicting.)
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
    /// Rough index
    idx: usize,
    /// For ixi situations. `Some(0)` for the first one (with `idx`) and `Some(1)` for the second
    /// one (with `idx + 1`).
    /// Not using an enum here (`enum { Vertical(usize), Diagonal(usize, usize) }`) since this makes
    /// usage rather tiresome.
    ixi_diagonalizer: Option<usize>,

    is_message_flow: bool,
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
    let (layered_edges, mut mf_store) = get_layered_edges(graph);

    let mut buffer = Vec::new();
    let mut layered_edges_it = layered_edges.into_iter();
    if let Some(mut segments) = layered_edges_it.next() {
        determine_segment_layers(
            graph,
            &mut mf_store,
            &mut segments,
            &mut buffer,
            graph
                .config
                .edge_segment_space(EdgeSegmentSpaceLocation::LeftBorder),
        )
    }
    layered_edges_it
        .enumerate()
        .for_each(|(layer_idx, mut segments)| {
            determine_segment_layers(
                graph,
                &mut mf_store,
                &mut segments,
                &mut buffer,
                if layer_idx + 1 < graph.num_layers {
                    graph
                        .config
                        .edge_segment_space(EdgeSegmentSpaceLocation::After(LayerId(layer_idx)))
                } else {
                    graph
                        .config
                        .edge_segment_space(EdgeSegmentSpaceLocation::AfterLast(LayerId(layer_idx)))
                },
            )
        });

    let mut layered_mfs = get_layered_mfs(graph, &mf_store);
    layered_mfs
        .iter_mut()
        .enumerate()
        .for_each(|(layer_idx, segments)| {
            let reference_pool = &graph.pools[PoolId(layer_idx)];
            let start_x = reference_pool.y.strict_add(reference_pool.height);
            let len = graph.config.vertical_space_between_pools;
            let end_x = start_x.strict_add(len);
            let center_x = start_x + len / 2;
            determine_segment_layers(
                graph,
                &mut mf_store,
                segments,
                &mut buffer,
                EdgeSegmentSpace {
                    start_x,
                    end_x,
                    center_x,
                },
            )
        });
}

fn determine_segment_layers(
    graph: &mut Graph,
    mf_store: &mut MessageFlowBendPointStore,
    routing_edges: &mut SegmentsOfSameLayer,
    buffer: &mut Vec</* max_y */ usize>,
    total_space: EdgeSegmentSpace,
) {
    // At this point the edges in each Vec are sorted by (min_y, max_y).
    // Now we need to take this information and make it into a rough layer assignment,
    // i.e. set idx values into the SegmentLayer variables. This is required to understand what
    // is the left-to-right order of all edge segments. The neat part is that this can be super
    // rough. For example, for upwards edges it basically is sufficient to set their idx value to
    // the order as they appear from `min_y=0..inf`. Later there is a scanline graph which checks
    // which segments are present at the same `y` value, and this just needs to know in general what
    // is left and what is right. Comments are within the functions.
    let mut total_count_of_segment_layers = 0;
    total_count_of_segment_layers += determine_segment_layers_left_or_right_loops(
        &mut routing_edges.left_loops,
        buffer,
        total_count_of_segment_layers,
        false,
    );
    total_count_of_segment_layers += determine_segment_layers_up_or_down_edges(
        &mut routing_edges.up_message_flows,
        buffer,
        total_count_of_segment_layers,
        false,
    );
    total_count_of_segment_layers += determine_segment_layers_up_or_down_edges(
        &mut routing_edges.up_edges,
        buffer,
        total_count_of_segment_layers,
        false,
    );
    total_count_of_segment_layers += determine_segment_layers_ixi(
        &mut routing_edges.ixi_crossing,
        buffer,
        total_count_of_segment_layers,
    );
    total_count_of_segment_layers += determine_segment_layers_up_or_down_edges(
        &mut routing_edges.down_edges,
        buffer,
        total_count_of_segment_layers,
        true,
    );
    total_count_of_segment_layers += determine_segment_layers_up_or_down_edges(
        &mut routing_edges.down_message_flows,
        buffer,
        total_count_of_segment_layers,
        true,
    );
    total_count_of_segment_layers += determine_segment_layers_left_or_right_loops(
        &mut routing_edges.right_loops,
        buffer,
        total_count_of_segment_layers,
        true,
    );

    // Silence clippy.
    let _ = total_count_of_segment_layers;

    // Now that the rough left-of relationship is clear for the segments, we can calculate the
    // real x coordinates (or y coordinates in the transpose case for message flows).
    // This is done by iterating paths of overlapping segments (from left to right) across all the
    // categories (see the `kmerge_by` argument). Note that we are always iterating full paths from
    // longest to shortest.
    let mut segment_graph = SegmentGraph::default();
    let mut sliding_window_of_segments = Vec::<&VerticalSegment>::new();
    let (ixi_above, ixi_below): (Vec<_>, Vec<_>) =
        routing_edges.ixi_crossing.iter().cloned().unzip();

    itertools::kmerge_by(
        [
            routing_edges.left_loops.iter(),
            routing_edges.up_message_flows.iter(),
            routing_edges.up_edges.iter(),
            ixi_above.iter(),
            ixi_below.iter(),
            routing_edges.down_edges.iter(),
            routing_edges.down_message_flows.iter(),
            routing_edges.right_loops.iter(),
        ]
        .iter_mut(),
        |left: &&VerticalSegment, right: &&VerticalSegment| left.min_y() < right.min_y(),
    )
    .for_each(|new_edge| {
        sliding_window_of_segments.retain(|old_edge| old_edge.max_y() > new_edge.min_y());
        sliding_window_of_segments.push(new_edge);
        sliding_window_of_segments.sort_unstable_by_key(|edge| edge.idx);
        let idx = sliding_window_of_segments
            .iter()
            .position(|edge| edge.id == new_edge.id)
            .unwrap();
        segment_graph.add_edge(
            new_edge,
            idx.checked_sub(1)
                .and_then(|idx| sliding_window_of_segments.get(idx))
                .map(|edge| edge.id),
            sliding_window_of_segments.get(idx + 1).copied(),
        );
    });
    let all_paths = segment_graph.all_paths_longest_first();
    let EdgeSegmentSpace {
        start_x,
        end_x,
        center_x,
    } = total_space;

    #[derive(Debug)]
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
    let count_of_comfortably_fitting_edge_segments = (horizontal_space / segspace) - 1;
    if path.len() >= count_of_comfortably_fitting_edge_segments {
        // Must squeeze them all into the space. No partitioning.
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
    assert!(leftmost_center_x <= rightmost_center_x);
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
    sort_buffer: &mut Vec</* max_y */ usize>,
    base_segment_layer: usize,
    reverse: bool,
) -> usize {
    sort_buffer.clear();
    sort_buffer.extend(0..routing_edges.len());
    sort_buffer.sort_unstable_by_key(|edge_idx| {
        let segment = &routing_edges[*edge_idx];
        // * First come sequence flows, then message flows. Usually message flows are longer than
        //   sequence flows and hence are usually second, but in a high pool this could be different.
        //   To ensure the ordering is consistent, be explicit about this ordering constraint.
        // * First come short loops, then long loops. Short loops have the chance of being
        //   "contained" in a long loop, thus avoiding edge crossings.
        // * First come upper segments, then lower ones. This is just general sort order.
        (
            segment.is_message_flow,
            segment.start_y.abs_diff(segment.end_y),
            segment.min_y(),
        )
    });

    let logical_idx = LogicalIdx::new(routing_edges.len(), reverse);

    sort_buffer.iter().enumerate().for_each(|(counter, idx)| {
        routing_edges[*idx].idx = logical_idx.compute(counter) + base_segment_layer;
    });

    routing_edges.len()
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
            left.ixi_diagonalizer = Some(0);
            right.idx = base_segment_layer + 1;
            right.ixi_diagonalizer = Some(1);
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
        for (left, right) in routing_edges.iter_mut() {
            if left.idx == from_idx {
                // Can only set the `right` one here since `left` is still used as a needle.
                right.idx = new_idx;
                right.ixi_diagonalizer = Some(1);
            }
        }
    }

    for (left, right) in routing_edges.iter_mut() {
        left.idx = right.idx;
        left.ixi_diagonalizer = Some(0);
    }

    max_y_per_layer_buffer.len() * 2
}

fn determine_segment_layers_up_or_down_edges(
    routing_edges: &mut [VerticalSegment],
    sort_buffer: &mut Vec</* max_y */ usize>,
    base_segment_layer: usize,
    reverse: bool,
) -> usize {
    sort_buffer.clear();
    sort_buffer.extend(0..routing_edges.len());
    sort_buffer.sort_unstable_by_key(|edge_idx| {
        let segment = &routing_edges[*edge_idx];
        // * First come sequence flows, then message flows. Usually message flows are longer than
        //   sequence flows and hence are usually second, but in a high pool this could be different.
        //   To ensure the ordering is consistent, be explicit about this ordering constraint.
        // * First come upper segments, then lower ones. This is just general sort order.
        (segment.is_message_flow, segment.min_y())
    });

    let logical_idx = LogicalIdx::new(routing_edges.len(), reverse);

    sort_buffer.iter().enumerate().for_each(|(counter, idx)| {
        routing_edges[*idx].idx = logical_idx.compute(counter) + base_segment_layer;
    });

    routing_edges.len()
}

fn interpool_y(graph: &Graph, from_pool: PoolId, to_pool: PoolId) -> (PoolId, usize) {
    // MFs bend away immediately when they leave their `from` pool. Port ordering is set up as per
    // this convention.
    let pool_id = if from_pool < to_pool {
        from_pool
    } else {
        assert!(
            from_pool > to_pool,
            "from_pool: {from_pool:?}, to_pool: {to_pool:?}"
        );
        PoolId(from_pool.0 - 1)
    };
    let reference_pool = &graph.pools[pool_id];
    (pool_id, reference_pool.y.strict_add(reference_pool.height))
}

fn get_layered_edges(graph: &mut Graph) -> (Vec<SegmentsOfSameLayer>, MessageFlowBendPointStore) {
    let mut mf_store = MessageFlowBendPointStore::default();
    let mut edge_layers = Vec::<Vec<VerticalSegment>>::new();
    edge_layers.resize_with(graph.num_layers + 1, Default::default);

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
        let AbsolutePort {
            y: start_y,
            x: start_x,
        } = from_node.port_of_outgoing(edge_id);
        let AbsolutePort { y: end_y, x: end_x } = to_node.port_of_incoming(edge_id);

        /////////////////////////////
        // Sequence / Data Flow Edges
        /////////////////////////////

        if !edge.is_message_flow() {
            if start_y == end_y {
                continue;
            }
            let segment_vec = &mut edge_layers[from_node.layer_id.0 + 1];
            segment_vec.push(VerticalSegment {
                id: edge_id,
                start_y,
                end_y,
                idx: 0,
                ixi_diagonalizer: None,
                alignment: Alignment::Center,
                is_message_flow: false,
                x_coordinate: Default::default(),
            });
            continue;
        }

        /////////////////////
        // Message Flow Edges
        /////////////////////

        assert!(edge.is_message_flow());

        // A message flow may also be cut into smaller dummies if it exits vertically but is then
        // sent horizontally in-pool through a bend dummy. In that case we do the stitching-together
        // already here in this file within the `finish_layer` function. But the important part is
        // that we only analyze the message flow part which is really going from one pool to
        // another. The other up to two vertical segment are then manually put together.
        if from_node.pool == to_node.pool {
            // Just the segment between a real node and the bend dummy.
            continue;
        }

        let from_hor = from_node.is_bend_dummy() || from_node.port_is_left_or_right(start_y);
        let to_hor = to_node.is_bend_dummy() || to_node.port_is_left_or_right(end_y);
        let (bends_after_pool, interpool_y) = interpool_y(graph, from_node.pool, to_node.pool);
        if from_hor && to_hor {
            if from_node.layer_id.0 + 1 == to_node.layer_id.0 {
                // The edge goes to the next layer, so there is just one vertical segment. No
                // inter-pool horizontal segment required.
                let segment_vec = &mut edge_layers[to_node.layer_id.0];
                segment_vec.push(VerticalSegment {
                    id: edge_id,
                    start_y,
                    end_y,
                    idx: 0,
                    ixi_diagonalizer: None,
                    alignment: Alignment::Center,
                    is_message_flow: true,
                    x_coordinate: Default::default(),
                });
                mf_store.register_edge(edge_id, MessageFlowBendState::HorVerHor);
            } else {
                let segment_vec = &mut edge_layers[from_node.layer_id.0 + 1];
                segment_vec.push(VerticalSegment {
                    id: edge_id,
                    start_y,
                    end_y: interpool_y,
                    idx: 0,
                    ixi_diagonalizer: None,
                    alignment: Alignment::Center,
                    is_message_flow: true,
                    x_coordinate: Default::default(),
                });
                let segment_vec = &mut edge_layers[to_node.layer_id.0];
                segment_vec.push(VerticalSegment {
                    id: edge_id,
                    start_y: interpool_y,
                    end_y,
                    idx: 0,
                    ixi_diagonalizer: None,
                    alignment: Alignment::Center,
                    is_message_flow: true,
                    x_coordinate: Default::default(),
                });
                mf_store.register_edge(
                    edge_id,
                    MessageFlowBendState::HorVerHorVerHor {
                        bends_after_pool,
                        bend_point_1: None,
                        bend_point_2: None,
                    },
                );
            }
        } else if from_hor {
            assert!(!to_hor);
            let segment_vec = &mut edge_layers[from_node.layer_id.0 + 1];
            segment_vec.push(VerticalSegment {
                id: edge_id,
                start_y,
                end_y: interpool_y,
                idx: 0,
                ixi_diagonalizer: None,
                alignment: Alignment::Center,
                is_message_flow: true,
                x_coordinate: Default::default(),
            });
            mf_store.register_edge(
                edge_id,
                MessageFlowBendState::HorVerHorVer {
                    bends_after_pool,
                    bend_point_1: None,
                    interpool_bendpoint2_x: end_x,
                },
            );
        } else if to_hor {
            assert!(!from_hor);
            let segment_vec = &mut edge_layers[to_node.layer_id.0];
            segment_vec.push(VerticalSegment {
                id: edge_id,
                start_y: interpool_y,
                end_y,
                idx: 0,
                ixi_diagonalizer: None,
                alignment: Alignment::Center,
                is_message_flow: true,
                x_coordinate: Default::default(),
            });
            mf_store.register_edge(
                edge_id,
                MessageFlowBendState::VerHorVerHor {
                    bends_after_pool,
                    interpool_bendpoint1_x: start_x,
                    bend_point_2: None,
                },
            );
        } else {
            assert!(!to_hor);
            assert!(!from_hor);
            //let segment_vec = &mut edge_layers[from_node.layer_id.0 + 1];
            //segment_vec.push(VerticalSegment {
            //    id: edge_id,
            //    start_y: interpool_y,
            //    end_y,
            //    idx: 0,
            //    idx2: None,
            //    alignment: Alignment::Center,
            //    is_message_flow: true,
            //    x_coordinate: Default::default(),
            //});
            mf_store.register_edge(
                edge_id,
                MessageFlowBendState::VerHorVer {
                    bends_after_pool,
                    interpool_bendpoint1_x: start_x,
                    interpool_bendpoint2_x: end_x,
                },
            );
        }
    }

    let mut result: Vec<SegmentsOfSameLayer> = vec![];
    for mut edge_layer in edge_layers.into_iter() {
        // sort by min_y: To identify groups
        // sort by max_y: To later be able to easily spot ixi crossings from the up_edges and
        // down_edges vectors.
        edge_layer.sort_unstable_by_key(|e| (e.min_y(), e.max_y()));
        // Chunk as long as edges are overlapping.
        let mut segments = SegmentsOfSameLayer::default();
        edge_layer.into_iter().for_each(|mut edge| {
            // TODO for self loops, check if edges[_id].from or .to is a special loop helper node,
            // in which case this should become a right_loops or left_loops member,
            // respectively.
            if edge.is_message_flow {
                // If the message flow goes to the left, then it consists of two looping segments.
                let to = &to!(edge.id);
                let from = &from!(edge.id);
                let layer = result.len();
                if to.layer_id <= from.layer_id {
                    if from.layer_id.0 + 1 == layer {
                        edge.alignment = Alignment::Left;
                        segments.left_loops.push(edge);
                    } else {
                        edge.alignment = Alignment::Right;
                        segments.right_loops.push(edge);
                    }
                    return;
                }
            }
            match edge.start_y.cmp(&edge.end_y) {
                std::cmp::Ordering::Less if edge.is_message_flow => {
                    edge.alignment = Alignment::Right;
                    segments.down_message_flows.push(edge);
                }
                std::cmp::Ordering::Greater if edge.is_message_flow => {
                    edge.alignment = Alignment::Left;
                    segments.up_message_flows.push(edge);
                }
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
                // No need to alter `i` or `j`, as we removed the elements at that location, and
                // need to check the new elements in the next iteration.
            } else if up < down {
                i += 1;
            } else {
                j += 1;
            }
        }
        result.push(segments);
    }
    (result, mf_store)
}

fn get_layered_mfs(
    graph: &mut Graph,
    mf_store: &MessageFlowBendPointStore,
) -> Vec<SegmentsOfSameLayer> {
    let mut edge_layers = Vec::<Vec<VerticalSegment>>::new();
    edge_layers.resize_with(graph.pools.len(), Default::default);

    for (pool_id, edge_id, StartEnd { start, end }) in mf_store.iter_edges_for_break_after_pool() {
        assert_ne!(start, end);
        let ((_, start_y), (_, end_y)) = transpose(
            (start, 0),
            (end, 0),
            from!(edge_id).pool < to!(edge_id).pool,
        );
        let segment_vec = &mut edge_layers[pool_id.0];
        segment_vec.push(VerticalSegment {
            id: edge_id,
            start_y,
            end_y,
            idx: 0,
            ixi_diagonalizer: None,
            alignment: Alignment::Center,
            is_message_flow: true,
            x_coordinate: Default::default(),
        });
    }

    let mut result: Vec<SegmentsOfSameLayer> = vec![];
    for mut edge_layer in edge_layers.into_iter() {
        // sort by min_y: To identify groups
        // sort by max_y: To later be able to easily spot ixi crossings from the up_edges and
        // down_edges vectors.
        edge_layer.sort_unstable_by_key(|e| (e.min_y(), e.max_y()));
        // Chunk as long as edges are overlapping.
        let mut segments = SegmentsOfSameLayer::default();
        edge_layer.into_iter().for_each(|edge| {
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
        // TODO could we make this reusable? Duplicated in `get_layered_edges`.
        let mut i = 0;
        let mut j = 0;
        while i < segments.up_edges.len() && j < segments.down_edges.len() {
            let up = (segments.up_edges[i].start_y, segments.up_edges[i].end_y);
            let down = (segments.down_edges[j].end_y, segments.down_edges[j].start_y);
            if up == down {
                segments
                    .ixi_crossing
                    .push((segments.up_edges.remove(i), segments.down_edges.remove(j)));
                // No need to alter `i` or `j`, as we removed the elements at that location, and
                // need to check the new elements in the next iteration.
            } else if up < down {
                i += 1;
            } else {
                j += 1;
            }
        }
        result.push(segments);
    }
    result
}

fn add_bend_points_one_segment(
    graph: &mut Graph,
    mf_store: &mut MessageFlowBendPointStore,
    segment: &VerticalSegment,
    idx: usize,
    segment_layer_width: f64,
    min_x: usize,
) {
    // `idx` in the caller starts at 0, but `min_x` is actually expecting one additional padding.
    // Sorry for the spaghetti.
    let x = min_x
        + ((idx + 1 - segment.ixi_diagonalizer.unwrap_or(0)) as f64 * segment_layer_width) as usize;
    let x2 = min_x
        + ((idx + 1 + (1 - segment.ixi_diagonalizer.unwrap_or(1))) as f64 * segment_layer_width)
            as usize;
    match segment.ixi_diagonalizer {
        Some(0) => segment.x_coordinate.set(Some(x)),
        Some(1) => segment.x_coordinate.set(Some(x2)),
        Some(_) => unreachable!(),
        _ => segment.x_coordinate.set(Some(x)),
    }
    let edge = &mut e!(segment.id);
    let (p1, p2) = ((x, segment.start_y), (x2, segment.end_y));
    match &mut edge.edge_type {
        _ if segment.is_message_flow => {
            mf_store.finish_layer(graph, segment.id, p1, p2);
        }
        EdgeType::Regular {
            bend_points: out_bend_points,
            ..
        } => {
            // BACKLOG: inlining graph.start_and_end_ports due to borrow checker.
            // TODO make this simply a standalone function taking `&mut graph.nodes`.
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

struct LogicalIdx {
    rev_multiplier: isize,
    rev_offset: isize,
}

impl LogicalIdx {
    fn new(len: usize, reverse: bool) -> Self {
        Self {
            rev_multiplier: if reverse { -1 } else { 1 },
            rev_offset: if reverse { (len - 1) as isize } else { 0 },
        }
    }

    fn compute(&self, idx: usize) -> usize {
        (self.rev_offset + self.rev_multiplier * idx as isize) as usize
    }
}

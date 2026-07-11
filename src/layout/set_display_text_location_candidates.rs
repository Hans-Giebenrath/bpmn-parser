use std::{num::NonZero, ops::ControlFlow};

use crate::{
    common::{
        bpmn_node::BpmnNode,
        edge::{EdgeType, RegularEdgeBendPoints},
        graph::Graph,
        node::{Dimension, NodeType},
    },
    layout::collision_grid::{Grid, Line},
};

pub struct DisplayTextLocationCandidate {
    alignment: Alignment,
    reference_point: ReferencePoint,
    /// In reference to the x,y coordinates of the [Node] itself, to be multiplied with the display
    /// text margin value of the graph config.
    x_margin_multiplier: f32,
    y_margin_multiplier: f32,
}

pub struct EdgeDisplayTextLocationCandidate {
    alignment: Alignment,
    reference_point: ReferencePoint,
    x: usize,
    y: usize,
}

// Indices for the display text location candidates. The arrays are laid out as such.
// Nodes contain then a list of
const TOP_LEFT: usize = 0;
const TOP: usize = 1;
const TOP_RIGHT: usize = 2;
const RIGHT: usize = 3;
const BOTTOM_RIGHT: usize = 4;
const BOTTOM: usize = 5;
const BOTTOM_LEFT: usize = 6;
const LEFT: usize = 7;

pub struct PackedIndicesForDisplayTextLocation {
    /// Since every index is just 0..=7, we can put it into 3 bits. At 8 numbers, we have
    /// just 8 * 3 = 24 bits, so it fits into a regular u32 number. Genius!
    bits: u32,
}

///      ABC     ABC    ABC    ABC     ABC         ^
///  ---------------------------------------->     v `margin` distance between edge and display text
///  |<-->| <- margin to ends
///       X       X       X       X       X   <- if just applying steps
///                      C <- The actual center of the edge. The ABC was collapsed here because
///                           it would otherwise be too close to the center, resulting in weird
///                           visuals.
///                             ^ on this side we try mirror the modified step size around the center,
///                               so that we reach the farthest end of the edge (otherwise would
///                               overrun since step size is probably not a perfect multiple of the
///                               `edge length - 2*margin_to_ends`)
struct Config {
    margin: u32,
    edge_x_step_size: NonZero<u32>,
    edge_y_step_size: NonZero<u32>,
    edge_x_margin_to_ends: u32,
    edge_y_margin_to_ends: u32,
    edge_x_min_distance_to_center_or_else_collapse_into_center: NonZero<u32>,
    edge_y_min_distance_to_center_or_else_collapse_into_center: NonZero<u32>,
}

impl PackedIndicesForDisplayTextLocation {
    fn new(indices: [usize; 8]) -> PackedIndicesForDisplayTextLocation {
        let mut bits: u32 = 0;
        for index in indices.iter().rev() {
            bits |= *index as u32;
            bits <<= 3;
        }
        Self { bits }
    }

    /// Use the result as indices into either [GATEWAY_CANDIDATES] or [EVENT_CANDIDATES].
    fn unpack(&self) -> [usize; 8] {
        let mut indices = [0, 0, 0, 0, 0, 0, 0, 0];
        let mut bits = self.bits;
        for index in indices.iter_mut() {
            *index = (bits & 0b111) as usize;
            bits >>= 3;
        }
        indices
    }
}

type DisplayLocationCallback = dyn Fn(&EdgeDisplayTextLocationCandidate) -> ControlFlow<(), f64>;

/// So the text is a bit moved closer to the gateway if it is placed in a corner,
/// otherwise I believe it is just appearing a little detached.
const GATEWAY_CORNER_FRACTIONAL_MARGIN: f32 = 0.8;

pub const GATEWAY_CANDIDATES: [DisplayTextLocationCandidate; 8] = [
    // Top Left
    DisplayTextLocationCandidate {
        alignment: Alignment::Right,
        reference_point: ReferencePoint::RightBottom,
        x_margin_multiplier: -GATEWAY_CORNER_FRACTIONAL_MARGIN,
        y_margin_multiplier: -GATEWAY_CORNER_FRACTIONAL_MARGIN,
    },
    // Top
    DisplayTextLocationCandidate {
        alignment: Alignment::Center,
        reference_point: ReferencePoint::CenterBottom,
        x_margin_multiplier: 0.,
        y_margin_multiplier: -1.,
    },
    // Top Right
    DisplayTextLocationCandidate {
        alignment: Alignment::Left,
        reference_point: ReferencePoint::LeftBottom,
        x_margin_multiplier: GATEWAY_CORNER_FRACTIONAL_MARGIN,
        y_margin_multiplier: -GATEWAY_CORNER_FRACTIONAL_MARGIN,
    },
    // Right
    DisplayTextLocationCandidate {
        alignment: Alignment::Left,
        reference_point: ReferencePoint::LeftCenter,
        x_margin_multiplier: 1.,
        y_margin_multiplier: 0.,
    },
    // Bottom Right
    DisplayTextLocationCandidate {
        alignment: Alignment::Left,
        reference_point: ReferencePoint::LeftTop,
        x_margin_multiplier: GATEWAY_CORNER_FRACTIONAL_MARGIN,
        y_margin_multiplier: GATEWAY_CORNER_FRACTIONAL_MARGIN,
    },
    // Bottom
    DisplayTextLocationCandidate {
        alignment: Alignment::Center,
        reference_point: ReferencePoint::CenterTop,
        x_margin_multiplier: 0.,
        y_margin_multiplier: 1.,
    },
    // Bottom Left
    DisplayTextLocationCandidate {
        alignment: Alignment::Right,
        reference_point: ReferencePoint::RightTop,
        x_margin_multiplier: -GATEWAY_CORNER_FRACTIONAL_MARGIN,
        y_margin_multiplier: GATEWAY_CORNER_FRACTIONAL_MARGIN,
    },
    // Left
    DisplayTextLocationCandidate {
        alignment: Alignment::Right,
        reference_point: ReferencePoint::RightCenter,
        x_margin_multiplier: -1.,
        y_margin_multiplier: 0.,
    },
];

/// So the text is a bit moved closer to the gateway if it is placed in a corner,
/// otherwise I believe it is just appearing a little detached. A little less
/// extreme than the gateway, since for events there is rounding.
const EVENT_CORNER_FRACTIONAL_MARGIN: f32 = 0.93;

pub const EVENT_CANDIDATES: [DisplayTextLocationCandidate; 8] = [
    // Top Left
    DisplayTextLocationCandidate {
        alignment: Alignment::Right,
        reference_point: ReferencePoint::RightBottom,
        x_margin_multiplier: -EVENT_CORNER_FRACTIONAL_MARGIN,
        y_margin_multiplier: -EVENT_CORNER_FRACTIONAL_MARGIN,
    },
    // Top
    DisplayTextLocationCandidate {
        alignment: Alignment::Center,
        reference_point: ReferencePoint::CenterBottom,
        x_margin_multiplier: 0.,
        y_margin_multiplier: -1.,
    },
    // Top Right
    DisplayTextLocationCandidate {
        alignment: Alignment::Left,
        reference_point: ReferencePoint::LeftBottom,
        x_margin_multiplier: EVENT_CORNER_FRACTIONAL_MARGIN,
        y_margin_multiplier: -EVENT_CORNER_FRACTIONAL_MARGIN,
    },
    // Right
    DisplayTextLocationCandidate {
        alignment: Alignment::Left,
        reference_point: ReferencePoint::LeftCenter,
        x_margin_multiplier: 1.,
        y_margin_multiplier: 0.,
    },
    // Bottom Right
    DisplayTextLocationCandidate {
        alignment: Alignment::Left,
        reference_point: ReferencePoint::LeftTop,
        x_margin_multiplier: EVENT_CORNER_FRACTIONAL_MARGIN,
        y_margin_multiplier: EVENT_CORNER_FRACTIONAL_MARGIN,
    },
    // Bottom
    DisplayTextLocationCandidate {
        alignment: Alignment::Center,
        reference_point: ReferencePoint::CenterTop,
        x_margin_multiplier: 0.,
        y_margin_multiplier: 1.,
    },
    // Bottom Left
    DisplayTextLocationCandidate {
        alignment: Alignment::Right,
        reference_point: ReferencePoint::RightTop,
        x_margin_multiplier: -EVENT_CORNER_FRACTIONAL_MARGIN,
        y_margin_multiplier: EVENT_CORNER_FRACTIONAL_MARGIN,
    },
    // Left
    DisplayTextLocationCandidate {
        alignment: Alignment::Right,
        reference_point: ReferencePoint::RightCenter,
        x_margin_multiplier: -1.,
        y_margin_multiplier: 0.,
    },
];

pub fn edge_display_text_location_candidates(
    config: &Config,
    line_points: &[(usize, usize)],
    // A gen fn would be cooler, but then again I maybe need to rotate the start
    callback: &DisplayLocationCallback,
) -> EdgeDisplayTextLocationCandidate {
    let mut it = line_points.iter().cloned().peekable();
    let mut best_candidate_score = f64::MAX;
    let mut best_candidate = EdgeDisplayTextLocationCandidate {
        alignment: Alignment::Center,
        reference_point: ReferencePoint::Center,
        x: 0,
        y: 0,
    };

    // The most natural position for the label would be on the first horizontal edge segment. So
    // if the edge leaves to the top or bottom, we need to manually pull this segment out.
    let skip_segment = if let [first, second, third, ..] = line_points
        && first.0 == second.0
    {
        match edge_segment_display_text_location_candidates(
            config,
            *second,
            *third,
            callback,
            &mut best_candidate_score,
            &mut best_candidate,
        ) {
            ControlFlow::Break(candidate) => return candidate,
            ControlFlow::Continue(()) => (),
        }
        Some(1)
    } else {
        None
    };

    let mut segment = 0;
    let mut cur_opt = it.next();
    let mut next_opt = it.next();
    while let (Some(cur), Some(next), peeked) = (cur_opt, next_opt, it.peek()) {
        if Some(segment) != skip_segment {
            match edge_segment_display_text_location_candidates(
                config,
                cur,
                next,
                callback,
                &mut best_candidate_score,
                &mut best_candidate,
            ) {
                ControlFlow::Break(candidate) => return candidate,
                ControlFlow::Continue(()) => (),
            }
        }
        if let Some(peeked) = peeked {
            match edge_corner_display_text_location_candidates(
                config,
                cur,
                next,
                *peeked,
                callback,
                &mut best_candidate_score,
                &mut best_candidate,
            ) {
                ControlFlow::Break(candidate) => return candidate,
                ControlFlow::Continue(()) => (),
            }
        }
        segment += 1;
        cur_opt = next_opt;
        next_opt = it.next();
    }
    best_candidate
}

/// Goes from start to end, always oscillating above->below->above or left->right->left (depending
/// on whether the segment is horizontal or vertical).
pub fn edge_segment_display_text_location_candidates(
    config: &Config,
    start: (usize, usize),
    end: (usize, usize),
    callback: &DisplayLocationCallback,
    best_candidate_score: &mut f64,
    best_candidate: &mut EdgeDisplayTextLocationCandidate,
) -> ControlFlow<EdgeDisplayTextLocationCandidate> {
    if start.0 == end.0 {
        // Going up or down.
        let it = iterate_edge_points(
            start.1 as u32,
            end.1 as u32,
            config.edge_y_margin_to_ends,
            config.edge_y_step_size,
            config.edge_y_min_distance_to_center_or_else_collapse_into_center,
        );
        for y in it {
            for candidate in [
                EdgeDisplayTextLocationCandidate {
                    alignment: Alignment::Right,
                    reference_point: ReferencePoint::RightCenter,
                    x: start.0.saturating_sub(config.margin as usize),
                    y: y as usize,
                },
                EdgeDisplayTextLocationCandidate {
                    alignment: Alignment::Left,
                    reference_point: ReferencePoint::LeftCenter,
                    x: start.0.saturating_add(config.margin as usize),
                    y: y as usize,
                },
            ] {
                match callback(&candidate) {
                    ControlFlow::Break(()) => return ControlFlow::Break(candidate),
                    ControlFlow::Continue(score) => {
                        if score < *best_candidate_score {
                            *best_candidate_score = score;
                            *best_candidate = candidate;
                        }
                    }
                }
            }
        }
    } else if start.1 == end.1 {
        // Going right or left.
        let it = iterate_edge_points(
            start.0 as u32,
            end.0 as u32,
            config.edge_x_margin_to_ends,
            config.edge_x_step_size,
            config.edge_x_min_distance_to_center_or_else_collapse_into_center,
        );
        for x in it {
            for candidate in [
                EdgeDisplayTextLocationCandidate {
                    alignment: Alignment::Center,
                    reference_point: ReferencePoint::CenterBottom,
                    y: start.1.saturating_sub(config.margin as usize),
                    x: x as usize,
                },
                EdgeDisplayTextLocationCandidate {
                    alignment: Alignment::Center,
                    reference_point: ReferencePoint::CenterTop,
                    y: start.1.saturating_add(config.margin as usize),
                    x: x as usize,
                },
            ] {
                match callback(&candidate) {
                    ControlFlow::Break(()) => return ControlFlow::Break(candidate),
                    ControlFlow::Continue(score) => {
                        if score < *best_candidate_score {
                            *best_candidate_score = score;
                            *best_candidate = candidate;
                        }
                    }
                }
            }
        }
    } else {
        panic!(
            "Data flows are the only edges which are allowed to be non-vertical, and they should not have labels atm in BPMD"
        );
    }
    ControlFlow::Continue(())
}

fn iterate_edge_points(
    start: u32,
    end: u32,
    margin_to_start: u32,
    step_size: NonZero<u32>,
    collapse_to_center: NonZero<u32>,
) -> impl Iterator<Item = u32> {
    let (new_start, new_end, step_size, mid_point, last_possible_coord) = if start < end {
        let midpoint = start + ((end - start) / 2);
        (
            start as i32 + margin_to_start as i32,
            end as i32 - margin_to_start as i32,
            step_size.get() as i32,
            midpoint,
            midpoint.saturating_sub_signed(collapse_to_center.get() as i32) as i32,
        )
    } else {
        let midpoint = end + ((start - end) / 2);
        (
            start as i32 - margin_to_start as i32,
            end as i32 + margin_to_start as i32,
            -(step_size.get() as i32),
            midpoint,
            midpoint.saturating_add_signed(collapse_to_center.get() as i32) as i32,
        )
    };

    let steps = if start < end {
        if last_possible_coord < new_start {
            0
        } else {
            (last_possible_coord - new_start) / step_size + 1
        }
    } else if start > end {
        if last_possible_coord > new_start {
            0
        } else {
            (last_possible_coord - new_start) / step_size + 1
        }
    } else {
        // start == end, this should never happen ...
        unreachable!("Some edge has length 0?")
    };

    assert!(steps >= 0);
    (0..steps)
        .map(move |i| (new_start + i * step_size) as u32)
        .chain(std::iter::once(mid_point))
        .chain(
            (0..steps)
                .rev()
                .map(move |i| (new_end - i * step_size) as u32),
        )
}

pub fn edge_corner_display_text_location_candidates(
    config: &Config,
    start: (usize, usize),
    middle: (usize, usize),
    end: (usize, usize),
    callback: &DisplayLocationCallback,
    best_candidate_score: &mut f64,
    best_candidate: &mut EdgeDisplayTextLocationCandidate,
) -> ControlFlow<EdgeDisplayTextLocationCandidate> {
}

pub enum ReferencePoint {
    Center,

    LeftTop,
    RightTop,
    LeftBottom,
    RightBottom,

    CenterTop,
    CenterBottom,
    RightCenter,
    LeftCenter,
}

pub enum Alignment {
    Left,
    Center,
    Right,
}

enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

struct SideFreedom {
    left_free: bool,
    left_top_free: bool,
    top_free: bool,
    right_top_free: bool,
    right_free: bool,
    bottom_right_free: bool,
    bottom_free: bool,
    bottom_left_free: bool,
}

pub fn set_display_text_location_candidates(graph: &mut Graph) {
    let grid = prepare_collision_grid(graph);

    for node in &graph.nodes {
        let dim = node.dimension();
        let NodeType::RealNode {
            event,
            display_text_location_candidate_indices,
            ..
        } = &mut node.node_type
        else {
            continue;
        };
        let old_len = graph.display_text_location_candidates.len();
        if matches!(event, BpmnNode::Gateway(..)) {
            find_gateway_label_position(dim, &mut graph)
        } else {
            find_non_gateway_label_position(dim, &mut graph)
        }
        let new_len = graph.display_text_location_candidates.len();
        *display_text_location_candidate_indices = std::range::Range::from(old_len..new_len);
    }
}

struct CandidateBuffer {
    free_candidates_buffer: Vec<DisplayTextLocationCandidate>,
    obstructed_candidates_buffer: Vec<DisplayTextLocationCandidate>,
}

impl CandidateBuffer {
    fn new() -> Self {
        Self {
            free_candidates_buffer: Default::default(),
            obstructed_candidates_buffer: Default::default(),
        }
    }

    fn drain<'a>(&'a mut self) -> impl Iterator<Item = DisplayTextLocationCandidate> + 'a {
        self.free_candidates_buffer
            .drain(..)
            .chain(self.obstructed_candidates_buffer.drain(..))
    }

    fn insert(&mut self, candidate: DisplayTextLocationCandidate, free: bool) {
        if free {
            self.free_candidates_buffer.push(candidate);
        } else {
            self.obstructed_candidates_buffer.push(candidate);
        }
    }
}

fn find_gateway_label_position(
    dim: Dimension,
    graph: &mut Graph,
    side_freedom: SideFreedom,
    incoming_from: Side,
    candidates_buffer: &mut CandidateBuffer,
) {
    let margin = graph.config.display_text_margin;
    const LEN: usize = 8;

    graph.display_text_location_candidates.extend();
}

fn prepare_collision_grid(graph: &Graph) -> Grid {
    let (total_width, total_height) = graph.total_width_height();
    let mut quad_tree = Grid::new(total_width, total_height);

    for edge in &graph.edges {
        let EdgeType::Regular {
            bend_points: RegularEdgeBendPoints::FullyRouted(bend_points),
            ..
        } = &edge.edge_type
        else {
            unreachable!("Only regular edges at this point");
        };
        for [start, end] in bend_points.array_windows() {
            quad_tree.insert(&Line::new(start, end));
        }
    }

    for node in &graph.nodes {
        if node.is_gateway() {
            #[rustfmt::skip]
            let (top, right, bottom, left)   = (
                (node.x + node.width / 2, node.y                  ),
                (node.x + node.width    , node.y + node.height / 2),
                (node.x + node.width / 2, node.y + node.height    ),
                (node.x                 , node.y + node.height / 2)
             );

            quad_tree.insert(&Line::new(&top, &right));
            quad_tree.insert(&Line::new(&top, &left));
            quad_tree.insert(&Line::new(&bottom, &right));
            quad_tree.insert(&Line::new(&bottom, &left));
        } else {
            let tl = (node.x, node.y);
            let tr = (node.x + node.width, node.y);
            let br = (node.x + node.width, node.y + node.height);
            let bl = (node.x, node.y + node.height);

            quad_tree.insert(&Line::new(&tl, &tr));
            quad_tree.insert(&Line::new(&tl, &bl));
            quad_tree.insert(&Line::new(&br, &tr));
            quad_tree.insert(&Line::new(&br, &bl));
        }
    }

    // At this point I ignore the pool and lane edges. I probably regret it very quickly.

    quad_tree
}

use bpmd_util::collision_grid::Grid;
use bpmd_util::collision_grid::Line;
use core::{num::NonZero, ops::ControlFlow};
use cosmic_text::{Align, Attrs, Buffer, Metrics, Shaping, Wrap};

use bpmd_graph::*;

pub fn set_display_text_locations(graph: &mut Graph, cache: &mut FontCache) {
    let mut grid = prepare_collision_grid(graph);
    for node in &mut graph.nodes {
        if node.is_blackbox_node() {
            // TODO this is a bit ugly, combine the next `let NodeType::RealNode` with a match.
            continue;
        }
        let NodeType::RealNode {
            display_text,
            event,
            ..
        } = &mut node.node_type
        else {
            unreachable!();
        };
        if display_text.raw_text.is_empty() {
            continue;
        }
        let text_dims = prep(cache, display_text);
        let (x, y) = (node.x, node.y);
        // Saving a bit of room when calling the `side_of_first_incoming_flow` function.
        let side_calc_args = (
            node.width,
            node.height,
            node.incoming.as_slice(),
            node.incoming_ports.as_slice(),
            graph.edges.as_slice(),
        );
        match event {
            BpmnNode::Event(..) => {
                display_text.location = event_display_text_location_candidates(
                    &graph.config,
                    text_dims,
                    Dimension {
                        x,
                        y,
                        width: EVENT_NODE_WIDTH,
                        height: EVENT_NODE_HEIGHT,
                    },
                    side_of_first_incoming_flow(side_calc_args, Edge::is_sequence_flow),
                    &|e: &DisplayTextLocation| grid.box_intersection_weight((e.x, e.y), text_dims),
                );
            }
            BpmnNode::Gateway(..) => {
                display_text.location = gateway_display_text_location_candidates(
                    &graph.config,
                    text_dims,
                    Dimension {
                        x,
                        y,
                        width: GATEWAY_NODE_WIDTH,
                        height: GATEWAY_NODE_HEIGHT,
                    },
                    side_of_first_incoming_flow(side_calc_args, Edge::is_sequence_flow),
                    &|e: &DisplayTextLocation| grid.box_intersection_weight((e.x, e.y), text_dims),
                );
            }
            BpmnNode::Activity(..) => {
                display_text.location = activity_display_text_location_candidates(
                    text_dims,
                    Dimension {
                        x,
                        y,
                        width: ACTIVITY_NODE_WIDTH,
                        height: ACTIVITY_NODE_HEIGHT,
                    },
                );
            }
            BpmnNode::Data(data_type, ..) => {
                let (width, height) = match data_type {
                    DataType::Store => (DATASTORE_NODE_WIDTH, DATASTORE_NODE_HEIGHT),
                    DataType::Object => (DATAOBJECT_NODE_WIDTH, DATAOBJECT_NODE_HEIGHT),
                };
                display_text.location = data_display_text_location_candidates(
                    &graph.config,
                    text_dims,
                    Dimension {
                        x,
                        y,
                        width,
                        height,
                    },
                    side_of_first_incoming_flow(side_calc_args, Edge::is_data_flow),
                    &|e: &DisplayTextLocation| grid.box_intersection_weight((e.x, e.y), text_dims),
                );
            }
        }

        text_into_grid(&display_text.location, text_dims, &mut grid);
    }

    for edge in &mut graph.edges {
        let EdgeType::Regular {
            text: Some(display_text),
            bend_points: RegularEdgeBendPoints::FullyRouted(points),
        } = &mut edge.edge_type
        else {
            continue;
        };

        if points.len() < 2 {
            return;
        }

        let text_dims = prep(cache, display_text);
        if matches!(&edge.flow_type, FlowType::DataFlow(..)) {
            // Data flows are ideally straight, so the fine logic for orthogonal edges won't work.
            let mid = if (points.len() & 1) == 1 {
                // Uneven, so just take middle point.
                points[points.len() / 2]
            } else {
                let a = points[(points.len() / 2) - 1];
                let b = points[points.len() / 2];
                ((a.0 + b.0) / 2, (a.1 + b.1) / 2)
            };
            display_text.location = DisplayTextLocation {
                alignment: Alignment::Center,
                x: mid.0,
                y: mid.1.saturating_sub(display_text.line_height as usize / 2),
            };
        } else {
            display_text.location = edge_display_text_location_candidates(
                &graph.config,
                text_dims,
                points,
                &|e: &DisplayTextLocation| grid.box_intersection_weight((e.x, e.y), text_dims),
            );
        }
        text_into_grid(&display_text.location, text_dims, &mut grid);
    }
}

struct DisplayTextLocationCandidateInner {
    alignment: Alignment,
    reference_point: ReferencePoint,
    x: usize,
    y: usize,
}

impl DisplayTextLocationCandidateInner {
    fn materialize(&self, (width, height): (usize, usize)) -> DisplayTextLocation {
        let x = match self.reference_point {
            ReferencePoint::Center | ReferencePoint::CenterTop | ReferencePoint::CenterBottom => {
                self.x.saturating_sub(width / 2)
            }
            ReferencePoint::LeftTop | ReferencePoint::LeftCenter | ReferencePoint::LeftBottom => {
                self.x
            }
            ReferencePoint::RightTop
            | ReferencePoint::RightCenter
            | ReferencePoint::RightBottom => self.x.saturating_sub(width),
        };

        let y = match self.reference_point {
            ReferencePoint::LeftTop | ReferencePoint::CenterTop | ReferencePoint::RightTop => {
                self.y
            }
            ReferencePoint::LeftCenter | ReferencePoint::Center | ReferencePoint::RightCenter => {
                self.y.saturating_sub(height / 2)
            }
            ReferencePoint::LeftBottom
            | ReferencePoint::CenterBottom
            | ReferencePoint::RightBottom => self.y.saturating_sub(height),
        };

        DisplayTextLocation {
            alignment: self.alignment,
            x,
            y,
        }
    }
}

struct CandidateTracker<'a> {
    score: u32,
    candidate: DisplayTextLocation,
    textbox_wh: (usize, usize),
    callback: &'a DisplayLocationCallback<'a>,
}

impl<'a> CandidateTracker<'a> {
    fn call_and_maybe_update(
        &mut self,
        candidate: &DisplayTextLocationCandidateInner,
    ) -> ControlFlow<()> {
        let candidate = candidate.materialize(self.textbox_wh);
        let score = (*self.callback)(&candidate);

        if score < self.score {
            self.score = score;
            self.candidate = candidate;
        }

        if self.score == 0 {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}
type DisplayLocationCallback<'a> = dyn Fn(&DisplayTextLocation) -> u32 + 'a;

fn edge_display_text_location_candidates(
    config: &Config,
    textbox_wh: (usize, usize),
    line_points: &[(usize, usize)],
    // A gen fn would be cooler, but then again I maybe need to rotate the start
    callback: &DisplayLocationCallback,
) -> DisplayTextLocation {
    let mut it = line_points.iter().cloned().peekable();
    let mut best_candidate = CandidateTracker {
        score: u32::MAX,
        candidate: DisplayTextLocationCandidateInner {
            alignment: Alignment::Center,
            reference_point: ReferencePoint::Center,
            x: line_points.first().cloned().unwrap_or_default().0,
            y: line_points.first().cloned().unwrap_or_default().1,
        }
        .materialize(textbox_wh),
        textbox_wh,
        callback,
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
            &mut best_candidate,
        ) {
            ControlFlow::Break(()) => return best_candidate.candidate,
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
                &mut best_candidate,
            ) {
                ControlFlow::Break(()) => return best_candidate.candidate,
                ControlFlow::Continue(()) => (),
            }
        }
        if let Some(peeked) = peeked {
            match edge_corner_display_text_location_candidates(
                config,
                cur,
                next,
                *peeked,
                &mut best_candidate,
            ) {
                ControlFlow::Break(()) => return best_candidate.candidate,
                ControlFlow::Continue(()) => (),
            }
        }
        segment += 1;
        cur_opt = next_opt;
        next_opt = it.next();
    }
    best_candidate.candidate
}

/// Goes from start to end, always oscillating above->below->above or left->right->left (depending
/// on whether the segment is horizontal or vertical).
fn edge_segment_display_text_location_candidates(
    config: &Config,
    start: (usize, usize),
    end: (usize, usize),
    best_candidate: &mut CandidateTracker,
) -> ControlFlow<()> {
    if start.0 == end.0 {
        // Going up or down.
        let it = iterate_edge_points(
            start.1 as u32,
            end.1 as u32,
            config.display_text_edge_y_margin_to_ends,
            config.display_text_edge_y_step_size.try_into().unwrap(),
            config
                .display_text_edge_y_min_distance_to_center_or_else_collapse_into_center
                .try_into()
                .unwrap(),
        );
        for y in it {
            for candidate in [
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Right,
                    reference_point: ReferencePoint::RightCenter,
                    x: start.0.saturating_sub(config.display_text_margin as usize),
                    y: y as usize,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Left,
                    reference_point: ReferencePoint::LeftCenter,
                    x: start.0.saturating_add(config.display_text_margin as usize),
                    y: y as usize,
                },
            ] {
                best_candidate.call_and_maybe_update(&candidate)?;
            }
        }
    } else if start.1 == end.1 {
        // Going right or left.

        // These first ones are meant to allow rough alignment, especially for gateway labels to
        // look more uniform.
        if start.0 < end.0 {
            best_candidate.call_and_maybe_update(&DisplayTextLocationCandidateInner {
                alignment: Alignment::Center,
                reference_point: ReferencePoint::LeftBottom,
                y: start.1.saturating_sub(config.display_text_margin as usize),
                x: start.0.saturating_add(config.display_text_margin as usize),
            })?;
            best_candidate.call_and_maybe_update(&DisplayTextLocationCandidateInner {
                alignment: Alignment::Center,
                reference_point: ReferencePoint::LeftTop,
                y: start.1.saturating_add(config.display_text_margin as usize),
                x: start.0.saturating_add(config.display_text_margin as usize),
            })?;
        } else {
            best_candidate.call_and_maybe_update(&DisplayTextLocationCandidateInner {
                alignment: Alignment::Center,
                reference_point: ReferencePoint::RightBottom,
                y: start.1.saturating_sub(config.display_text_margin as usize),
                x: start.0.saturating_sub(config.display_text_margin as usize),
            })?;
            best_candidate.call_and_maybe_update(&DisplayTextLocationCandidateInner {
                alignment: Alignment::Center,
                reference_point: ReferencePoint::RightTop,
                y: start.1.saturating_add(config.display_text_margin as usize),
                x: start.0.saturating_sub(config.display_text_margin as usize),
            })?;
        }

        let it = iterate_edge_points(
            start.0 as u32,
            end.0 as u32,
            config.display_text_edge_x_margin_to_ends,
            config.display_text_edge_x_step_size.try_into().unwrap(),
            config
                .display_text_edge_x_min_distance_to_center_or_else_collapse_into_center
                .try_into()
                .unwrap(),
        );
        for x in it {
            for candidate in [
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Center,
                    reference_point: ReferencePoint::CenterBottom,
                    y: start.1.saturating_sub(config.display_text_margin as usize),
                    x: x as usize,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Center,
                    reference_point: ReferencePoint::CenterTop,
                    y: start.1.saturating_add(config.display_text_margin as usize),
                    x: x as usize,
                },
            ] {
                best_candidate.call_and_maybe_update(&candidate)?;
            }
        }
    } else {
        panic!(
            "Data flows are the only edges which are allowed to be non-vertical, and they should not have labels atm in BPMD. Start: {start:?}, end: {end:?}"
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
        .chain(core::iter::once(mid_point))
        .chain(
            (0..steps)
                .rev()
                .map(move |i| (new_end - i * step_size) as u32),
        )
}

fn edge_corner_display_text_location_candidates(
    config: &Config,
    start: (usize, usize),
    middle: (usize, usize),
    end: (usize, usize),
    best_candidate: &mut CandidateTracker,
) -> ControlFlow<()> {
    use core::cmp::Ordering::{Equal, Greater, Less};
    let candidates_to_try = match (
        start.0.cmp(&middle.0),
        start.1.cmp(&middle.1),
        middle.0.cmp(&end.0),
        middle.1.cmp(&end.1),
    ) {
        (Equal, Greater, Greater, Equal) => {
            // up-left
            [
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Center,
                    reference_point: ReferencePoint::CenterBottom,
                    x: middle.0,
                    y: middle.1.saturating_sub(config.display_text_margin as usize),
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Left,
                    reference_point: ReferencePoint::LeftCenter,
                    x: middle.0 + config.display_text_margin as usize,
                    y: middle.1,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Left,
                    reference_point: ReferencePoint::LeftBottom,
                    x: middle.0 + config.display_text_margin as usize,
                    y: middle.1.saturating_sub(config.display_text_margin as usize),
                },
            ]
        }
        (Equal, Greater, Less, Equal) => {
            // up-right
            [
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Center,
                    reference_point: ReferencePoint::CenterBottom,
                    x: middle.0,
                    y: middle.1.saturating_sub(config.display_text_margin as usize),
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Right,
                    reference_point: ReferencePoint::RightCenter,
                    x: middle.0.saturating_sub(config.display_text_margin as usize),
                    y: middle.1,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Right,
                    reference_point: ReferencePoint::RightBottom,
                    x: middle.0.saturating_sub(config.display_text_margin as usize),
                    y: middle.1.saturating_sub(config.display_text_margin as usize),
                },
            ]
        }
        (Less, Equal, Equal, Greater) => {
            // right-up
            [
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Left,
                    reference_point: ReferencePoint::LeftCenter,
                    x: middle.0 + config.display_text_margin as usize,
                    y: middle.1,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Center,
                    reference_point: ReferencePoint::CenterTop,
                    x: middle.0,
                    y: middle.1 + config.display_text_margin as usize,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Left,
                    reference_point: ReferencePoint::LeftTop,
                    x: middle.0 + config.display_text_margin as usize,
                    y: middle.1 + config.display_text_margin as usize,
                },
            ]
        }
        (Less, Equal, Equal, Less) => {
            // right-down
            [
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Left,
                    reference_point: ReferencePoint::LeftCenter,
                    x: middle.0 + config.display_text_margin as usize,
                    y: middle.1,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Center,
                    reference_point: ReferencePoint::CenterBottom,
                    x: middle.0,
                    y: middle.1.saturating_sub(config.display_text_margin as usize),
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Left,
                    reference_point: ReferencePoint::LeftBottom,
                    x: middle.0 + config.display_text_margin as usize,
                    y: middle.1.saturating_sub(config.display_text_margin as usize),
                },
            ]
        }
        (Equal, Less, Greater, Equal) => {
            // down-right
            [
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Center,
                    reference_point: ReferencePoint::CenterTop,
                    x: middle.0,
                    y: middle.1 + config.display_text_margin as usize,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Right,
                    reference_point: ReferencePoint::RightCenter,
                    x: middle.0.saturating_sub(config.display_text_margin as usize),
                    y: middle.1,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Right,
                    reference_point: ReferencePoint::RightTop,
                    x: middle.0.saturating_sub(config.display_text_margin as usize),
                    y: middle.1 + config.display_text_margin as usize,
                },
            ]
        }
        (Equal, Less, Less, Equal) => {
            // down-left
            [
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Center,
                    reference_point: ReferencePoint::CenterTop,
                    x: middle.0,
                    y: middle.1 + config.display_text_margin as usize,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Left,
                    reference_point: ReferencePoint::LeftCenter,
                    x: middle.0 + config.display_text_margin as usize,
                    y: middle.1,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Left,
                    reference_point: ReferencePoint::LeftTop,
                    x: middle.0 + config.display_text_margin as usize,
                    y: middle.1 + config.display_text_margin as usize,
                },
            ]
        }
        (Greater, Equal, Equal, Less) => {
            // left-down
            [
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Right,
                    reference_point: ReferencePoint::RightCenter,
                    x: middle.0.saturating_sub(config.display_text_margin as usize),
                    y: middle.1,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Center,
                    reference_point: ReferencePoint::CenterBottom,
                    x: middle.0,
                    y: middle.1.saturating_sub(config.display_text_margin as usize),
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Right,
                    reference_point: ReferencePoint::RightBottom,
                    x: middle.0.saturating_sub(config.display_text_margin as usize),
                    y: middle.1.saturating_sub(config.display_text_margin as usize),
                },
            ]
        }
        (Greater, Equal, Equal, Greater) => {
            // left-up
            [
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Right,
                    reference_point: ReferencePoint::RightCenter,
                    x: middle.0.saturating_sub(config.display_text_margin as usize),
                    y: middle.1,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Center,
                    reference_point: ReferencePoint::CenterTop,
                    x: middle.0,
                    y: middle.1 + config.display_text_margin as usize,
                },
                DisplayTextLocationCandidateInner {
                    alignment: Alignment::Right,
                    reference_point: ReferencePoint::RightTop,
                    x: middle.0.saturating_sub(config.display_text_margin as usize),
                    y: middle.1 + config.display_text_margin as usize,
                },
            ]
        }
        _ => unreachable!("The edge has some fake-bendpoint? {start:?} {middle:?} {end:?}"),
    };

    for candidate in candidates_to_try {
        best_candidate.call_and_maybe_update(&candidate)?;
    }
    ControlFlow::Continue(())
}

enum ReferencePoint {
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

fn gateway_display_text_location_candidates(
    config: &Config,
    textbox_wh: (usize, usize),
    dim: Dimension,
    incoming_from: Side,
    callback: &DisplayLocationCallback,
) -> DisplayTextLocation {
    let full_margin = config.display_text_margin as usize;
    let fract_margin = ((full_margin * full_margin) / 2).isqrt();

    let mut best_candidate = CandidateTracker {
        score: u32::MAX,
        candidate: DisplayTextLocationCandidateInner {
            alignment: Alignment::Center,
            reference_point: ReferencePoint::Center,
            x: dim.x.saturating_add(dim.width / 2),
            y: dim.y.saturating_add(dim.height / 2),
        }
        .materialize(textbox_wh),
        textbox_wh,
        callback,
    };
    let top_left = DisplayTextLocationCandidateInner {
        alignment: Alignment::Right,
        reference_point: ReferencePoint::RightBottom,
        x: dim
            .x
            .saturating_add(dim.width / 4)
            .saturating_sub(fract_margin),
        y: dim
            .y
            .saturating_add(dim.height / 4)
            .saturating_sub(fract_margin),
    };
    let top = DisplayTextLocationCandidateInner {
        alignment: Alignment::Center,
        reference_point: ReferencePoint::CenterBottom,
        x: dim.x.saturating_add(dim.width / 2),
        y: dim.y.saturating_sub(full_margin),
    };
    let top_right = DisplayTextLocationCandidateInner {
        alignment: Alignment::Left,
        reference_point: ReferencePoint::LeftBottom,
        x: dim
            .x
            .saturating_add(dim.width)
            .saturating_sub(dim.width / 4)
            .saturating_add(fract_margin),
        y: dim
            .y
            .saturating_add(dim.height / 4)
            .saturating_sub(fract_margin),
    };
    let right = DisplayTextLocationCandidateInner {
        alignment: Alignment::Left,
        reference_point: ReferencePoint::LeftCenter,
        x: dim.x.saturating_add(dim.width).saturating_add(full_margin),
        y: dim.y.saturating_add(dim.height / 2),
    };
    let bottom_right = DisplayTextLocationCandidateInner {
        alignment: Alignment::Left,
        reference_point: ReferencePoint::LeftTop,
        x: dim
            .x
            .saturating_add(dim.width)
            .saturating_sub(dim.width / 4)
            .saturating_add(fract_margin),
        y: dim
            .y
            .saturating_add(dim.height)
            .saturating_sub(dim.height / 4)
            .saturating_add(fract_margin),
    };
    let bottom = DisplayTextLocationCandidateInner {
        alignment: Alignment::Center,
        reference_point: ReferencePoint::CenterTop,
        x: dim.x.saturating_add(dim.width / 2),
        y: dim.y.saturating_add(dim.height).saturating_add(full_margin),
    };
    let bottom_left = DisplayTextLocationCandidateInner {
        alignment: Alignment::Right,
        reference_point: ReferencePoint::RightTop,
        x: dim
            .x
            .saturating_add(dim.width / 4)
            .saturating_sub(fract_margin),
        y: dim
            .y
            .saturating_add(dim.height)
            .saturating_sub(dim.height / 4)
            .saturating_add(fract_margin),
    };
    let left = DisplayTextLocationCandidateInner {
        alignment: Alignment::Right,
        reference_point: ReferencePoint::RightCenter,
        x: dim.x.saturating_sub(full_margin),
        y: dim.y.saturating_add(dim.height / 2),
    };

    let candidates: [DisplayTextLocationCandidateInner; 8] = match incoming_from {
        Side::Left => [
            top_left,
            right,
            bottom_left,
            top,
            bottom,
            top_right,
            bottom_right,
            left,
        ],
        Side::Right => [
            top_right,
            left,
            bottom_right,
            top,
            bottom,
            top_left,
            bottom_left,
            right,
        ],
        Side::Top => [
            top_left,
            bottom,
            top_right,
            left,
            right,
            bottom_left,
            bottom_right,
            top,
        ],
        Side::Bottom => [
            bottom_left,
            top,
            bottom_right,
            left,
            right,
            top_left,
            top_right,
            bottom,
        ],
    };

    for candidate in candidates {
        if let ControlFlow::Break(()) = best_candidate.call_and_maybe_update(&candidate) {
            return best_candidate.candidate;
        }
    }

    best_candidate.candidate
}

fn event_or_data_display_text_location_candidates(
    config: &Config,
    corner_offset: usize,
    textbox_wh: (usize, usize),
    dim: Dimension,
    incoming_from: Side,
    callback: &DisplayLocationCallback,
) -> DisplayTextLocation {
    let full_margin = config.display_text_margin as usize;
    let fract_margin = ((full_margin * full_margin) / 2).isqrt();

    let mut best_candidate = CandidateTracker {
        score: u32::MAX,
        candidate: DisplayTextLocationCandidateInner {
            alignment: Alignment::Center,
            reference_point: ReferencePoint::Center,
            x: dim.x.saturating_add(dim.width / 2),
            y: dim.y.saturating_add(dim.height / 2),
        }
        .materialize(textbox_wh),
        textbox_wh,
        callback,
    };
    let top_left = DisplayTextLocationCandidateInner {
        alignment: Alignment::Right,
        reference_point: ReferencePoint::RightBottom,
        x: dim
            .x
            .saturating_add(corner_offset / 4)
            .saturating_sub(fract_margin),
        y: dim
            .y
            .saturating_add(corner_offset / 4)
            .saturating_sub(fract_margin),
    };
    let top = DisplayTextLocationCandidateInner {
        alignment: Alignment::Center,
        reference_point: ReferencePoint::CenterBottom,
        x: dim.x.saturating_add(dim.width / 2),
        y: dim.y.saturating_sub(full_margin),
    };
    let top_right = DisplayTextLocationCandidateInner {
        alignment: Alignment::Left,
        reference_point: ReferencePoint::LeftBottom,
        x: dim
            .x
            .saturating_add(dim.width)
            .saturating_sub(corner_offset / 4)
            .saturating_add(fract_margin),
        y: dim
            .y
            .saturating_add(corner_offset / 4)
            .saturating_sub(fract_margin),
    };
    let right = DisplayTextLocationCandidateInner {
        alignment: Alignment::Left,
        reference_point: ReferencePoint::LeftCenter,
        x: dim.x.saturating_add(dim.width).saturating_add(full_margin),
        y: dim.y.saturating_add(dim.height / 2),
    };
    let bottom_right = DisplayTextLocationCandidateInner {
        alignment: Alignment::Left,
        reference_point: ReferencePoint::LeftTop,
        x: dim
            .x
            .saturating_add(dim.width)
            .saturating_sub(corner_offset / 4)
            .saturating_add(fract_margin),
        y: dim
            .y
            .saturating_add(dim.height)
            .saturating_sub(corner_offset / 4)
            .saturating_add(fract_margin),
    };
    let bottom = DisplayTextLocationCandidateInner {
        alignment: Alignment::Center,
        reference_point: ReferencePoint::CenterTop,
        x: dim.x.saturating_add(dim.width / 2),
        y: dim.y.saturating_add(dim.height).saturating_add(full_margin),
    };
    let bottom_left = DisplayTextLocationCandidateInner {
        alignment: Alignment::Right,
        reference_point: ReferencePoint::RightTop,
        x: dim
            .x
            .saturating_add(corner_offset / 4)
            .saturating_sub(fract_margin),
        y: dim
            .y
            .saturating_add(dim.height)
            .saturating_sub(corner_offset / 4)
            .saturating_add(fract_margin),
    };
    let left = DisplayTextLocationCandidateInner {
        alignment: Alignment::Right,
        reference_point: ReferencePoint::RightCenter,
        x: dim.x.saturating_sub(full_margin),
        y: dim.y.saturating_add(dim.height / 2),
    };

    let candidates: [DisplayTextLocationCandidateInner; 8] = match incoming_from {
        Side::Left => [
            bottom,
            top,
            right,
            top_left,
            bottom_left,
            top_right,
            bottom_right,
            left,
        ],
        Side::Right => [
            bottom,
            top,
            left,
            top_right,
            bottom_right,
            top_left,
            bottom_left,
            right,
        ],
        Side::Top => [
            bottom,
            left,
            right,
            top_left,
            top_right,
            bottom_left,
            bottom_right,
            top,
        ],
        Side::Bottom => [
            top,
            left,
            right,
            bottom_left,
            bottom_right,
            top_left,
            top_right,
            bottom,
        ],
    };

    for candidate in candidates {
        if let ControlFlow::Break(()) = best_candidate.call_and_maybe_update(&candidate) {
            return best_candidate.candidate;
        }
    }

    best_candidate.candidate
}

fn event_display_text_location_candidates(
    config: &Config,
    textbox_wh: (usize, usize),
    dim: Dimension,
    incoming_from: Side,
    callback: &DisplayLocationCallback,
) -> DisplayTextLocation {
    event_or_data_display_text_location_candidates(
        config,
        5,
        textbox_wh,
        dim,
        incoming_from,
        callback,
    )
}

fn data_display_text_location_candidates(
    config: &Config,
    textbox_wh: (usize, usize),
    dim: Dimension,
    incoming_from: Side,
    callback: &DisplayLocationCallback,
) -> DisplayTextLocation {
    event_or_data_display_text_location_candidates(
        config,
        0,
        textbox_wh,
        dim,
        incoming_from,
        callback,
    )
}

fn activity_display_text_location_candidates(
    textbox_wh: (usize, usize),
    dim: Dimension,
) -> DisplayTextLocation {
    DisplayTextLocationCandidateInner {
        alignment: Alignment::Center,
        reference_point: ReferencePoint::Center,
        x: dim.x.saturating_add(dim.width / 2),
        y: dim.y.saturating_add(dim.height / 2),
    }
    .materialize(textbox_wh)
}

fn prepare_collision_grid(graph: &Graph) -> Grid {
    let mut grid = Grid::new(graph.total_width_height());

    for edge in &graph.edges {
        let EdgeType::Regular {
            bend_points: RegularEdgeBendPoints::FullyRouted(bend_points),
            ..
        } = &edge.edge_type
        else {
            unreachable!("Only regular edges at this point, {edge:?}");
        };
        let weight = match edge.flow_type {
            FlowType::MessageFlow(..) => 8,
            FlowType::DataFlow(..) => 2,
            FlowType::SequenceFlow => 10,
        };
        for [start, end] in bend_points.array_windows() {
            grid.insert(&Line::new(start, end), weight);
        }
    }

    for node in &graph.nodes {
        let node_weight = 10;
        if node.is_gateway() {
            #[rustfmt::skip]
            let (top, right, bottom, left)   = (
                (node.x + node.width / 2, node.y                  ),
                (node.x + node.width    , node.y + node.height / 2),
                (node.x + node.width / 2, node.y + node.height    ),
                (node.x                 , node.y + node.height / 2)
             );

            grid.insert_quadrangle(top, right, bottom, left, node_weight);
        } else {
            let tl = (node.x, node.y);
            let tr = (node.x + node.width, node.y);
            let br = (node.x + node.width, node.y + node.height);
            let bl = (node.x, node.y + node.height);

            grid.insert_quadrangle(tl, tr, br, bl, node_weight);
        }
    }

    for pool in &graph.pools {
        for lane in &pool.lanes {
            let lane_weight = 9;
            let tl = (lane.x, lane.y);
            let tr = (lane.x + lane.width, lane.y);
            let br = (lane.x + lane.width, lane.y + lane.height);
            let bl = (lane.x, lane.y + lane.height);

            grid.insert_quadrangle(tl, tr, br, bl, lane_weight);
        }
    }

    grid
}

fn prep(cache: &mut FontCache, display_text: &mut DisplayText) -> (usize, usize) {
    let metrics = Metrics::new(display_text.font_size, display_text.line_height);

    let mut buffer = Buffer::new(&mut cache.font_system, metrics);

    buffer.set_wrap(Wrap::Word);
    buffer.set_size(display_text.max_width.map(|width| width as f32), None);
    buffer.set_text(
        // Don't escape just yet. We want to first inspect the text that will be visible.
        &display_text.raw_text,
        &Attrs::new().family(cosmic_text::Family::Name(&display_text.font_family)),
        Shaping::Advanced,
        // Can only have Center here, since we don't know where it will be finally positioned at.
        Some(Align::Center),
    );

    // Perform shaping as desired
    buffer.shape_until_scroll(&mut cache.font_system, false /* not sure? */);
    let count = buffer.layout_runs().count().max(1); // Always have at least one.
    let height = count as f32 * display_text.line_height;
    let width = buffer.layout_runs().fold(0.0, |state, line| {
        if state < line.line_w {
            line.line_w
        } else {
            state
        }
    });

    display_text.buffer = buffer;

    (width.ceil() as usize, height.ceil() as usize)
}

// Taking arguments very peace meal to circumvent the borrow checker.
fn side_of_first_incoming_flow(
    (width, height, incoming, incoming_ports, edges): (
        usize,
        usize,
        &[EdgeId],
        &[RelativePort],
        &[Edge],
    ),
    edge_type: fn(&Edge) -> bool,
) -> Side {
    let Some((port, _)) = incoming_ports
        .iter()
        .zip(incoming.iter())
        .find(|&(_, e)| edge_type(&edges[*e]))
    else {
        return Side::Left;
    };
    if port.x == 0 {
        Side::Left
    } else if port.x == width {
        Side::Right
    } else if port.y == 0 {
        Side::Top
    } else {
        assert!(port.y == height);
        Side::Bottom
    }
}

fn text_into_grid(
    location: &DisplayTextLocation,
    (width, height): (usize, usize),
    grid: &mut Grid,
) {
    let x = location.x;
    let y = location.y;
    grid.insert_quadrangle(
        (x, y),
        (x + width, y),
        (x + width, y + height),
        (x, y + height),
        10,
    );
}

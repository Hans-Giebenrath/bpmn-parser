use std::{num::NonZero, ops::ControlFlow};

use crate::common::{
    config::Config,
    node::{Dimension, Side},
};

struct DisplayTextLocationCandidateInner {
    pub alignment: Alignment,
    pub reference_point: ReferencePoint,
    pub x: usize,
    pub y: usize,
}

impl DisplayTextLocationCandidateInner {
    fn materialize(&self, (width, height): (usize, usize)) -> DisplayTextLocationCandidate {
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

        DisplayTextLocationCandidate {
            alignment: self.alignment,
            x,
            y,
        }
    }
}

#[derive(Debug)]
pub struct DisplayTextLocationCandidate {
    pub alignment: Alignment,
    pub x: usize,
    pub y: usize,
}

struct CandidateTracker<'a> {
    score: u32,
    candidate: DisplayTextLocationCandidate,
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
type DisplayLocationCallback<'a> = dyn Fn(&DisplayTextLocationCandidate) -> u32 + 'a;

pub fn edge_display_text_location_candidates(
    config: &Config,
    textbox_wh: (usize, usize),
    line_points: &[(usize, usize)],
    // A gen fn would be cooler, but then again I maybe need to rotate the start
    callback: &DisplayLocationCallback,
) -> DisplayTextLocationCandidate {
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
        .chain(std::iter::once(mid_point))
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
    use std::cmp::Ordering::{Equal, Greater, Less};
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

#[derive(Debug, Clone, Copy)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

pub fn gateway_display_text_location_candidates(
    config: &Config,
    textbox_wh: (usize, usize),
    dim: Dimension,
    incoming_from: Side,
    callback: &DisplayLocationCallback,
) -> DisplayTextLocationCandidate {
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
) -> DisplayTextLocationCandidate {
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

pub fn event_display_text_location_candidates(
    config: &Config,
    textbox_wh: (usize, usize),
    dim: Dimension,
    incoming_from: Side,
    callback: &DisplayLocationCallback,
) -> DisplayTextLocationCandidate {
    event_or_data_display_text_location_candidates(
        config,
        5,
        textbox_wh,
        dim,
        incoming_from,
        callback,
    )
}

pub fn data_display_text_location_candidates(
    config: &Config,
    textbox_wh: (usize, usize),
    dim: Dimension,
    incoming_from: Side,
    callback: &DisplayLocationCallback,
) -> DisplayTextLocationCandidate {
    event_or_data_display_text_location_candidates(
        config,
        0,
        textbox_wh,
        dim,
        incoming_from,
        callback,
    )
}

pub fn activity_display_text_location_candidates(
    textbox_wh: (usize, usize),
    dim: Dimension,
) -> DisplayTextLocationCandidate {
    DisplayTextLocationCandidateInner {
        alignment: Alignment::Center,
        reference_point: ReferencePoint::Center,
        x: dim.x.saturating_add(dim.width / 2),
        y: dim.y.saturating_add(dim.height / 2),
    }
    .materialize(textbox_wh)
}

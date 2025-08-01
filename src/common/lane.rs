//lane.rs
use crate::common::graph::LaneId;
use crate::common::graph::NodeId;
use crate::common::macros::impl_index;

#[derive(Default)]
pub struct Lane {
    /// TODO Not sure yet if a None could also just be an empty String.
    pub name: Option<String>,
    /// These will be sorted by layers after the layer assignment is solved.
    pub nodes: Vec<NodeId>,
    /// Assigned in xy_ilp phase.
    pub x: usize,
    /// Assigned in xy_ilp phase.
    pub y: usize,
    /// Assigned in xy_ilp phase.
    pub width: usize,
    /// Assigned in xy_ilp phase.
    pub height: usize,
}

impl Lane {
    pub fn new(lane: Option<String>) -> Self {
        Lane {
            name: lane,
            ..Default::default()
        }
    }
}

impl_index!(LaneId, Lane, lane_idx);

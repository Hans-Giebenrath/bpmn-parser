//lane.rs
use crate::TokenCoordinate;
use crate::graph::LaneId;
use crate::graph::NodeId;
use crate::macros::impl_index;

#[derive(Default)]
pub struct Lane {
    /// `None` is for the anonymous lane. See invariant at `Pool::name`.
    pub name: Option<String>,
    /// These will be sorted by layers after the layer assignment is solved.
    pub nodes: Vec<NodeId>,
    /// If `name.is_none()`, then this is the tc of the statement which implicitly created the
    /// anonymous lane.
    pub tc: TokenCoordinate,
    /// Assigned in xy_ilp phase.
    pub x: usize,
    /// Assigned in xy_ilp phase.
    pub y: usize,
    /// Assigned in xy_ilp phase.
    pub width: usize,
    /// Assigned in xy_ilp phase.
    pub height: usize,

    pub stroke_color: Option<String>,
    pub fill_color: Option<String>,
}

impl Lane {
    pub fn new(lane: Option<String>, tc: TokenCoordinate) -> Self {
        Lane {
            name: lane,
            tc,
            ..Default::default()
        }
    }
}

impl_index!(LaneId, Lane);

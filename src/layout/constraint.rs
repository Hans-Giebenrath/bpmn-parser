use crate::common::graph::EdgeId;
use crate::common::graph::NodeId;

#[derive(Default)]
pub struct LayoutConstraints {
    pub left_of: Vec<LeftOf>,
    pub above: Vec<Above>,
    pub same_layer: Vec<SameLayer>,
    /// Note: These are _only_ the user provided back edge constraints. The results of the back edge
    /// detection phase are stored somewhere else.
    pub back_edge: Vec<BackEdge>,
}

pub struct LeftOf {
    pub left: NodeId,
    pub right: NodeId,
}

pub struct Above {
    pub above: NodeId,
    pub below: NodeId,
}
pub struct SameLayer(pub NodeId, pub NodeId);
/// This tells the layer organization ILP to not treat this edge as a forward-facing edge, i.e.
/// it does not move into the list of constraints.
pub struct BackEdge(pub EdgeId);

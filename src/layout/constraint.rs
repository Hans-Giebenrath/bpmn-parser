use crate::common::graph::EdgeId;
use crate::common::graph::NodeId;

#[derive(Default)]
pub struct LayoutConstraints {
    left_of: Vec<LeftOf>,
    above: Vec<Above>,
    same_layer: Vec<SameLayer>,
    back_edge: Vec<BackEdge>,
}

pub struct LeftOf {
    left: NodeId,
    right: NodeId,
}

pub struct Above {
    above: NodeId,
    below: NodeId,
}
pub struct SameLayer(NodeId, NodeId);
/// This tells the layer organization ILP to not treat this edge as a forward-facing edge, i.e.
/// it does not move into the list of constraints.
pub struct BackEdge(EdgeId);

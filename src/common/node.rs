// node.rs

use super::macros::impl_index;
use crate::common::bpmn_node::*;
use crate::common::graph::Coord3;
use crate::common::graph::EdgeId;
use crate::common::graph::LaneId;
use crate::common::graph::NodeId;
use crate::common::graph::PoolAndLane;
use crate::common::graph::PoolId;
use crate::common::graph::SdeId;
use crate::layout::all_crossing_minimization::CrossingMinimizationNodeData;
use crate::layout::solve_layer_assignment::LayerAssignmentData;
use crate::layout::xy_ilp::XyIlpNodeData;
use crate::lexer::DataAux;
use crate::lexer::PeBpmnProtection;
use crate::lexer::TokenCoordinate;
use std::ops::Add;

// TODO this needs to move into a global configuration struct.
pub const LAYER_WIDTH: f64 = 80.0;
const AVAILABLE_SPACE: f64 = LAYER_WIDTH - 20.0;

#[derive(Debug)]
pub enum NodeType {
    RealNode {
        /// Is data node -> DataStoreReference or DataObjectReference
        event: BpmnNode,
        display_text: String,
        /// The node, but also a sequence flow jump or landing associated with this node.
        tc: TokenCoordinate,
        transported_data: Vec<SdeId>,
        pe_bpmn_hides_protection_operations: bool,
    },
    // Dummy nodes are inserted in three stages:
    //  1. After the layer assignment phase, to break long edges into uniformly short edges (those
    //     which would otherwise span multiple layers)
    //     These are reversed/undone in the last dummy node replacement phase.
    //  2. During all crossing optimization when there are edges in the same layer, then they are
    //     split into two with a dummy node in the neighboring layer (or actually on both
    //     neighboring layers).
    //     These are reversed/undone in the same crossing optimization phase.
    //  3. During port assignment, to handle those edges which leave above or below a node and then
    //     bend to the right. The bendpoint is represented by a new dummy node.
    DummyNode,
    // Bend dummies are inserted during the port assignment phase. They help to create vertical edge
    // segments for edges that leave at the top or bottom side of a node.
    BendDummy {
        originating_node: NodeId,
        kind: BendDummyKind,
    },
}

/// This is primarily used to make some Y-ILP stuff easier.
/// Check whether it makes sense to inline this, i.e. make more BendDummy variants on NodeType.
#[derive(Debug)]
pub(crate) enum BendDummyKind {
    /// The target node is in another lane but the bend dummy stayed *in the same* lane as the
    /// gateway node due to a blocker.
    FromGatewayBlockedLaneCrossing {
        gateway_node: NodeId,
        target_node: NodeId,
    },
    /// The target node is in another lane and the bend dummy could move out of the lane of the
    /// originating gateway node, but *not necessarily* resides in the target lane. If semantics
    /// requires, this might be split up to better differentiate.
    FromGatewayFreeLaneCrossing {
        gateway_node: NodeId,
        target_node: NodeId,
    },
    /// The target node is in the same lane as the gateway node, so no lane crossing is required at
    /// all.
    FromGatewayToSameLane {
        gateway_node: NodeId,
        target_node: NodeId,
    },
    /// For boundary events we don't have such finicky details at all.
    FromBoundaryEvent,
}

//struct BendDummyCrossLaneTarget {
//    cross_lane_edge: EdgeId,
//    gateway_edge:
//}

#[derive(Debug)]
pub enum NodePhaseAuxData {
    None,
    LayerAssignmentData(LayerAssignmentData),
    CrossingMinimizationNodeData(CrossingMinimizationNodeData),
    XyIlpNodeData(XyIlpNodeData),
}

#[derive(PartialEq, PartialOrd, Ord, Eq, Debug, Clone, Copy, Hash)]
pub struct LayerId(pub usize);

#[derive(Debug)]
pub struct Node {
    pub id: NodeId,
    pub node_type: NodeType,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub stroke_color: Option<String>,
    pub fill_color: Option<String>,
    pub pool: PoolId,
    pub lane: LaneId,

    /// Assigned in layer assignment phase.
    pub layer_id: LayerId,
    /// Set during layer assignment, and then later used to shift the `x` value (if no obstacles are
    /// in the way).
    pub uses_half_layer: bool,

    // This creates a kind of doubly linked list. Created during crossing minimisation phase,
    // and then used in the y-ILP.
    pub node_above_in_same_lane: Option<NodeId>,
    pub node_below_in_same_lane: Option<NodeId>,

    // Sequence flow edges, message edges and data object edges are all treated equally.
    // Note: This might benefit from going
    //   from: Vec<EdgeId>
    //   to:   Vec<struct{EdgeId, FlowType, stays_within_lane}>
    /// Invariant: After global crossing minimization, this is sorted.
    pub incoming: Vec<EdgeId>,
    pub outgoing: Vec<EdgeId>,

    pub incoming_ports: Vec<RelativePort>,
    pub outgoing_ports: Vec<RelativePort>,

    pub aux: NodePhaseAuxData,
}

#[derive(Debug)]
pub struct XY {
    pub x: usize,
    pub y: usize,
}

#[derive(Debug)]
// Relative to the node's size.
pub struct RelativePort {
    pub x: usize,
    pub y: usize,
}

#[derive(Debug)]
// Absolute position of the port within the diagram.
pub struct AbsolutePort {
    pub x: usize,
    pub y: usize,
}

impl Add<XY> for &RelativePort {
    type Output = AbsolutePort;

    fn add(self, rhs: XY) -> Self::Output {
        AbsolutePort {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

pub(crate) enum LoneDataElement {
    Nope,
    IsInput(EdgeId),
    IsOutput(EdgeId),
}

impl Node {
    pub fn is_data(&self) -> bool {
        matches!(
            self.node_type,
            NodeType::RealNode {
                event: BpmnNode::Data(_, _),
                ..
            }
        )
    }

    pub fn get_data_aux(&self) -> Option<&DataAux> {
        if let NodeType::RealNode {
            event: BpmnNode::Data(_, data_aux),
            ..
        } = &self.node_type
        {
            Some(data_aux)
        } else {
            None
        }
    }

    pub fn set_pebpmn_protection(&mut self, protection: PeBpmnProtection) {
        if let NodeType::RealNode {
            event: BpmnNode::Data(_, data_aux),
            ..
        } = &mut self.node_type
        {
            data_aux.pebpmn_protection.push(protection);
        } else {
            return;
        }
    }

    pub fn get_node_transported_data(&self) -> &[SdeId] {
        if let NodeType::RealNode {
            transported_data, ..
        } = &self.node_type
        {
            transported_data
        } else {
            &[]
        }
    }

    pub fn add_node_transported_data(&mut self, data: &[SdeId]) {
        if let NodeType::RealNode {
            transported_data, ..
        } = &mut self.node_type
        {
            transported_data.extend_from_slice(data);
        }
    }

    /// This type of nodes is treated specially:
    /// It will always be placed directly next to the sole task it is connected to,
    /// i.e. directly above or below. This means we don't need helper nodes to model
    /// loop edges (edges starting and ending in the same layer). This does not mean
    /// that there won't be loop edges in the end: When there are three or more such
    /// special data nodes connected to the same node, the more distant ones cannot
    /// have direct connections and still need self loops.
    /// TODO the name must become better in the future when there are left-right constraints.
    /// Maybe is_data_with_only_one_edge_directly_next_to_connected_node
    /// because there might be more than two such nodes where it doesn't make sense
    /// (putting them in the left or right half layer would be better in this case),
    /// or a left-right constraint dictates to not place them in the same layer.
    /// `shall_be_placed_next_to_only_connected_node = No, Above, Below` as a property,
    /// to be determined in a preprocessing phase before layer assignment.
    pub fn data_with_only_one_edge(&self) -> LoneDataElement {
        match (self.is_data(), &self.incoming[..], &self.outgoing[..]) {
            (true, &[edge_id], &[]) => LoneDataElement::IsOutput(edge_id),
            (true, &[], &[edge_id]) => LoneDataElement::IsInput(edge_id),
            _ => LoneDataElement::Nope,
        }
    }

    pub fn is_data_with_only_one_edge(&self) -> bool {
        !matches!(self.data_with_only_one_edge(), LoneDataElement::Nope)
    }

    pub fn is_dummy(&self) -> bool {
        matches!(
            self.node_type,
            NodeType::DummyNode | NodeType::BendDummy { .. }
        )
    }

    pub fn is_from_gateway_to_same_lane(&self) -> Option<NodeId> {
        if let NodeType::BendDummy {
            kind: BendDummyKind::FromGatewayToSameLane { target_node, .. },
            ..
        } = &self.node_type
        {
            Some(*target_node)
        } else {
            None
        }
    }

    pub fn is_gateway(&self) -> bool {
        matches!(
            self.node_type,
            NodeType::RealNode {
                event: BpmnNode::Gateway(_),
                ..
            }
        )
    }

    pub fn is_some_sequence_flow_box(&self) -> bool {
        !self.is_data() && !self.is_dummy()
    }

    pub fn size(&self) -> (usize, usize) {
        return (self.width, self.height);
    }

    pub fn port_of_incoming(&self, edge_id: EdgeId) -> Option<AbsolutePort> {
        self.incoming
            .iter()
            .cloned()
            .zip(self.incoming_ports.iter())
            .find(|(inner_edge_id, _)| *inner_edge_id == edge_id)
            .map(|(_, port)| port + self.xy())
    }

    pub fn port_of_outgoing(&self, edge_id: EdgeId) -> Option<AbsolutePort> {
        self.outgoing
            .iter()
            .cloned()
            .zip(self.outgoing_ports.iter())
            .find(|(inner_edge_id, _)| *inner_edge_id == edge_id)
            .map(|(_, port)| port + self.xy())
    }

    pub fn display_text(&self) -> Option<&str> {
        if let NodeType::RealNode { display_text, .. } = &self.node_type {
            Some(display_text.as_str())
        } else {
            return None;
        }
    }

    pub fn pool_and_lane(&self) -> PoolAndLane {
        PoolAndLane {
            pool: self.pool,
            lane: self.lane,
        }
    }

    pub fn coord3(&self) -> Coord3 {
        Coord3 {
            pool_and_lane: self.pool_and_lane(),
            layer: self.layer_id,
            half_layer: self.uses_half_layer,
        }
    }

    pub fn xy(&self) -> XY {
        XY {
            x: self.x,
            y: self.y,
        }
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Node {{ id: {}, x: {:?}, y: {:?}, event: {:?}, pool: {:?}, lane: {:?} }}",
            self.id, self.x, self.y, self.node_type, self.pool, self.lane
        )
    }
}

impl_index!(NodeId, Node, node_idx);

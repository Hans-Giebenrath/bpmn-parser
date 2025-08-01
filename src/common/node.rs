// node.rs

use super::macros::impl_index;
use crate::common::bpmn_node::*;
use crate::common::graph::EdgeId;
use crate::common::graph::LaneId;
use crate::common::graph::NodeId;
use crate::common::graph::PoolAndLane;
use crate::common::graph::PoolId;
use crate::common::graph::SdeId;
use crate::layout::all_crossing_minimization::CrossingMinimizationNodeData;
use crate::layout::edge_routing::EdgeRoutingNodeData;
use crate::layout::xy_ilp::XyIlpNodeData;
use crate::lexer::DataAux;
use crate::lexer::PeBpmnProtection;
use crate::lexer::TokenCoordinate;

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
    DummyNode,
}

#[derive(Debug)]
pub enum NodePhaseAuxData {
    None,
    CrossingMinimizationNodeData(CrossingMinimizationNodeData),
    XyIlpNodeData(XyIlpNodeData),
    EdgeRoutingNodeData(EdgeRoutingNodeData),
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
    /// TODO did I implement this?
    pub incoming: Vec<EdgeId>,
    pub outgoing: Vec<EdgeId>,

    pub aux: NodePhaseAuxData,
}

pub struct XY {
    pub x: usize,
    pub y: usize,
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
    pub fn is_data_with_only_one_edge(&self) -> bool {
        self.is_data() && (self.incoming.len() + self.outgoing.len() == 1)
    }

    pub fn is_dummy(&self) -> bool {
        matches!(self.node_type, NodeType::DummyNode)
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

    /// Nodes have just one port where their edges are leaving (and one for entering).
    /// TODO handle gateways differently.
    pub fn left_port(&self) -> XY {
        XY {
            x: self.x,
            y: self.y + self.height / 2,
        }
    }

    /// Nodes have just one port where their edges are leaving (and one for entering).
    /// TODO handle gateways differently.
    pub fn right_port(&self) -> XY {
        XY {
            x: self.x + self.width,
            y: self.y + self.height / 2,
        }
    }

    //    pub fn get_mid_point(
    //        index: &usize,
    //        size: &usize,
    //        from_node: &Node,
    //        to_node: &Node,
    //    ) -> Option<[XY; 2]> {
    //        let XY {
    //            x: from_x,
    //            y: from_y,
    //        } = from_node.right_port();
    //        let XY { x: _, y: to_y } = to_node.left_port();
    //        if from_y == to_y {
    //            return None;
    //        }
    //
    //        let gap = AVAILABLE_SPACE / *size as f64;
    //        let x_start = from_x as f64 + (crate::layout::xy_ilp::LAYER_WIDTH - AVAILABLE_SPACE) / 2.0;
    //        let mid_x = x_start + (*index as f64 * gap);
    //        Some([
    //            XY {
    //                x: (mid_x) as usize,
    //                y: (from_y) as usize,
    //            },
    //            XY {
    //                x: (mid_x) as usize,
    //                y: (to_y) as usize,
    //            },
    //        ])
    //    }

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

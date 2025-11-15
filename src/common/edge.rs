use crate::common::bpmn_node::BoundaryEvent;
use crate::common::graph::{EdgeId, NodeId, SdeId};
use crate::common::macros::impl_index;
use crate::lexer::PeBpmnProtection;

/// TODO better name.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::enum_variant_names)]
pub enum FlowType {
    MessageFlow(MessageFlowAux),
    DataFlow(DataFlowAux),
    SequenceFlow,
}

#[derive(Default, Debug, Clone, PartialEq)]
pub struct MessageFlowAux {
    pub transported_data: Vec<SdeId>,
    pub pebpmn_protection: Vec<(SdeId, Vec<PeBpmnProtection>)>,
}

#[derive(Default, Debug, Clone, PartialEq)]
pub struct DataFlowAux {
    pub transported_data: Vec<SdeId>,
}

#[derive(Debug, Clone)]
pub enum EdgeType {
    Regular {
        text: Option<String>,
        /// TODO ToBeDeterminedOrStraight is confusing, better make this Option<...> and just leave
        /// "Straight"?
        bend_points: RegularEdgeBendPoints,
    },
    // If a ReplacedByDummies node is assigned its final bend_points, e.g. when a straight edge can
    // be drawn, then it is automatically converted back into a `Regular` edge.
    ReplacedByDummies {
        /// The EdgeId is the Id of the first DummyEdge which replaces this edge. Note that the
        /// dummy edges, which make up this original edge, are not consecutive within `graph.nodes`.
        /// Instead, one must move through the incoming/outgoing edges of the connected nodes, i.e.
        /// more through the graph.
        first_dummy_edge: EdgeId,
        text: Option<String>,
    },
    DummyEdge {
        /// All the dummy edges which replace the same original edge share the same value.
        original_edge: EdgeId,
        /// TODO ToBeDeterminedOrStraight is confusing, better make this Option<...> and just leave
        /// "Straight"?
        bend_points: DummyEdgeBendPoints,
    },
}

/// TODO split this in two: RegularEdgeBendPoints, and a DummyEdgeBendPoints without FullyRouted.
#[derive(Debug, Clone)]
pub enum RegularEdgeBendPoints {
    ToBeDetermined,
    SegmentEndpoints((usize, usize), (usize, usize)),
    FullyRouted(Vec<(usize, usize)>),
}

#[derive(Debug, Clone)]
pub enum DummyEdgeBendPoints {
    ToBeDeterminedOrStraight,
    SegmentEndpoints((usize, usize), (usize, usize)),
    VerticalBendDummy((usize, usize)),
    /// Vertical lines from a gateway which happened to leave directly to the right side instead of
    /// from the top or bottom edge.
    VerticalCollapsed,
}

/// `Clone` is required for the all_crossing_minimization self-layer edge replacement with dummy
/// nodes.
#[derive(Debug, Clone)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub edge_type: EdgeType,
    pub is_reversed: bool,
    pub flow_type: FlowType,
    pub stays_within_lane: bool,
    /// If `! is_reversed` then attached via `from`, otherwise via `to`.
    /// TODO did I keep in mind to copy this property to replacement dummy nodes?
    pub attached_to_boundary_event: Option<BoundaryEvent>,

    // For vertical edges or edge segments. E.g. the part which leaves a real node and then goes
    // into a bend dummy. Or for message flows or sequence flows which go straight up or down
    // without having any obstacle (non-dummy) in the way.
    // (Explicitly not meant for loop edges! They first go to the side, then up/down, and then back
    // into the same layer. They are handled as regular sequence flows.)
    // This information is used in the edge routing phase of direct edges. It could be derived from
    // scratch again, but this is computationally rather complex. Since this is already computed
    // once, the information is just stored as a bool.
    pub is_vertical: bool,

    pub stroke_color: Option<String>,
    //pub bend_points: Option<Vec<(usize, usize)>>,
    //pub edge_has_direct_connection: bool,
}

impl Edge {
    pub fn is_replaced_by_dummies(&self) -> bool {
        matches!(self.edge_type, EdgeType::ReplacedByDummies { .. })
    }

    pub fn is_dummy(&self) -> bool {
        matches!(self.edge_type, EdgeType::DummyEdge { .. })
    }

    pub fn is_regular(&self) -> bool {
        matches!(self.edge_type, EdgeType::Regular { .. })
    }

    pub fn is_sequence_flow(&self) -> bool {
        matches!(self.flow_type, FlowType::SequenceFlow)
    }

    pub fn is_message_flow(&self) -> bool {
        matches!(self.flow_type, FlowType::MessageFlow(_))
    }

    pub fn is_data_flow(&self) -> bool {
        matches!(self.flow_type, FlowType::DataFlow(_))
    }

    pub fn get_transported_data(&self) -> &[SdeId] {
        if let FlowType::DataFlow(aux) = &self.flow_type {
            &aux.transported_data
        } else if let FlowType::MessageFlow(aux) = &self.flow_type {
            &aux.transported_data
        } else {
            &[]
        }
    }

    pub fn set_pebpmn_protection(&mut self, sde: &SdeId, protection: PeBpmnProtection) {
        if let FlowType::MessageFlow(MessageFlowAux {
            pebpmn_protection, ..
        }) = &mut self.flow_type
        {
            if let Some((_, protections)) = pebpmn_protection.iter_mut().find(|(id, _)| id == sde) {
                protections.push(protection);
            } else {
                pebpmn_protection.push((*sde, vec![protection]));
            }
        }
    }

    pub fn text(&self) -> Option<&str> {
        match &self.edge_type {
            EdgeType::Regular { text, .. } => text.as_deref(),
            EdgeType::ReplacedByDummies { text, .. } => text.as_deref(),
            _ => None,
        }
    }
}

impl_index!(EdgeId, Edge, edge_idx);

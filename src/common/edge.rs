use crate::common::graph::{EdgeId, NodeId, SdeId};
use crate::common::macros::impl_index;
use crate::lexer::PeBpmnProtection;

/// TODO better name.
#[derive(Debug, Clone, PartialEq)]
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
        bend_points: RegularEdgeBendPoints,
    },
    // If a ReplacedByDummies node is assigned its final bend_points, e.g. when a straight edge can
    // be drawn, then it is automatically converted back into a `Regular` edge.
    ReplacedByDummies {
        /// The EdgeId is the Id of the first DummyEdge which replaces this edge.
        /// When an edge is replaced by dummies, then the dummy nodes are added all in a row,
        /// ordered from `from` to `to` (ignoring `is_reversed`).
        /// To calculate how many there are one can just check from this original edge the distance
        /// between the `from` to `to` node layers, or just iterate over the edges from the given
        /// `EdgeId` until the `original_edge` in the dummy nodes is no longer the current edge's
        /// EdgeId.
        first_dummy_edge: EdgeId,
        text: Option<String>,
    },
    DummyEdge {
        /// All the dummy edges which replace the same original edge share the same value.
        original_edge: EdgeId,
        bend_points: DummyEdgeBendPoints,
    },
}

/// TODO split this in two: RegularEdgeBendPoints, and a DummyEdgeBendPoints without FullyRouted.
#[derive(Debug, Clone)]
pub enum RegularEdgeBendPoints {
    ToBeDeterminedOrStraight,
    SegmentEndpoints((usize, usize), (usize, usize)),
    FullyRouted(Vec<(usize, usize)>),
}

#[derive(Debug, Clone)]
pub enum DummyEdgeBendPoints {
    ToBeDeterminedOrStraight,
    SegmentEndpoints((usize, usize), (usize, usize)),
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
                pebpmn_protection.push((sde.clone(), vec![protection]));
            }
        }
    }

    // pub fn with_default_text(from: usize, to: usize) -> Self {
    //     Edge {
    //         from,
    //         to,
    //         text: None,
    //         bend_points: vec![], // Alguses tühjad painutuspunktid
    //     }
    // }

    pub fn text(&self) -> Option<&str> {
        match &self.edge_type {
            EdgeType::Regular { text, .. } => text.as_deref(),
            EdgeType::ReplacedByDummies { text, .. } => text.as_deref(),
            _ => None,
        }
    }
}

impl_index!(EdgeId, Edge, edge_idx);

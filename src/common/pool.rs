use crate::common::node::Node;
use crate::lexer::{PeBpmnProtection, TokenCoordinate};
use std::collections::HashSet;

// pool.rs
use crate::common::graph::{LaneId, NodeId};
use crate::common::graph::{PoolId, SdeId};
use crate::common::lane::Lane;
use crate::common::macros::impl_index;
use crate::common::node::LayerId;
pub struct Pool {
    /// None: Anonymous Pool. Invariant: If a graph contains a pool with a None name, then this is
    /// the only pool in the graph. In this case the pool is not rendered at all, and its contained
    /// only lane must have a None name as well.
    pub name: Option<String>,
    pub lanes: Vec<Lane>,
    /// Pools can be put on one vertical line. In this case, this is set to true.
    /// Note: This is not yet integrated correctly, only in the xy_ilp function.
    pub is_right_of_the_previous_pool: bool,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub stroke_color: Option<String>,
    pub fill_color: Option<String>,
    /// If `name.is_none()`, then this is the tc of the statement which implicitly created the
    /// anonymous lane.
    pub tc: TokenCoordinate,

    #[allow(non_snake_case)]
    pub tee_admin_has_pe_bpmn_visibility_A_for: HashSet<(SdeId, PeBpmnProtection)>,
    #[allow(non_snake_case)]
    pub tee_admin_has_pe_bpmn_visibility_H_for: HashSet<(SdeId, PeBpmnProtection)>,
    pub tee_external_root_access: HashSet<(SdeId, PeBpmnProtection)>,
}

impl Pool {
    pub fn new(name: Option<String>, tc: TokenCoordinate) -> Self {
        Pool {
            name,
            tc,
            lanes: Vec::new(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            is_right_of_the_previous_pool: false,
            stroke_color: None,
            fill_color: None,
            tee_admin_has_pe_bpmn_visibility_A_for: HashSet::new(),
            tee_admin_has_pe_bpmn_visibility_H_for: HashSet::new(),
            tee_external_root_access: HashSet::new(),
        }
    }

    pub fn add_lane(&mut self, lane: Option<String>, tc: TokenCoordinate) -> LaneId {
        self.lanes.push(Lane::new(lane, tc));
        LaneId(self.lanes.len() - 1)
    }

    pub fn add_node(
        &mut self,
        nodes: &mut Vec<Node>,
        lane: LaneId,
        node_id: NodeId,
        layer: Option<LayerId>,
    ) {
        if let Some(layer) = layer
            && let Some(position) = self.lanes[lane]
                .nodes
                .iter()
                .position(|node_id| nodes[node_id.0].layer_id >= layer)
        {
            // If the layer is known, we need to insert it at the correct position in this sorted
            // Lane::nodes vector (sorted by layer).
            self.lanes[lane].nodes.insert(position, node_id);
        } else {
            self.lanes[lane].nodes.push(node_id);
        }
    }
}

impl_index!(PoolId, Pool, pool_idx);

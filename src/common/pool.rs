use std::collections::{HashMap, HashSet};
use vecset::VecSet;

// pool.rs
use crate::common::graph::{LaneId, NodeId};
use crate::common::graph::{PoolId, SdeId};
use crate::common::lane::Lane;
use crate::common::macros::impl_index;
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

    pub tee_admin_has_pe_bpmn_visibility_A_for: VecSet<SdeId>,
    pub tee_admin_has_pe_bpmn_visibility_H_for: VecSet<SdeId>,
    pub tee_external_root_access: HashSet<SdeId>,
}

impl Pool {
    pub fn new(name: Option<String>) -> Self {
        Pool {
            name,
            lanes: Vec::new(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            is_right_of_the_previous_pool: false,
            stroke_color: None,
            fill_color: None,
            tee_admin_has_pe_bpmn_visibility_A_for: VecSet::new(),
            tee_admin_has_pe_bpmn_visibility_H_for: VecSet::new(),
            tee_external_root_access: HashSet::new(),
        }
    }

    pub fn add_lane(&mut self, lane: Option<String>) -> LaneId {
        self.lanes.push(Lane::new(lane));
        LaneId(self.lanes.len() - 1)
    }

    pub fn add_node(&mut self, lane: LaneId, node_id: NodeId) {
        self.lanes[lane.0].nodes.push(node_id);
    }
}

impl_index!(PoolId, Pool, pool_idx);

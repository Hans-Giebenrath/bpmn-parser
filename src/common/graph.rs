use super::macros::impl_index;
use crate::common::bpmn_node::{BpmnNode, EventVisual};
use crate::common::config::Config;
use crate::common::edge::{DummyEdgeBendPoints, Edge, EdgeType};
use crate::common::node::LayerId;
use crate::common::node::{Node, NodeType};
use crate::common::pool::Pool;
use crate::lexer::{DataType, EventType};
use crate::parser::ParseError;
use annotate_snippets::Level;
use proc_macros::{e, from, n, to};
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Debug};
use std::mem;

use super::edge::FlowType;

/// Represents a graph consisting of nodes and edges.
#[derive(Default)]
pub struct Graph {
    /// Nodes shall only be added to this Vec, but the order shall not be modified.
    /// Otherwise NodeIds will point to the wrong nodes.
    pub nodes: Vec<Node>,
    /// Edges shall only be added to this Vec, but the order shall not be modified.
    /// Otherwise EdgeIds will point to the wrong edges.
    pub edges: Vec<Edge>,
    pub pools: Vec<Pool>,

    pub data_elements: Vec<SemanticDataElement>,
    pub config: Config,

    pub num_layers: usize,
}

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub struct SemanticDataElement {
    pub name: String,
    pub data_element: Vec<NodeId>,
}

#[derive(Debug, Clone, Eq, PartialEq, Copy, Hash, PartialOrd, Ord)]
pub struct SdeId(pub usize);
impl_index!(SdeId, SemanticDataElement, sde_idx);

/// A Newtype to make sure that code outside of the module does not modify its value.
/// The invariant is that every created NodeId does point to some existing node.
#[derive(PartialEq, Default, Clone, Debug, Copy, Hash, Eq, PartialOrd, Ord)]
pub struct PoolId(pub usize);

/// A Newtype to make sure that code outside of the module does not modify its value.
/// The invariant is that every created NodeId does point to some existing node.
#[derive(PartialEq, Default, Clone, Debug, Copy, Hash, Eq, PartialOrd, Ord)]
pub struct LaneId(pub usize);

// TODO Naming is a bit long, but it says what it is. Maybe `PoolLane` is shorted and better than
// `PoolAndLane`? Or maybe something funny like `Poolane`? `PoLa`? `Place2`? `Coord2`?
// `PoolLane` seems the most reasonable. The variables could then be shortly named `poolane`,
// `poollane` or really `pool_lane`. Probably `pool_lane` is the most readable after all.
#[derive(PartialEq, Default, Clone, Debug, Copy, Hash, Eq, PartialOrd, Ord)]
pub struct PoolAndLane {
    pub pool: PoolId,
    pub lane: LaneId,
}

pub struct Coord3 {
    pub pool_and_lane: PoolAndLane,
    pub layer: LayerId,
    pub half_layer: bool,
}

/// A Newtype to make sure that code outside of the module does not modify its value.
/// The invariant is that every created NodeId does point to some existing node.
#[derive(PartialEq, Default, Clone, Debug, Copy, Hash, Eq)]
pub struct NodeId(pub usize);

#[derive(PartialEq, Default, Clone, Debug, Copy, Hash, Eq)]
pub struct EdgeId(pub usize);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Graph {
    pub fn add_pool(&mut self, pool: Option<String>) -> PoolId {
        self.pools.push(Pool::new(pool));
        PoolId(self.pools.len() - 1)
    }

    pub fn add_node(
        &mut self,
        node_type: NodeType,
        pool_lane: PoolAndLane,
        // This is usually known in later stages when dummy nodes are added. Since in that case the
        // Lane::nodes array sorted order must be uphold, this is done by this function.
        layer: Option<LayerId>,
    ) -> NodeId {
        add_node(
            &mut self.nodes,
            &mut self.pools,
            node_type,
            pool_lane,
            layer,
        )
    }

    pub fn add_edge(
        &mut self,
        from: NodeId,
        to: NodeId,
        edge_type: EdgeType,
        flow_type: FlowType,
    ) -> EdgeId {
        let edge_id = EdgeId(self.edges.len());
        self.edges.push(Edge {
            from,
            to,
            edge_type,
            flow_type,
            is_reversed: false,
            stroke_color: None,
            stays_within_lane: self.nodes[from.0].pool == self.nodes[to.0].pool
                && self.nodes[from.0].lane == self.nodes[to.0].lane,
        });

        self.nodes[from.0].outgoing.push(edge_id);
        self.nodes[to.0].incoming.push(edge_id);
        edge_id
    }

    pub fn add_sde(&mut self, name: String, node_ids: Vec<NodeId>) -> SdeId {
        let sde_id = SdeId(self.data_elements.len());
        self.data_elements.push(SemanticDataElement {
            name,
            data_element: node_ids,
        });
        sde_id
    }

    pub fn transported_data(&self, edge_id: EdgeId) -> Option<&[SdeId]> {
        let edge = &self.edges[edge_id];
        match &edge.flow_type {
            FlowType::DataFlow(aux) => Some(&aux.transported_data),
            FlowType::MessageFlow(aux) => Some(&aux.transported_data),
            _ => None,
        }
    }

    /// Only allowed before the bend points are added.
    pub fn reverse_edge(&mut self, edge_id: EdgeId) {
        // Modifies the edge in place (instead of marking it as deleted and creating a new one), so
        // no unnecessary hole is created within graph.edges.
        let edge = &mut self.edges[edge_id.0];
        assert!(edge.from.0 != edge.to.0);
        assert!(
            self.nodes[edge.from.0]
                .outgoing
                .iter()
                .any(|e| e.0 == edge_id.0)
        );
        self.nodes[edge.from.0]
            .outgoing
            .retain(|e| e.0 != edge_id.0);
        self.nodes[edge.from.0].incoming.push(edge_id);

        assert!(
            self.nodes[edge.to.0]
                .incoming
                .iter()
                .any(|e| e.0 == edge_id.0)
        );
        self.nodes[edge.to.0].incoming.retain(|e| e.0 != edge_id.0);
        self.nodes[edge.to.0].outgoing.push(edge_id);

        mem::swap(&mut edge.from, &mut edge.to);
        edge.is_reversed = true;
    }

    pub(crate) fn get_bottom_node(&self, pool_lane: PoolAndLane, layer: LayerId) -> Option<NodeId> {
        let PoolAndLane { pool, lane } = pool_lane;
        let mut it = self.pools[pool].lanes[lane].nodes.iter().cloned();
        loop {
            if let Some(node_id) = it.next()
                && let node = &self.nodes[node_id]
                && node.layer_id <= layer
            {
                if node.layer_id == layer && node.node_below_in_same_lane.is_none() {
                    return Some(node_id);
                }
            } else {
                // The layer within the current lane does not have any nodes.
                return None;
            }
        }
    }

    pub(crate) fn get_bottom_node_on_higher_lane(
        &self,
        lane_below_requested_one: PoolAndLane,
        layer: LayerId,
    ) -> Option<(PoolAndLane, Option<NodeId>)> {
        let pool_lane @ PoolAndLane { mut pool, mut lane } = lane_below_requested_one;
        if lane.0 == 0 {
            if pool.0 == 0 {
                return None;
            }
            pool.0 -= 1;
            lane.0 = self.pools[pool].lanes.len() - 1;
        }

        Some((pool_lane, self.get_bottom_node(pool_lane, layer)))
    }

    pub(crate) fn get_top_node(&self, pool_lane: PoolAndLane, layer: LayerId) -> Option<NodeId> {
        let PoolAndLane { pool, lane } = pool_lane;

        let mut it = self.pools[pool].lanes[lane].nodes.iter().cloned();
        loop {
            if let Some(node_id) = it.next()
                && let node = &self.nodes[node_id]
                && node.layer_id <= layer
            {
                if node.layer_id == layer && node.node_above_in_same_lane.is_none() {
                    return Some(node_id);
                }
            } else {
                // The layer within the current lane does not have any nodes.
                return None;
            }
        }
    }

    pub(crate) fn get_top_node_on_lower_lane(
        &self,
        lane_above_requested_one: PoolAndLane,
        layer: LayerId,
    ) -> Option<(PoolAndLane, Option<NodeId>)> {
        let pool_lane @ PoolAndLane { mut pool, mut lane } = lane_above_requested_one;
        if lane.0 == self.pools[pool].lanes.len() - 1 {
            if pool.0 == self.pools.len() - 1 {
                return None;
            }
            pool.0 += 1;
            lane.0 = 0;
        }

        Some((pool_lane, self.get_top_node(pool_lane, layer)))
    }
}

pub fn node_size(node_type: &NodeType) -> (usize, usize) {
    let event = match &node_type {
        NodeType::DummyNode => {
            // Height of 0 so there is just padding between the lines.
            // Otherwise there would be too much whitespace between lines.
            return (100, 0);
        }
        NodeType::RealNode { event, .. } => event,
    };

    match event {
        // Start Events
        BpmnNode::Event(_, _) => (36, 36),

        // Gateways
        BpmnNode::Gateway(_) => (50, 50),

        // Activities
        BpmnNode::Activity(_) => (100, 80),

        BpmnNode::Data(DataType::Store, _) => (50, 50),
        BpmnNode::Data(DataType::Object, _) => (36, 50),
    }
}

impl Debug for Graph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "graph - #nodes {} #edges {}",
            self.nodes.len(),
            self.edges.len()
        )?;
        for n in &self.nodes {
            writeln!(
                f,
                "  node: {} (p: {}, l: {}, lyr: {:?}) - {:?} - in: {:?}, out: {:?}",
                n.id.0,
                n.pool.0,
                n.lane.0,
                n.layer_id,
                n.display_text(),
                n.incoming,
                n.outgoing
            )?;
        }
        for e in &self.edges {
            writeln!(f, "  edge: {} -> {}", e.from.0, e.to.0)?;
        }
        for (pool_idx, pool) in self.pools.iter().enumerate() {
            for (lane_idx, lane) in pool.lanes.iter().enumerate() {
                writeln!(f, "  p/l {}/{}: {:?}", pool_idx, lane_idx, lane.nodes)?;
            }
        }
        Ok(())
    }
}

impl SemanticDataElement {
    pub(crate) fn contains(&self, node_id: NodeId) -> bool {
        self.data_element.contains(&node_id)
    }
}

type ValidationErrors = ParseError;

pub fn validate_invariants(graph: &Graph) -> Result<(), ValidationErrors> {
    // TODO
    //
    // (1) there is no situation where an edge's from has multiple edges in its outgoing vec, and
    // the edge's to has multiple edges in its incoming vec. TODO in the future this should
    // actually work.
    // (2) No self-loops: An edge's from is different from to.
    // (3) The gateway connection stuff should make sense
    // (4) Data names should be consistent (when sending and receiving something)

    let mut errors = Vec::new();

    {
        for message_flow in graph.edges.iter().filter(|e| e.is_message_flow()) {
            check_if_valid_message_flow_start(&graph.nodes[message_flow.from], &mut errors);
            check_if_valid_message_flow_end(&graph.nodes[message_flow.to], &mut errors);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_if_valid_message_flow_start(node: &Node, errors: &mut ValidationErrors) {
    if let NodeType::RealNode { event, tc, .. } = &node.node_type {
        match event {
            BpmnNode::Event(EventType::Message, EventVisual::Throw | EventVisual::End) => (),
            BpmnNode::Activity(_) => (),
            _ => {
                errors.push((
                "This node type cannot send messages. Only message events (M#) or tasks (e.g. .-) can be used as message flow starts. Note that shorthand events (#) are automatically transformed into message events when they are used in a message flow.".to_string(),
                    *tc,
                    Level::Error
                ));
            }
        }
    }
}

fn check_if_valid_message_flow_end(node: &Node, errors: &mut ValidationErrors) {
    if let NodeType::RealNode { event, tc, .. } = &node.node_type {
        match event {
            BpmnNode::Event(EventType::Message, EventVisual::Start(_) | EventVisual::Catch(_)) => {}
            BpmnNode::Event(EventType::Blank, _) => (),
            BpmnNode::Activity(_) => (),
            _ => {
                errors.push((
                "This node type cannot send messages. Only message events (M#) or tasks (e.g. .-) can be used as message flow starts. Note that shorthand events (#) are automatically transformed into message events when they are used in a message flow.".to_string(),
                    *tc,
                    Level::Error
                ));
            }
        }
    }
}

/// Very small function, does not really deserve it's own file? Is also just a helper thingy, not
/// really a dedicated phase.
pub(crate) fn sort_lanes_by_layer(graph: &mut Graph) {
    for pool in &mut graph.pools {
        for lane in &mut pool.lanes {
            lane.nodes
                .sort_unstable_by_key(|node_id| graph.nodes[node_id.0].layer_id.0);
        }
    }
}

pub(crate) fn adjust_above_and_below_for_new_inbetween(
    inbetween: NodeId,
    above: Option<NodeId>,
    below: Option<NodeId>,
    graph: &mut Graph,
) {
    if let Some(above) = above {
        let above = &mut graph.nodes[above];
        assert!(above.node_below_in_same_lane == below);
        above.node_below_in_same_lane = Some(inbetween);
    }

    if let Some(below) = below {
        let below = &mut graph.nodes[below];
        assert!(below.node_above_in_same_lane == above);
        below.node_above_in_same_lane = Some(inbetween);
    }

    graph.nodes[inbetween].node_above_in_same_lane = above;
    graph.nodes[inbetween].node_below_in_same_lane = below;
}

/// Helper function to not call graph.add_node(..) directly, as this cancels borrows of graph.edges
/// as well.
pub(crate) fn add_node(
    nodes: &mut Vec<Node>,
    pools: &mut Vec<Pool>,
    node_type: NodeType,
    PoolAndLane { pool, lane }: PoolAndLane,
    layer: Option<LayerId>,
) -> NodeId {
    let node_id = NodeId(nodes.len());

    // Add node ID to the pool and lane stuff
    pools[pool].add_node(nodes, lane, node_id, layer);

    let (width, height) = node_size(&node_type);

    nodes.push(Node {
        id: node_id,
        node_type,
        pool,
        lane,
        x: 0,
        y: 0,
        width,
        height,
        stroke_color: None,
        fill_color: None,
        layer_id: layer.unwrap_or(LayerId(0)),
        node_above_in_same_lane: None,
        node_below_in_same_lane: None,
        uses_half_layer: false,
        incoming: Vec::new(),
        outgoing: Vec::new(),
        incoming_ports: Vec::new(),
        outgoing_ports: Vec::new(),
        aux: super::node::NodePhaseAuxData::None,
    });

    node_id
}

pub(crate) fn contains_only_dummy_nodes_in_intermediate_lanes(
    nodes: &[Node],
    pools: &[Pool],
    layer: LayerId,
    from_lane: PoolAndLane,
    to_lane: PoolAndLane,
) -> bool {
    let (from_lane, to_lane) = if from_lane < to_lane {
        (from_lane, to_lane)
    } else {
        (to_lane, from_lane)
    };

    let mut cur_lane = from_lane;
    cur_lane.lane.0 += 1;
    loop {
        if cur_lane == to_lane {
            // No blocker found on the way, we reached the target lane.
            return true;
        }
        let Some(pool) = pools.get(cur_lane.pool.0) else {
            unreachable!(
                "We should have reached to_lane? {from_lane:?}, {to_lane:?}, {cur_lane:?}"
            );
        };
        let Some(lane) = pool.lanes.get(cur_lane.lane.0) else {
            // Wrap to the next pool.
            cur_lane.pool.0 += 1;
            cur_lane.lane.0 = 0;
            continue;
        };

        for node_id in &lane.nodes {
            let node = &nodes[*node_id];
            if node.layer_id < layer {
                continue;
            }
            if node.layer_id > layer {
                // We cleared the current layer.
                cur_lane.lane.0 += 1;
                break;
            }
            if !node.is_dummy() {
                return false;
            }
        }
    }
}

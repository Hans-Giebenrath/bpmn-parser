use bpmd_graph::TokenCoordinate;
use bpmd_graph::graph::Graph;
use bpmd_graph::graph::NodeId;
use bpmd_graph::graph::PoolAndLane;
use bpmd_util::vecset::VecSet;
use proc_macros::{from, n};

#[derive(Debug, Eq, PartialEq)]
pub struct SameLayerLaneCrossing {
    pub top_pool_lane: PoolAndLane,
    pub top_tc: TokenCoordinate,
    pub top_node_id: NodeId,
    pub bot_pool_lane: PoolAndLane,
    pub bot_tc: TokenCoordinate,
    pub bot_node_id: NodeId,
}

impl Ord for SameLayerLaneCrossing {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            self.top_pool_lane,
            self.bot_pool_lane,
            self.top_node_id,
            self.bot_node_id,
        )
            .cmp(&(
                other.top_pool_lane,
                other.bot_pool_lane,
                other.top_node_id,
                other.bot_node_id,
            ))
    }
}

impl PartialOrd for SameLayerLaneCrossing {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub fn same_layer_lane_crossings_within_cluster(
    graph: &Graph,
    cluster: &VecSet<NodeId>,
) -> impl Iterator<Item = SameLayerLaneCrossing> {
    cluster.iter().flat_map(|node_id|
            // Only need to inspect `incoming`, as we are only interested for those pairs where
            // the other end is also within the cluster, so if we would look at that one's
            // `outgoing`, we'd get duplicates.
        {let node = &n!(*node_id);
            node.incoming
                .iter()
                .map(|e| &from!(*e))
                .map(|n| (n.id, n.pool_and_lane()))
                .filter(|(n, _)| n!(*n).pool == node.pool) // We don't inspect message flows.
                .filter(|(n, _)| cluster.contains(n))
                .map(|(n, pool_lane)| {
                    if node.pool_and_lane() < pool_lane {
                        SameLayerLaneCrossing {
                            top_pool_lane: node.pool_and_lane(),
                            top_node_id: node.id,
                            top_tc: node.tc(),
                            bot_pool_lane: pool_lane,
                            bot_node_id: n,
                            bot_tc: n!(n).tc(),
                        }
                    } else {
                        SameLayerLaneCrossing {
                            bot_pool_lane: node.pool_and_lane(),
                            bot_node_id: node.id,
                            bot_tc: node.tc(),
                            top_pool_lane: pool_lane,
                            top_node_id: n,
                            top_tc: n!(n).tc(),
                        }
                    }
                })})
}

use bpmd_graph::*;

pub(crate) fn classify_barrier_node_for_gateway(
    gateway_node_id: NodeId,
    node: &Node,
) -> Option<NodeId> {
    match &node.node_type {
        NodeType::LongEdgeDummy => None,
        // The back edge corner dummy is meant to create a loop, i.e. in this context it behaves
        // like a LongEdgeDummy (it does not go vertical exactly here, but only in the in-between
        // layers area where vertical regular edge segments are located at.
        NodeType::BackEdgeCornerDummy { .. } => None,
        // (Not sure about this): Technically this seems incorrect (we would only inspect bend
        // dummies from the other direction (incoming vs outgoing) due to the two phases of looking
        // at them, so we would align bend dummies on the same side). However, the later xy-ilp phase
        // ensures that the bend dummies are correctly aligned not above / not below the gateway
        // node.
        NodeType::BendDummy {
            originating_node, ..
        } if *originating_node == gateway_node_id => None,
        // The gateway node itself is not a barrier as well. We transition across it.
        _ if node.id == gateway_node_id => None,
        // But every other kind of node (real node or another gateway's bend dummy) is a barrier.
        _ => Some(node.id),
    }
}

use crate::Graph;
use crate::common::graph::EdgeId;
use crate::common::node::Node;
use proc_macros::from;
use proc_macros::to;

pub fn introduce_snake_edge_bisect_dummies(graph: &mut Graph) {
    for edge_id in (0..graph.edges.len()).map(EdgeId) {
        let to = &to!(edge_id);
        let from = &from!(edge_id);

        let is_snake_edge = to.layer_id == from.layer_id
            && to.pool == from.pool
            // We don't have flipped nodes (were outgoing is left and incoming is right),
            // so if `from` or `to` are dummy nodes, then that means that they are back edge corner
            // dummies. Codified below as an assert.
            && !from.is_any_dummy()
            && !to.is_any_dummy();

        if !is_snake_edge {
            assert!(
                !(to.layer_id == from.layer_id && to.pool == from.pool)
                    || (from.is_back_edge_corner_dummy() && !to.is_any_dummy())
                    || (!from.is_any_dummy() && to.is_back_edge_corner_dummy())
            );
            continue;
        }

        todo!("Replace the edge.");
    }
}

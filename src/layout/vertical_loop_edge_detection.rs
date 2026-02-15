use crate::common::edge::EdgeType;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::graph::NodeId;
use crate::common::graph::StartAt;
use crate::common::node::Node;
use crate::common::node::NodeType;
use proc_macros::e;
use std::collections::HashMap;
use std::collections::HashSet;

pub fn vertical_loop_edge_detection(graph: &mut Graph) {
    let mut nodes_to_look_at = HashSet::new();

    for node in graph.nodes.iter() {
        match &node.node_type {
            NodeType::SnakeEdgeBisectDummy {
                from_real_node_id,
                to_real_node_id,
                ..
            } => {
                nodes_to_look_at.insert(*from_real_node_id);
                nodes_to_look_at.insert(*to_real_node_id);
            }
            NodeType::BackEdgeCornerDummy {
                same_layer_real_node_id,
                ..
            } => {
                nodes_to_look_at.insert(*same_layer_real_node_id);
            }
            _ => (),
        }
    }

    let mut snake_edge_state = HashMap::new();
    let mut to_be_verticalised = Vec::new();

    for node_id in nodes_to_look_at {
        process_node(
            graph,
            node_id,
            &mut snake_edge_state,
            &mut to_be_verticalised,
        );
        // Do this already here, so `to_be_verticalised` can be cleared immediately afterwards and
        // thus stays small.
        for edge_id in to_be_verticalised.iter() {
            e!(*edge_id).is_vertical = true;
        }
        to_be_verticalised.clear();
    }

    // Snake edges should only be vertical if both ends are vertical. Otherwise, it looks weird.
    for (_, (e1, e2)) in snake_edge_state.into_iter() {
        let (Some(e1), Some(e2)) = (e1, e2) else {
            continue;
        };
        e!(e1).is_vertical = true;
        e!(e2).is_vertical = true;
    }
}

fn process_node(
    graph: &Graph,
    this_node_id: NodeId,
    snake_edge_state: &mut HashMap<EdgeId, (Option<EdgeId>, Option<EdgeId>)>,
    // Circumventing the borrow checker (could also make `is_vertical` a Cell, but that would be
    // annoying to use afterwards I guess?). Can't say `e!(..).is_vertical = true` while borrowing
    // `graph`.
    to_be_verticalised: &mut Vec<EdgeId>,
) {
    for other_node in graph.iter_upwards_same_pool(StartAt::Node(this_node_id), None) {
        match process_node_edge(
            graph,
            this_node_id,
            other_node,
            snake_edge_state,
            to_be_verticalised,
        ) {
            Flow::Continue => continue,
            Flow::FoundBlocker => return,
        }
    }
    for other_node in graph.iter_downwards_same_pool(StartAt::Node(this_node_id), None) {
        match process_node_edge(
            graph,
            this_node_id,
            other_node,
            snake_edge_state,
            to_be_verticalised,
        ) {
            Flow::Continue => continue,
            Flow::FoundBlocker => return,
        }
    }
}

enum Flow {
    Continue,
    FoundBlocker,
}

// Note: Conceptually a duplicate to `port_assignment::classify_barrier_node` but not sure
// how to unify them without making it just more confusing.
fn process_node_edge(
    graph: &Graph,
    this_node_id: NodeId,
    other_node: &Node,
    snake_edge_state: &mut HashMap<EdgeId, (Option<EdgeId>, Option<EdgeId>)>,
    // Circumventing the borrow checker (could also make `is_vertical` a Cell, but that would be
    // annoying to use afterwards I guess?). Can't say `e!(..).is_vertical = true` while borrowing
    // `graph`.
    to_be_verticalised: &mut Vec<EdgeId>,
) -> Flow {
    // The edge to the real node is always on index [0], this is just how the construction works
    // in the dummy node generation.
    // TODO but why is [1] correct then? I sure must be looking at the wrong end of the
    // edge then?
    match &other_node.node_type {
        NodeType::RealNode { .. } => Flow::FoundBlocker,
        NodeType::LongEdgeDummy => Flow::Continue,
        // (technically not possible as BendDummy nodes are only inserted in a later phase.)
        // Our own bend dummy, we can just move past.
        NodeType::BendDummy {
            originating_node, ..
        } if *originating_node == this_node_id => Flow::Continue,
        // (technically not possible as BendDummy nodes are only inserted in a later phase.)
        // A foreign bend dummy, we must stop here.
        NodeType::BendDummy { .. } => Flow::FoundBlocker,
        NodeType::SnakeEdgeBisectDummy {
            from_real_node_id,
            from_edge_id,
            to_real_node_id,
            to_edge_id,
        } => {
            if *from_real_node_id == this_node_id || *to_real_node_id == this_node_id {
                let EdgeType::DummyEdge { original_edge, .. } = &e!(*from_edge_id).edge_type else {
                    unreachable!("should be a dummy edge, wat");
                };

                // The first insertion is `false`, the other insertion makes it `true`:
                // So if both sides are found to be potentially vertical, only then the whole
                // thing can be vertical, otherwise we must make a snake edge.
                snake_edge_state
                    .entry(*original_edge)
                    .and_modify(|state| state.1 = Some(*to_edge_id))
                    .or_insert_with(|| (Some(*from_edge_id), None));
            }
            // Even a foreign snake bisect dummy is *not* a reason to stop here, as it could be a snake,
            // and hence just like a long edge dummy.
            Flow::Continue
        }
        NodeType::BackEdgeCornerDummy {
            same_layer_real_node_id,
            same_layer_edge_id,
        } if *same_layer_real_node_id == this_node_id => {
            // The actual one of two lines which is the whole purpose of this file, the rest
            // just helping to find the situation to run this one line.
            to_be_verticalised.push(*same_layer_edge_id);
            Flow::Continue
        }
        NodeType::BackEdgeCornerDummy { .. } => Flow::FoundBlocker,
    }
}

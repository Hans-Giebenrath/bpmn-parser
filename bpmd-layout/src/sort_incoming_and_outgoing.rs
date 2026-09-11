use crate::common::graph::{Graph, NodeId};
use crate::common::node::Node;
use proc_macros::from;
use proc_macros::to;

/// TODO the images are wrong, make a BPMN from this to correct it.
/// Sort order images only show the upper part, but the lower part is equal, just mirrored.
///   TODO this is totally flawed. The left hand nodes are also entered from the left, not from the
///   right.
///
/// Sort order of incoming nodes:
///   NB: Horizontal part is closest to the `edge.from` node's pool.
///
/// `             ┌────┐ `
/// ` ┌┐          │    ┼─┐ `
/// ` │┼──┐       └────┘ │ `
/// ` └┘  │              │      (Pool 1)                  ┌──┐    ┌─┐ `
/// `     │              │                            ┌───┼  │ ┌──┼ │ `
/// `     │              │                            │   └──┘ │  └─┘ `
/// `     │              └─────┐ ┌────────────────────┘        │ `
/// `     └──────────────────┐ │ │ ┌───────────────────────────┘ `
/// ` ┌┐          ┌─────┐    │ │ │ │                       ┌┐ `
/// ` │┼─┐        │     ┼──┐ │ │ │ │   (Pool 2)         ┌──┼│   ┌┐ `
/// ` └┘ │        └─────┘  │ │ │ │ │                    │  └┘ ┌─┼│ `
/// `    │                 │ │ │ │ │ ┌──────────────────┘     │ └┘ `
/// `    │                 │ │ │ │ │ │ ┌──────────────────────┘ `
/// `    │                 │ │ │ │ │ │ │  ┌───────┐ `
/// `    │                 │ │ │ │ │ │ └──►       │ `
/// `    │                 │ │ │ │ │ └────►       │ `
/// `    │                 │ │ │ │ └──────►       │ `
/// `    │    (Pool 3)     │ │ │ └────────►       │ `
/// `    │                 │ │ └──────────►       │ `
/// `    │                 │ └────────────►       │ `
/// `    │                 └──────────────►       │ `
/// `    └────────────────────────────────►       │ `
/// `                                     │       │ `
/// `                                     └───────┘ `
///
///
///
/// Sort order of outgoing nodes:
///   NB: Horizontal part is closest to the `edge.from` node's pool.
///   TODO this is totally flawed. The left hand nodes are also entered from the left, not from the
///   right.
///
/// `┌────┐   ┌────┐   ┌────┐                    ┌────┐      ┌────┐ `
/// `│    ◄──┐│    ◄─┐ │    ◄─┐   (Pool 1)   ┌───►    │  ┌──►│    │ `
/// `└────┘  │└────┘ │ └────┘ │              │   └────┘  │   └────┘ `
/// `        │       │        │              │           │ `
/// `        │       │        │              │           │ `
/// `        │       │        │              │           │ `
/// `┌────┐  │       │        │              │   ┌────┐  │   ┌────┐ `
/// `│    ◄─┐        │        │   (Pool 2)   │┌──►    │  │┌──►    │ `
/// `└────┘ ││       │        └────┐ ┌───────┘│  └────┘  ││  └────┘ `
/// `       ││       └───────────┐ │ │ ┌──────┘          ││ `
/// `       │└──────────────────┐│ │ │ │┌────────────────┘│ `
/// `       └─────────────────┐ ││ │ │ ││ ┌───────────────┘ `
/// `                  ┌────┐ │ ││ │ │ ││ │ `
/// `                  │    │─┘ ││ │ │ ││ │ `
/// `                  │    │───┘│ │ │ ││ │ `
/// `                  │    │────┘ │ │ ││ │ `
/// `                  │    │──────┘ │ ││ │ `
/// `                  │    │────────┼►││ │   (Pool 3) `
/// `                  │    │────────┴─┘│ │ `
/// `                  │    │───────────┘ │ `
/// `                  │    │─────────────┘ `
/// `                  │    │ `
/// `                  └────┘ `
///
///
pub fn sort_incoming_and_outgoing(graph: &mut Graph) {
    // Sorting is done a bit inefficiently (n^2), feel free to improve this if it becomes a
    // bottleneck.
    // The strategy is to count how far we can go up in the `.node_above_in_same_lane` linked list.
    // The farther we can go up, the later it should come in the `.incoming`/`.outgoing` vec.
    fn rank_within_lane(node: &Node, graph: &Graph) -> usize {
        let mut rank_within_lane = 0;
        let mut node = node; // for the borrow checker, lol
        while let Some(above) = node.node_above_in_same_lane {
            rank_within_lane += 1;
            node = &graph.nodes[above];
        }
        rank_within_lane
    }
    for node_id in (0..graph.nodes.len()).map(NodeId) {
        let node = &mut graph.nodes[node_id];
        if node.is_blackbox_node() {
            continue;
        }
        if node.incoming.len() > 1 {
            let mut incoming_cpy = std::mem::take(&mut node.incoming);
            incoming_cpy.sort_by_cached_key(|edge_id| {
                let from_node = &from!(*edge_id);
                let to_node = &to!(*edge_id);
                let from_rank_within_lane = rank_within_lane(from_node, graph);
                let to_rank_within_lane = rank_within_lane(to_node, graph);

                if from_node.pool == to_node.pool
                    && from_node.layer_id == to_node.layer_id
                    && (from_node.lane, from_rank_within_lane) < (to_node.lane, to_rank_within_lane)
                {
                    // SF/`DF` Looping downwards (toward this node) in the same pool
                    let group_order = 1;
                    (
                        group_order,
                        0isize, // pool is equal so ignore
                        0isize, // layerid is equal so ignore
                        -(from_node.lane.0 as isize),
                        -(from_rank_within_lane as isize),
                    )
                } else if from_node.pool < to_node.pool && from_node.layer_id >= to_node.layer_id {
                    // MF coming from above/right top
                    let group_order = 2;
                    (
                        group_order,
                        -(from_node.pool.0 as isize),
                        -(from_node.layer_id.0 as isize),
                        (from_node.lane.0 as isize),
                        (from_rank_within_lane as isize),
                    )
                } else if from_node.pool < to_node.pool && from_node.layer_id < to_node.layer_id {
                    // MF coming from left top
                    let group_order = 3;
                    (
                        group_order,
                        (from_node.pool.0 as isize),
                        -(from_node.layer_id.0 as isize),
                        (from_node.lane.0 as isize),
                        (from_rank_within_lane as isize),
                    )
                } else if from_node.pool == to_node.pool && from_node.layer_id < to_node.layer_id {
                    // SF/`DF` coming from the left
                    assert_eq!(to_node.layer_id.0, from_node.layer_id.0 + 1);
                    let group_order = 4;
                    (
                        group_order,
                        0isize, // pool is equal (+1) so ignore
                        0isize, // layerid is equal so ignore
                        (from_node.lane.0 as isize),
                        (from_rank_within_lane as isize),
                    )
                } else if from_node.pool > to_node.pool && from_node.layer_id < to_node.layer_id {
                    // MF coming from left bottom
                    let group_order = 5;
                    (
                        group_order,
                        (from_node.pool.0 as isize),
                        (from_node.layer_id.0 as isize),
                        (from_node.lane.0 as isize),
                        (from_rank_within_lane as isize),
                    )
                } else if from_node.pool > to_node.pool && from_node.layer_id >= to_node.layer_id {
                    // MF coming from below/right bottom
                    let group_order = 6;
                    (
                        group_order,
                        -(from_node.pool.0 as isize),
                        (from_node.layer_id.0 as isize),
                        (from_node.lane.0 as isize),
                        (from_rank_within_lane as isize),
                    )
                } else if from_node.pool == to_node.pool
                    && from_node.layer_id == to_node.layer_id
                    && (from_node.lane, from_rank_within_lane) > (to_node.lane, to_rank_within_lane)
                {
                    // SF/`DF` Looping downwards (toward this node) in the same pool
                    let group_order = 7;
                    (
                        group_order,
                        0isize, // pool is equal so ignore
                        0isize, // layerid is equal so ignore
                        -(from_node.lane.0 as isize),
                        -(from_rank_within_lane as isize),
                    )
                } else {
                    unreachable!("from_node: {from_node:#?}\nto_node: {to_node:#?}, from_rank_within_lane: {from_rank_within_lane}, to_rank_within_lane: {to_rank_within_lane},\ngraph: {graph:#?}");
                }
            });
            graph.nodes[node_id].incoming = incoming_cpy;
        }

        let node = &mut graph.nodes[node_id];
        if node.outgoing.len() > 1 {
            let mut outgoing_cpy = std::mem::take(&mut node.outgoing);
            outgoing_cpy.sort_by_cached_key(|edge_id| {
                let from_node = &from!(*edge_id);
                let to_node = &to!(*edge_id);
                let to_rank_within_lane = rank_within_lane(to_node, graph);
                let from_rank_within_lane = rank_within_lane(from_node, graph);

                if from_node.pool == to_node.pool
                    && from_node.layer_id == to_node.layer_id
                    && (to_node.lane, to_rank_within_lane) < (from_node.lane, from_rank_within_lane)
                {
                    // SF/`DF` Looping upwards in the same pool
                    let group_order = 1;
                    (
                        group_order,
                        0isize, // layerid is equal so ignore
                        0isize, // pool is equal so ignore
                        -(to_node.lane.0 as isize),
                        -(to_rank_within_lane as isize),
                    )
                } else if to_node.pool < from_node.pool {
                    // MF to upper pools (Note: The documentation also has group 2 there but that is
                    // a mistake, there is just "above", not split in left or right).
                    let group_order = 3;
                    (
                        group_order,
                        (to_node.layer_id.0 as isize),
                        (to_node.pool.0 as isize),
                        (to_node.lane.0 as isize),
                        (to_rank_within_lane as isize),
                    )
                } else if to_node.pool == from_node.pool && to_node.layer_id > from_node.layer_id {
                    // SF/`DF` to the right same pool
                    assert_eq!(to_node.layer_id.0, from_node.layer_id.0 + 1);
                    let group_order = 4;
                    (
                        group_order,
                        0isize, // layerid is equal (+1) so ignore
                        0isize, // pool is equal so ignore
                        (to_node.lane.0 as isize),
                        (to_rank_within_lane as isize),
                    )
                } else if to_node.pool > from_node.pool {
                    // MF to lower pools (Note: The documentation also has group 6 there but that is
                    // a mistake, there is just "below", not split in left or right).
                    let group_order = 5;
                    (
                        group_order,
                        -(to_node.layer_id.0 as isize),
                        (to_node.pool.0 as isize),
                        (to_node.lane.0 as isize),
                        (to_rank_within_lane as isize),
                    )
                } else if from_node.pool == to_node.pool
                    && from_node.layer_id == to_node.layer_id
                    && (to_node.lane, to_rank_within_lane) > (from_node.lane, from_rank_within_lane)
                {
                    // SF/`DF` Looping downwards in the same pool
                    let group_order = 7;
                    (
                        group_order,
                        0isize, // layerid is equal so ignore
                        0isize, // pool is equal so ignore
                        -(to_node.lane.0 as isize),
                        -(to_rank_within_lane as isize),
                    )
                } else {
                    unreachable!("from_node: {from_node:#?}\nto_node: {to_node:#?}, from_rank_within_lane: {from_rank_within_lane}, to_rank_within_lane: {to_rank_within_lane},\ngraph: {graph:#?}");
                }
            });
            graph.nodes[node_id].outgoing = outgoing_cpy;
        }
    }
}

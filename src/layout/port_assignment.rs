use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::Edge;
use crate::common::edge::EdgeType;
use crate::common::graph::Coord3;
use crate::common::graph::DUMMY_NODE_WIDTH;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::graph::NodeId;
use crate::common::graph::Place;
use crate::common::graph::PoolAndLane;
use crate::common::graph::StartAt;
use crate::common::graph::add_node;
use crate::common::graph::adjust_above_and_below_for_new_inbetween;
use crate::common::graph::node_size;
use crate::common::node::BendDummyKind;
use crate::common::node::LayerId;
use crate::common::node::Node;
use crate::common::node::NodeType;
use crate::common::node::RelativePort;
use proc_macros::{e, from, n, to};
use std::collections::HashSet;

pub fn port_assignment(graph: &mut Graph) {
    // First handle non-gateway nodes and then gateway nodes in two separate loops. This way,
    // `is_vertical_edge` is easier as it does not need to account for bend dummies which make
    // everything harder.
    for node_id in (0..graph.nodes.len()).map(NodeId) {
        if !n!(node_id).is_gateway() {
            handle_nongateway_node(node_id, graph);
        }
    }
    for node_id in (0..graph.nodes.len()).map(NodeId) {
        if n!(node_id).is_gateway() {
            handle_gateway_node(node_id, graph);
        }
    }
}

#[derive(Debug)]
pub(crate) enum PlaceForBendDummy {
    AtTheTop,
    Above(NodeId),
    Below(NodeId),
    AtTheBottom,
}

/// The boundary of a box is on a first approximation subdivided into eight areas:
/// The North, East, West and South ports, and then the four corner areas between them.
/// ("Corner area" in the sense that it is not just the corner point, but also the parts of the
/// edges connected to the corner up to the next N/E/W/S point.)
///  ┌─── N ───┐
///  │┌───────┐│  <top
///  ││       ││
///  W│       │E  <W and E are always on the middle of left and right.
///  ││       ││
///  │└───────┘│  <bottom
///  └─── S ───┘
///   ^       ^
///   left    right
///       ^
///       N and S can be shifted (only to the left I believe) to make room for more elements on the
///       sides. ("I believe" because from the `incoming` side everything else must be on "left" so
///       there cannot be other elements on the left of N or S. For `outgoing`, the "directly
///       up/down" edge is already placed to the very beginning or very end, so again all other
///       elements are surely placed right of N or S).
///
/// "Port": The (x,y) coordinate where an edge starts (or ends) on the boundary of the box.
/// Invariant which we assume at this point: `incoming` and `outgoing` are sorted correctly by
/// how their ports are supposed to be ordered around the box.
/// For non-gateways, the assignment of specific ports to the edges is done as follows:
///  (Invariant: We assume there is only <=1 SF in `incoming` and <=1 SF in `outgoing`)
///  (1) The incoming nodes are analysed. There are only SF, MF and DF, but no boundary events.
///  They are expected to be only on the left side,
///  with the exception when the "start" of the edge is exactly above or below the current node in
///  the same layer (across lanes or pools, possibly), with only dummy nodes in between. In that
///  case the port can be on N or S as well.
///  Now we can assign the incoming SF (if there is one which is not on S or N) to W and place the
///  other left-side ports with equivalent gaps according to their order in `incoming`.
///  The N or S assigned port is not yet assigned a specific x coordinate.
///
///  (2) The outgoing edges are mostly done the same. Exception is that they can have boundary
///  events which should be located on "top" or "bottom". So if a DF or MF is farther from E than
///  a boundary event, then they cannot be put on the "right" as otherwise crossings would be
///  created. So they need to be put onto "top" or "bottom" left of the boundary event.
///
///  Now we know what elements need to fit on "top", "bottom" and "right", so they can be layouted
///  with according gaps (E must be in the middle of "right", as W must be in the middle of
///  "left").
///
/// Now we have edges which have an additional bend point, since they leave to the top or bottom and
/// then must turn right. For these situations we must add additional dummy nodes above or below
/// the current node which represents the bend point:
///
/// ┌────────┐ current node
/// │        │
/// │        │
/// │        ├──────────
/// │        │
/// │        │
/// └──┬─┬─┬─┘
///    │ │ │
///    │ │ │
///  ┌────────┐
///  │ │ │ └──│──────────
///  └────────┘ new dummy node
///    │ │
///  ┌────────┐
///  │ │ └────│──────────
///  └────────┘ new dummy node
///    │
///    │
///    │
///    │
///  ┌───────┐
///  │       │  Some next neighbor
///  │       │
///  │       │
///  │       │
///  └───────┘
///
///
///  Gateways are treated slightly differently:
///  They are expected to have *only* SFs. No DFs, no MFs, no boundary events. (Maybe comments but
///  these are likely treated differently then, not sure yet)
///  The side which has only one SF simply will have its port on W or E (in the middle).
///  The other side will have them all connected to the new dummy nodes, they are all expected to
///  go out at the top or bottom. But with the big caveat: Since we don't know really what of them
///  shall leave below or above the node, the dummy nodes are not enforced to be above or below
///  the gateway node. Consequently, some dummy node may be located "within" the gateway node,
///  and in that case the outgoing edge will be assigned the respective E or W.
///
fn handle_nongateway_node(this_node_id: NodeId, graph: &mut Graph) {
    let this_node = &graph.nodes[this_node_id];
    let mut incoming_ports = Vec::<RelativePort>::new();
    let mut outgoing_ports = Vec::<RelativePort>::new();

    let mut above_inc = 0;
    let mut above_out = 0;
    let mut below_inc = 0;
    let mut below_out = 0;

    let mut has_vertical_edge_above = false;
    let mut has_vertical_edge_below = false;

    /***********************************************************************/
    /*  PHASE 1: Determine how many ports are above, below, right and low  */
    /***********************************************************************/

    // Determine how many incoming ports are `above` and `below` (and implicitly `left`).
    match &this_node.incoming[..] {
        [] => (),
        [single] => match is_vertical_edge(*single, graph, this_node_id) {
            None => (),
            Some(VerticalEdgeDocks::Above) => {
                above_inc = 1;
                has_vertical_edge_above = true;
                e!(*single).is_vertical = true;
            }
            Some(VerticalEdgeDocks::Below) => {
                below_inc = 1;
                has_vertical_edge_below = true;
                e!(*single).is_vertical = true;
            }
        },
        [first, .., last] => {
            match is_vertical_edge(*first, graph, this_node_id) {
                None => (),
                Some(VerticalEdgeDocks::Above) => {
                    above_inc = 1;
                    has_vertical_edge_above = true;
                    e!(*first).is_vertical = true;
                }
                Some(VerticalEdgeDocks::Below) => {
                    unreachable!(
                        "Can't be. first: {:?}, this_node_id: {:?}",
                        *first, this_node_id
                    )
                }
            }
            match is_vertical_edge(*last, graph, this_node_id) {
                None => (),
                Some(VerticalEdgeDocks::Above) => {
                    unreachable!("Can't be")
                }
                Some(VerticalEdgeDocks::Below) => {
                    below_inc = 1;
                    has_vertical_edge_below = true;
                    e!(*last).is_vertical = true;
                }
            }
        }
    }

    // Determine how many outgoing ports are `above` and `below` (and implicitly `right`).
    match &this_node.outgoing[..] {
        [] => (),
        [single] => match is_vertical_edge(*single, graph, this_node_id) {
            None => {
                // A single boundary event usually is placed on the bottom side.
                if graph.boundary_events.contains_key(&(this_node.id, *single)) {
                    below_out = 1;
                }
            }
            Some(VerticalEdgeDocks::Above) => {
                above_out = 1;
                assert!(!has_vertical_edge_above);
                has_vertical_edge_above = true;
                e!(*single).is_vertical = true;
            }
            Some(VerticalEdgeDocks::Below) => {
                below_out = 1;
                assert!(!has_vertical_edge_below);
                has_vertical_edge_below = true;
                e!(*single).is_vertical = true;
            }
        },
        many @ [first, .., last] => {
            // First check if the ends are vertical edges.
            match is_vertical_edge(*first, graph, this_node_id) {
                None => (),
                Some(VerticalEdgeDocks::Above) => {
                    above_out += 1;
                    assert!(!has_vertical_edge_above);
                    has_vertical_edge_above = true;
                    e!(*first).is_vertical = true;
                }
                Some(VerticalEdgeDocks::Below) => {
                    unreachable!("Can't be")
                }
            }
            match is_vertical_edge(*last, graph, this_node_id) {
                None => (),
                Some(VerticalEdgeDocks::Above) => {
                    unreachable!("Can't be")
                }
                Some(VerticalEdgeDocks::Below) => {
                    below_out += 1;
                    assert!(!has_vertical_edge_below);
                    has_vertical_edge_below = true;
                    e!(*last).is_vertical = true;
                }
            }

            // Now refine that number by looking at boundary events.
            // By default boundary events are placed below the node. So
            // start from the end to look for boundary events and a sequence flow.
            // The sequence flow flips over from moving stuff on the `below` side to putting
            // stuff on the `above` side. For example:
            //       (DF, BE1, DF, BE2, DF, MF, SF, DF, BE3, DF, BE4, DF)
            // Then everything up to (including) BE2 is on `above`, and everything
            // starting from BE3 is on `below`. If there would be no SF, then instead
            // everything starting from BE1 would be on `below`, and the first DF would be on
            // `right`.
            let mut it = many
                .iter()
                .cloned()
                .enumerate()
                // That one was already covered, no need to process it again.
                .skip(above_out)
                .rev()
                .enumerate()
                // If the last one was actually a vertical sequence flow, then we *still* want
                // to continue putting BEs on `below`. So just skip the vertical edge.
                .skip(below_out);
            for (rev_idx, (_, edge_id)) in it.by_ref() {
                if graph.boundary_events.contains_key(&(this_node.id, edge_id)) {
                    // rev_idx starts at 0, but if the 0th element is on `below` we want to
                    // have `below_out == 1`.
                    below_out = rev_idx + 1;
                } else if graph.edges[edge_id].is_sequence_flow() {
                    break;
                }
            }
            for (_, (idx, edge_id)) in it {
                if graph.boundary_events.contains_key(&(this_node.id, edge_id)) {
                    // idx starts at 0, but if the 0th element is on `above` we want to
                    // have `above_out == 1`.
                    above_out = idx + 1;
                    // We found the right-most boundary event, so we can short-circuit away
                    // here. Everything else left of it now must be on `above` as well.
                    break;
                }
            }
        }
    }

    /********************************************************/
    /*  PHASE 2: Assign specific XY positions to each port  */
    /********************************************************/

    // At this point we know exactly how many ports are at what side, so we can do simple math
    // stuff to place the ports using `points_on_side`.
    let mut above_coordinates =
        points_on_side(this_node.width, above_inc + above_out).map(|x| RelativePort { x, y: 0 });
    let mut below_coordinates =
        points_on_side(this_node.width, below_inc + below_out).map(|x| RelativePort {
            x,
            y: this_node.height,
        });
    if above_inc > 0 {
        assert_eq!(above_inc, 1);
        incoming_ports.push(above_coordinates.next().unwrap());
    }
    incoming_ports.extend(
        partitioned_points_on_side(
            this_node.height,
            this_node.incoming.len() - above_inc - below_inc,
            this_node
                .incoming
                .iter()
                .skip(above_inc)
                .position(|edge_id| e!(*edge_id).is_sequence_flow()),
        )
        .map(|y| RelativePort { x: 0, y }),
    );
    if below_inc > 0 {
        assert_eq!(below_inc, 1);
        incoming_ports.push(below_coordinates.next().unwrap());
    }

    outgoing_ports.extend(above_coordinates);
    outgoing_ports.extend(
        partitioned_points_on_side(
            this_node.height,
            this_node.outgoing.len() - above_out - below_out,
            this_node
                .outgoing
                .iter()
                .skip(above_out)
                .position(|edge_id| e!(*edge_id).is_sequence_flow()),
        )
        .map(|y| RelativePort {
            x: this_node.width,
            y,
        }),
    );
    outgoing_ports.extend(below_coordinates);

    /***************************************************************************/
    /*  PHASE 3: Add additional dummy nodes & edges for above and below ports  */
    /***************************************************************************/

    let this_node = &n!(this_node_id);
    let this_node_width = this_node.width;
    // The straight-up/down vertical edges are the first (left-most) one on the above/below sides.
    assert!(above_inc <= 1);
    assert!(below_inc <= 1);
    // If there is an above_inc edge, then that one is the vertical one.
    assert!(above_inc == 0 || has_vertical_edge_above);
    assert!(below_inc == 0 || has_vertical_edge_below);

    // TODO use smallvec for these, usually just a small amount of edges.
    // Dummy nodes are added top to bottom (starting with the ones farthest from this_node).
    let above_edges_which_require_bendpoint = this_node
        .outgoing
        .iter()
        .cloned()
        .zip(outgoing_ports.iter().map(|x| x.x))
        .take(above_out)
        .skip((has_vertical_edge_above as usize) - above_inc)
        .collect::<Vec<_>>();
    // Dummy nodes are added bottom to top (starting with the ones farthest from this_node).
    let below_edges_which_require_bendpoint = this_node
        .outgoing
        .iter()
        .cloned()
        .zip(outgoing_ports.iter().map(|x| x.x))
        .skip(this_node.outgoing.len() - below_out)
        // A bit unsure here, better use strict_sub to panic if I messed up.
        .take(below_out.strict_sub((has_vertical_edge_below as usize).strict_sub(below_inc)))
        // Turn it around. Then the node_above_in_same_lane adjustment is easier since it is always
        // relative to this_node.
        .rev()
        .collect::<Vec<_>>();

    enum IsWhere {
        Above,
        Below,
    }

    let united_edges_which_require_bendpoint = above_edges_which_require_bendpoint
        .into_iter()
        .map(|(above_edge_id, x)| (above_edge_id, x, IsWhere::Above))
        .chain(
            below_edges_which_require_bendpoint
                .into_iter()
                .map(|(below_edge_id, x)| (below_edge_id, x, IsWhere::Below)),
        );

    let Coord3 {
        pool_and_lane: from_pool_and_lane,
        layer: from_layer,
        ..
    } = this_node.coord3();

    for (edge_id, relative_port_x, is_where) in united_edges_which_require_bendpoint {
        let to_pool_and_lane = to!(edge_id).pool_and_lane();
        let is_same_lane = from_pool_and_lane == to_pool_and_lane;
        let (pool_and_lane, place) = match is_where {
            IsWhere::Above if is_same_lane => {
                (from_pool_and_lane, PlaceForBendDummy::Above(this_node_id))
            }
            IsWhere::Below if is_same_lane => {
                (from_pool_and_lane, PlaceForBendDummy::Below(this_node_id))
            }

            // In the target lane above, go to the lower end.
            // TODO is this actually useful? Or should the bend point actually be in the
            // from_lane? Or is it wrong anyway because, if the other-lane-border-node is
            // a dummy node we might want to be on its other side, not become ourselves the
            // border node? This seems to be unhandeable at the moment, maybe needs more
            // thought later.
            IsWhere::Above => {
                let barrier = graph
                    .iter_upwards_same_pool(StartAt::Node(this_node_id), Some(to_pool_and_lane))
                    .find(|node| !is_skippable_dummy_node(node, graph));
                match barrier {
                    Some(barrier) => (
                        barrier.pool_and_lane(),
                        PlaceForBendDummy::Below(barrier.id),
                    ),
                    None => (to_pool_and_lane, PlaceForBendDummy::AtTheBottom),
                }
            }
            IsWhere::Below => {
                let barrier = graph
                    .iter_downwards_same_pool(StartAt::Node(this_node_id), Some(to_pool_and_lane))
                    .find(|node| !is_skippable_dummy_node(node, graph));
                match barrier {
                    Some(barrier) => (
                        barrier.pool_and_lane(),
                        PlaceForBendDummy::Above(barrier.id),
                    ),
                    None => (to_pool_and_lane, PlaceForBendDummy::AtTheTop),
                }
            }
        };

        assert!(DUMMY_NODE_WIDTH >= this_node_width);
        // relative_port_x - (((this_node_width as i64) - (DUMMY_NODE_WIDTH as i64)) / 2);
        let relative_port_x = (relative_port_x + (DUMMY_NODE_WIDTH / 2)) - (this_node_width / 2);
        add_bend_dummy_node(
            graph,
            edge_id,
            pool_and_lane,
            from_layer,
            place,
            this_node_id,
            relative_port_x,
            BendDummyKind::FromBoundaryEvent,
        );
    }
    n!(this_node_id).incoming_ports = incoming_ports;
    n!(this_node_id).outgoing_ports = outgoing_ports;
}

fn handle_gateway_node(this_node_id: NodeId, graph: &mut Graph) {
    // See comment at the end of the function.
    let (above, below) = (
        n!(this_node_id).node_above_in_same_lane,
        n!(this_node_id).node_below_in_same_lane,
    );
    for direction in [Direction::Outgoing, Direction::Incoming] {
        let this_node = &mut graph.nodes[this_node_id];
        let (incoming_or_outgoing, ports, x) = match direction {
            Direction::Outgoing => (
                &this_node.outgoing[..],
                &mut this_node.outgoing_ports,
                this_node.width,
            ),
            Direction::Incoming => (&this_node.incoming[..], &mut this_node.incoming_ports, 0),
        };
        match incoming_or_outgoing {
            [] => {
                // ... is this valid at all? Maybe when in draft mode, i.e. while creating the diagram
                // and the diagram is just rendered as the BPMD text is written? So don't panic here.
            }
            [_single] => {
                // This edge must leave as a regular flow. Nothing shall be done here.
                ports.push(RelativePort {
                    x,
                    y: this_node.height / 2,
                });
            }
            many => {
                // Ports are in the center, since right now we don't know which one will be above, and
                // which one will be below. They can only be corrected at a later staged.
                ports.extend((0..many.len()).map(|_| RelativePort {
                    x: this_node.width / 2,
                    y: this_node.height / 2,
                }));

                handle_gateway_node_one_side(this_node_id, graph, direction);
            }
        }
    }
    // The gateway node might become detached if bend dummies are added in the same lane. In that
    // case the gateway node should point to the original above and below nodes to ensure that they
    // point to the original above/below. Note: If the original above itself was already a bend
    // dummy, it is replaced by its originating node to ensure proper spacing as well.
    n!(this_node_id).node_above_in_same_lane = above.map(|node_id| {
        if let NodeType::BendDummy {
            originating_node, ..
        } = &n!(node_id).node_type
            && n!(*originating_node).pool_and_lane() == n!(this_node_id).pool_and_lane()
        {
            assert_ne!(*originating_node, this_node_id);
            *originating_node
        } else {
            assert_ne!(node_id, this_node_id);
            node_id
        }
    });
    n!(this_node_id).node_below_in_same_lane = below.map(|node_id| {
        if let NodeType::BendDummy {
            originating_node, ..
        } = &n!(node_id).node_type
            && n!(*originating_node).pool_and_lane() == n!(this_node_id).pool_and_lane()
        {
            assert_ne!(*originating_node, this_node_id);
            *originating_node
        } else {
            assert_ne!(node_id, this_node_id);
            node_id
        }
    });
}

enum VerticalEdgeDocks {
    Below,
    Above,
}

/// Returns true if the start and end of the given edge are two real nodes (i.e. not dummy nodes)
/// which are in the same layer, and have no real nodes in between (only dummy nodes, if any).
/// This goes across lanes and pools for message flows.
fn is_vertical_edge(
    edge_id: EdgeId,
    graph: &Graph,
    current_node: NodeId,
) -> Option<VerticalEdgeDocks> {
    let from = &from!(edge_id);
    let to = &to!(edge_id);
    assert_ne!(from.id, to.id);

    if from.layer_id != to.layer_id {
        return None;
    }

    // We don't know which is above here, so we just need to search both ways.
    for (start_above, end_below) in [(from, to), (to, from)] {
        if start_above.pool_and_lane() > end_below.pool_and_lane() {
            continue;
        }
        let mut obstacle_in_the_way = false;
        for node in graph.iter_downwards_all_pools(
            StartAt::Node(start_above.id),
            Some(end_below.pool_and_lane()),
        ) {
            // Bend dummies should only be added later.
            assert!(!node.is_bend_dummy());
            if node.id == end_below.id {
                if obstacle_in_the_way {
                    return None;
                } else if current_node == start_above.id {
                    return Some(VerticalEdgeDocks::Below);
                } else {
                    return Some(VerticalEdgeDocks::Above);
                }
            }
            if !node.is_any_dummy() {
                // This function should run before gateway bend dummies are added. If this is not
                // possible then this logic needs to be adapted, will be a bit annoying...
                assert!(
                    !matches!(node.node_type, NodeType::BendDummy { originating_node , .. } if n!(originating_node).is_gateway())
                );
                obstacle_in_the_way = true;
            }
        }
    }

    unreachable!(
        "We did not find the other end of the edge in the same layer. Very, verrryyyy strange. Smells like ... a ... BUG?!"
    );
}

/// Return an iterator over `k` equally spaced interior points
/// on the line segment [x1, x2].
///
/// Example:
///   points_on_side(0.0, 10.0, 3).collect::<Vec<_>>()
///       == [2.5, 5.0, 7.5]
pub fn points_on_side(side_len: usize, k: usize) -> impl Iterator<Item = usize> + Clone {
    let step = side_len as f64 / (k as f64 + 1.0);
    (1..=k).map(move |i| ((i as f64) * step) as usize)
}

#[track_caller]
pub fn partitioned_points_on_side(
    side_len: usize,
    k: usize,
    mid_point: Option<usize>,
) -> impl Iterator<Item = usize> + Clone {
    assert!(
        mid_point.is_none() || mid_point.unwrap() < k,
        "midpoint {:?} k {k}",
        mid_point
    );
    let points_before_midpoint = mid_point.unwrap_or(0);
    let points_after_midpoint = mid_point.map_or_else(
        move || k,
        move |mid_point| k.strict_sub(1).strict_sub(mid_point),
    );
    points_on_side(side_len / 2, points_before_midpoint)
        .chain(mid_point.into_iter().map(move |_| side_len / 2))
        .chain(
            points_on_side(
                mid_point.map_or_else(|| 0, move |_| side_len / 2),
                points_after_midpoint,
            )
            .map(move |point| point + side_len / 2),
        )
}

enum Direction {
    Incoming,
    Outgoing,
}

fn maybe_update_best_position(
    current_best_position: &mut (bool, PoolAndLane, PlaceForBendDummy, i32),
    new_best_position_candidate: (bool, PoolAndLane, PlaceForBendDummy, i32),
    is_above_gateway: bool,
) {
    // Lower is better.
    let current_crossing_number = new_best_position_candidate.3;
    if is_above_gateway {
        // If the new position is at least as good as the previous best we update. This brings us
        // closer to the gateway node.
        if current_crossing_number <= current_best_position.3 {
            *current_best_position = new_best_position_candidate;
        }
    } else {
        // If we are below the gateway we only go farther from the gateway if the situation is
        // strictly better.
        if current_crossing_number < current_best_position.3 {
            *current_best_position = new_best_position_candidate;
        }
    }
}

// Try to push the dummy bend points as far outside as
// possible, i.e. until the crossing count increases. The goal is to "eat" as meany dummy nodes as
// possible (without moving into the "wrong" lane)
fn handle_gateway_node_one_side(this_node_id: NodeId, graph: &mut Graph, direction: Direction) {
    // borrow-checker workaround (with undo at the end)
    let edges = match direction {
        Direction::Outgoing => n!(this_node_id).outgoing.clone(),
        Direction::Incoming => n!(this_node_id).incoming.clone(),
    };
    let Coord3 {
        pool_and_lane: this_pool_and_lane,
        layer: this_layer,
        ..
    } = n!(this_node_id).coord3();
    // The x is relative to the created bend dummies.
    let relative_port_x = node_size(&NodeType::BendDummy {
        originating_node: Default::default(),
        kind: BendDummyKind::FromBoundaryEvent,
    })
    .0 / 2;
    let mut has_dummy_nodes_in_same_lane = false;
    let other_topmost_node = match direction {
        Direction::Outgoing => &to!(*edges.first().expect("caller-guaranteed this is not empty")),
        Direction::Incoming => &from!(*edges.first().expect("caller-guaranteed this is not empty")),
    };
    let top_most_pool_lane = other_topmost_node.pool_and_lane();
    let bottom_most_pool_lane = match direction {
        Direction::Outgoing => {
            to!(*edges.last().expect("caller-guaranteed this is not empty")).pool_and_lane()
        }
        Direction::Incoming => {
            from!(*edges.last().expect("caller-guaranteed this is not empty")).pool_and_lane()
        }
    };

    // First we search a regular (or dummy-(loop-connected)-to-regular) node as the top barrier.
    // Later these are our own newly inserted dummy nodes.
    let mut current_top_barrier = graph
        .iter_upwards_same_pool(StartAt::Node(this_node_id), Some(top_most_pool_lane))
        .find(|&node| !is_skippable_dummy_node(node, graph))
        .map(|node| node.id);

    // This is always the same. We iterate always until we hit this one.
    let bottom_barrier = graph
        .iter_downwards_same_pool(StartAt::Node(this_node_id), Some(bottom_most_pool_lane))
        .find(|node| !is_skippable_dummy_node(node, graph))
        .map(|node| node.id);

    let mut above_nodes_in_other_layer = HashSet::<NodeId>::from_iter(
        graph
            .iter_upwards_same_pool(StartAt::Node(other_topmost_node.id), None)
            .map(|node| node.id),
    );

    let mut is_above_gateway = false;
    let mut previous_other_node_id = None;
    for edge_id in edges.iter().cloned() {
        let other_node = match direction {
            Direction::Outgoing => &to!(edge_id),
            Direction::Incoming => &from!(edge_id),
        };
        let other_node_pool_lane = other_node.pool_and_lane();

        if let Some(previous_other_node_id) = previous_other_node_id {
            above_nodes_in_other_layer.extend(
                graph
                    .iter_upwards_same_pool(StartAt::Node(other_node.id), None)
                    .map(|node| node.id)
                    .take_while(|node_id| *node_id != previous_other_node_id),
            );
            above_nodes_in_other_layer.insert(previous_other_node_id);
        }
        previous_other_node_id = Some(other_node.id);

        let mut local_is_above_gateway = is_above_gateway;
        // Lower is better.
        let mut crossing_number = 0i32;
        let (mut best_position, iteration_start) = if let Some(top_barrier) = current_top_barrier {
            let best_position = (
                local_is_above_gateway,
                n!(top_barrier).pool_and_lane(),
                PlaceForBendDummy::Below(top_barrier),
                crossing_number,
            );
            let iteration_start = StartAt::Node(top_barrier);
            (best_position, iteration_start)
        } else {
            let best_position = (
                local_is_above_gateway,
                top_most_pool_lane,
                PlaceForBendDummy::AtTheTop,
                crossing_number,
            );
            let iteration_start = StartAt::PoolLane(Coord3 {
                pool_and_lane: top_most_pool_lane,
                layer: this_layer,
                half_layer: false, // irrelevant
            });
            (best_position, iteration_start)
        };

        for crossable_node_or_bottom_barrier_or_none in graph
            .iter_downwards_same_pool(
                iteration_start,
                Some(std::cmp::max(this_pool_and_lane, bottom_most_pool_lane)),
            )
            .take_while(|&crossable_node| Some(crossable_node.id) != bottom_barrier)
            .map(Some)
            .chain(std::iter::once(bottom_barrier.map(|node_id| &n!(node_id))))
        {
            let crossable_node_or_bottom_barrier = match crossable_node_or_bottom_barrier_or_none {
                Some(crossable_node_or_bottom_barrier)
                    if crossable_node_or_bottom_barrier.pool_and_lane() <= other_node_pool_lane =>
                {
                    crossable_node_or_bottom_barrier
                }
                _ => {
                    // other_node_pool_lane < crossable_node_or_bottom_barrier_or_none.pool_lane()
                    // no more nodes, or the crossing node is in some lower lane.
                    if best_position.1 < other_node_pool_lane {
                        // Must be moved to the new lane
                        best_position = (
                            local_is_above_gateway,
                            other_node_pool_lane,
                            PlaceForBendDummy::AtTheTop,
                            crossing_number,
                        );
                    }
                    break;
                }
            };

            if best_position.1 < crossable_node_or_bottom_barrier.pool_and_lane() {
                // Move the best position into the current lane (which is also closer to
                // `other_node_pool_lane`).
                best_position = (
                    local_is_above_gateway,
                    crossable_node_or_bottom_barrier.pool_and_lane(),
                    PlaceForBendDummy::AtTheTop,
                    crossing_number,
                );
                // But also continue, since we still need to check whether we need to cross more
                // nodes.
            }

            let crossable_node = if Some(crossable_node_or_bottom_barrier.id) != bottom_barrier {
                // Continue only if this is not the bottom barrier. And rename for clarity.
                crossable_node_or_bottom_barrier
            } else {
                break;
            };
            if crossable_node.id == this_node_id {
                // Don't inspect the outgoing edges of the gateway.
                local_is_above_gateway = false;
                continue;
            }

            // TODO If we have proper handling of loops, could it be that there are more outgoing
            // edges?
            let crossable_other_node = match direction {
                Direction::Outgoing => &to!(crossable_node.outgoing[0]),
                Direction::Incoming => &from!(crossable_node.incoming[0]),
            };

            // At this point we should actually have a counting system and iterate as far as we can.
            // Right now it is rather asymmetric.
            if above_nodes_in_other_layer.contains(&crossable_other_node.id) {
                crossing_number -= 1;
            } else if crossable_other_node.id != other_node.id {
                // So it is below other_node.
                crossing_number += 1;
            }

            maybe_update_best_position(
                &mut best_position,
                (
                    local_is_above_gateway,
                    crossable_node.pool_and_lane(),
                    PlaceForBendDummy::Below(crossable_node.id),
                    crossing_number,
                ),
                is_above_gateway,
            );
        }
        let kind = if this_pool_and_lane == other_node_pool_lane {
            BendDummyKind::FromGatewayToSameLane {
                gateway_node: this_node_id,
                target_node: other_node.id,
            }
        } else if best_position.1 == this_pool_and_lane {
            BendDummyKind::FromGatewayBlockedLaneCrossing {
                gateway_node: this_node_id,
                target_node: other_node.id,
            }
        } else {
            BendDummyKind::FromGatewayFreeLaneCrossing {
                gateway_node: this_node_id,
                target_node: other_node.id,
            }
        };
        let new_dummy_node = add_bend_dummy_node(
            graph,
            edge_id,
            best_position.1,
            this_layer,
            best_position.2,
            this_node_id,
            relative_port_x,
            kind,
        );
        if best_position.1 == this_pool_and_lane {
            has_dummy_nodes_in_same_lane = true;
        }
        is_above_gateway = best_position.0;
        // The order is already fixed, so we are not allowed to put any of the following nodes
        // above `new_dummy_node`. We insert from top to bottom.
        current_top_barrier = Some(new_dummy_node);
        continue;
    }

    if has_dummy_nodes_in_same_lane {
        // Take the gateway node out, so the Y-ILP does not force the gateway node exactly
        // between its above and below nodes. The idea is that it should actually be rather
        // floaty, and if less edge crossings can be achieved by moving it over above/below
        // nodes, then it should move to the more optimal position.
        let this_node = &mut n!(this_node_id);
        let above = this_node.node_above_in_same_lane.take();
        let below = this_node.node_below_in_same_lane.take();
        if let Some(above) = above {
            assert_ne!(Some(above), below);
            n!(above).node_below_in_same_lane = below;
        }
        if let Some(below) = below {
            assert_ne!(above, Some(below));
            n!(below).node_above_in_same_lane = above;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn add_bend_dummy_node(
    graph: &mut Graph,
    edge_id: EdgeId,
    pool_and_lane: PoolAndLane,
    layer: LayerId,
    place_for_bend_dummy: PlaceForBendDummy,
    reference_node: NodeId,
    relative_port_x: usize,
    kind: BendDummyKind,
) -> NodeId {
    let place = match place_for_bend_dummy {
        PlaceForBendDummy::AtTheTop => match graph.get_top_node(pool_and_lane, layer) {
            Some(top_node) => Place::Above(top_node),
            None => Place::AsOnlyNode,
        },
        PlaceForBendDummy::AtTheBottom => match graph.get_bottom_node(pool_and_lane, layer) {
            Some(bottom_node) => Place::Below(bottom_node),
            None => Place::AsOnlyNode,
        },
        PlaceForBendDummy::Above(x) => Place::Above(x),
        PlaceForBendDummy::Below(x) => Place::Below(x),
    };
    let dummy_node_id = add_node(
        &mut graph.nodes,
        &mut graph.pools,
        NodeType::BendDummy {
            originating_node: reference_node,
            kind,
        },
        pool_and_lane,
        Some(layer),
    );
    let current_num_edges = graph.edges.len();
    let cur_edge = &mut e!(edge_id);
    let flow_type = cur_edge.flow_type.clone();
    let from_id = cur_edge.from;
    let to_id = cur_edge.to;
    match &cur_edge.edge_type {
        EdgeType::Regular { text, .. } => {
            let text = text.clone();

            cur_edge.edge_type = EdgeType::ReplacedByDummies {
                first_dummy_edge: EdgeId(current_num_edges),
                text,
            };

            // First remove the reference to the now-replaced edge, so there is certainly room
            // for the new edge references, i.e. no need to allocate accidentally.
            let edge1 = {
                let edge1 = EdgeId(graph.edges.len());
                graph.edges.push(Edge {
                    from: from_id,
                    to: dummy_node_id,
                    edge_type: EdgeType::DummyEdge {
                        original_edge: edge_id,
                        bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                    },
                    flow_type: flow_type.clone(),
                    is_vertical: false,
                    is_reversed: false,
                    stroke_color: None,
                    stays_within_lane: n!(from_id).pool_and_lane() == pool_and_lane,
                });
                for outgoing_edge_idx in &mut n!(from_id).outgoing {
                    if *outgoing_edge_idx == edge_id {
                        *outgoing_edge_idx = edge1;
                        break;
                    }
                }
                n!(dummy_node_id).incoming.push(edge1);

                edge1
            };
            let edge2 = {
                let edge2 = EdgeId(graph.edges.len());
                graph.edges.push(Edge {
                    from: dummy_node_id,
                    to: to_id,
                    edge_type: EdgeType::DummyEdge {
                        original_edge: edge_id,
                        bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                    },
                    flow_type,
                    is_vertical: false,
                    is_reversed: false,
                    stroke_color: None,
                    stays_within_lane: pool_and_lane == n!(to_id).pool_and_lane(),
                });

                for incoming_edge_idx in &mut n!(to_id).incoming {
                    if *incoming_edge_idx == edge_id {
                        *incoming_edge_idx = edge2;
                        break;
                    }
                }

                n!(dummy_node_id).outgoing.push(edge2);

                edge2
            };
            if reference_node == from_id {
                e!(edge1).is_vertical = true;
            } else {
                e!(edge2).is_vertical = true;
            }
        }
        EdgeType::DummyEdge { original_edge, .. } => {
            let original_edge = *original_edge;

            graph.nodes[from_id]
                .outgoing
                .retain(|outgoing_edge_idx| *outgoing_edge_idx != edge_id);
            let new_dummy_edge_id = graph.add_edge(
                from_id,
                dummy_node_id,
                EdgeType::DummyEdge {
                    original_edge,
                    bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                },
                flow_type.clone(),
            );
            // Bend around
            e!(edge_id).from = dummy_node_id;
            n!(dummy_node_id).outgoing.push(edge_id);
            // The original edge might need to point to another edge as its first dummy edge.
            let original_edge = &mut graph.edges[original_edge];
            let EdgeType::ReplacedByDummies {
                first_dummy_edge, ..
            } = &mut original_edge.edge_type
            else {
                unreachable!();
            };
            if *first_dummy_edge == edge_id {
                *first_dummy_edge = new_dummy_edge_id;
            }
            if reference_node == from_id {
                e!(new_dummy_edge_id).is_vertical = true;
            } else {
                e!(edge_id).is_vertical = true;
            }
        }
        EdgeType::ReplacedByDummies { .. } => {
            unreachable!("This edge should not be part of the graph")
        }
    }

    n!(dummy_node_id).outgoing_ports.push(RelativePort {
        x: relative_port_x,
        y: 0,
    });
    n!(dummy_node_id).incoming_ports.push(RelativePort {
        x: relative_port_x,
        y: 0,
    });

    adjust_above_and_below_for_new_inbetween(dummy_node_id, place, graph);
    dummy_node_id
}

fn is_skippable_dummy_node(node: &Node, graph: &Graph) -> bool {
    if !node.is_any_dummy() {
        // A real node is a barrier through which we cannot send our edges further.
        return false;
    }
    let [single_incoming] = node.incoming[..] else {
        unreachable!("or is it?");
    };
    let [single_outgoing] = node.outgoing[..] else {
        unreachable!("or is it?");
    };
    let from_layer = from!(single_incoming).layer_id;
    let to_layer = to!(single_outgoing).layer_id;
    // Just some sanity checks, in case this ever changes.
    assert!(from_layer == node.layer_id || from_layer.0 + 1 == node.layer_id.0);
    assert!(to_layer == node.layer_id || node.layer_id.0 + 1 == to_layer.0);
    if from_layer == node.layer_id || to_layer == node.layer_id {
        // A dummy which is looping in the same layer. This means that it is likely a
        // helper node for a neighboring gateway or node with boundary events.
        return false;
    }
    true
}

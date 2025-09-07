use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::EdgeType;
use crate::common::graph::Coord3;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::graph::NodeId;
use crate::common::graph::PoolAndLane;
use crate::common::graph::add_node;
use crate::common::graph::adjust_above_and_below_for_new_inbetween;
use crate::common::graph::contains_only_dummy_nodes_in_intermediate_lanes;
use crate::common::node::Node;
use std::collections::HashMap;
use std::collections::HashSet;
//use crate::common::macros::{from, to};
use crate::common::node::NodeType;
use crate::common::node::Port;
use proc_macros::{e, from, n, to};

pub fn port_assignment(graph: &mut Graph) {
    for node_id in (0..graph.nodes.len()).map(NodeId) {
        if graph.nodes[node_id].is_gateway() {
            handle_gateway_node(node_id, graph);
        } else {
            handle_nongateway_node(node_id, graph);
        }
    }
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
    let mut incoming_ports = Vec::<Port>::new();
    let mut outgoing_ports = Vec::<Port>::new();

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
            }
            Some(VerticalEdgeDocks::Below) => {
                below_inc = 1;
                has_vertical_edge_below = true;
            }
        },
        [first, .., last] => {
            match is_vertical_edge(*first, graph, this_node_id) {
                None => (),
                Some(VerticalEdgeDocks::Above) => {
                    above_inc = 1;
                    has_vertical_edge_above = true;
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
                    below_inc = 1;
                    has_vertical_edge_below = true;
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
                if this_node.is_boundary_event(*single) {
                    below_out = 1;
                }
            }
            Some(VerticalEdgeDocks::Above) => {
                above_out = 1;
                assert!(!has_vertical_edge_above);
                has_vertical_edge_above = true;
            }
            Some(VerticalEdgeDocks::Below) => {
                below_out = 1;
                assert!(!has_vertical_edge_below);
                has_vertical_edge_below = true;
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
            while let Some((rev_idx, (_, edge_id))) = it.next() {
                if this_node.is_boundary_event(edge_id) {
                    // rev_idx starts at 0, but if the 0th element is on `below` we want to
                    // have `below_out == 1`.
                    below_out = rev_idx + 1;
                } else if graph.edges[edge_id].is_sequence_flow() {
                    break;
                }
            }
            while let Some((_, (idx, edge_id))) = it.next() {
                if this_node.is_boundary_event(edge_id) {
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
    // stuff to place the ports using `points_on_segment`.
    let mut above_coordinates =
        points_on_side(this_node.width, above_inc + above_out).map(|x| Port {
            x,
            y: 0,
            on_top_or_bottom: true,
        });
    let mut below_coordinates =
        points_on_side(this_node.width, below_inc + below_out).map(|x| Port {
            x,
            y: 0,
            on_top_or_bottom: true,
        });
    if above_inc > 0 {
        assert_eq!(above_inc, 1);
        incoming_ports.push(above_coordinates.next().unwrap());
    }
    incoming_ports.extend(
        points_on_side(
            this_node.height,
            this_node.incoming.len() - above_inc - below_inc,
        )
        .map(|y| Port {
            x: 0,
            y,
            on_top_or_bottom: false,
        }),
    );
    if below_inc > 0 {
        assert_eq!(below_inc, 1);
        incoming_ports.push(below_coordinates.next().unwrap());
    }

    outgoing_ports.extend(above_coordinates);
    outgoing_ports.extend(
        points_on_side(
            this_node.height,
            this_node.outgoing.len() - above_out - below_out,
        )
        .map(|y| Port {
            x: 0,
            y,
            on_top_or_bottom: false,
        }),
    );
    outgoing_ports.extend(below_coordinates);

    /***************************************************************************/
    /*  PHASE 3: Add additional dummy nodes & edges for above and below ports  */
    /***************************************************************************/

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
        .take(above_out)
        .skip((has_vertical_edge_above as usize) - above_inc)
        .cloned()
        .collect::<Vec<_>>();
    // Dummy nodes are added bottom to top (starting with the ones farthest from this_node).
    let below_edges_which_require_bendpoint = this_node
        .outgoing
        .iter()
        .skip(this_node.outgoing.len() - below_out)
        // A bit unsure here, better use strict_sub to panic if I messed up.
        .take(below_out.strict_sub((has_vertical_edge_below as usize).strict_sub(below_inc)))
        .cloned()
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
        .map(|x| (x, IsWhere::Above))
        .chain(
            below_edges_which_require_bendpoint
                .into_iter()
                .map(|x| (x, IsWhere::Below)),
        );

    let Coord3 {
        pool_and_lane: from_pool_and_lane,
        layer: from_layer,
        ..
    } = this_node.coord3();

    for (edge_id, is_where) in united_edges_which_require_bendpoint {
        let current_num_edges = graph.edges.len();
        let cur_edge = &mut graph.edges[edge_id];
        let flow_type = cur_edge.flow_type.clone();
        let to_id = cur_edge.to;
        let to_pool_and_lane @ PoolAndLane {
            pool: to_pool,
            lane: to_lane,
        } = graph.nodes[to_id].pool_and_lane();
        let dummy_node_id = add_node(
            &mut graph.nodes,
            &mut graph.pools,
            NodeType::DummyNode,
            to_pool_and_lane,
            Some(from_layer),
        );
        match &cur_edge.edge_type {
            EdgeType::Regular { text, .. } => {
                let text = text.clone();

                cur_edge.edge_type = EdgeType::ReplacedByDummies {
                    first_dummy_edge: EdgeId(current_num_edges),
                    text,
                };

                // First remove the reference to the now-replaced edge, so there is certainly room
                // for the new edge references, i.e. no need to allocate accidentally.
                graph.nodes[this_node_id]
                    .outgoing
                    .retain(|outgoing_edge_idx| *outgoing_edge_idx != edge_id);
                graph.nodes[to_id]
                    .incoming
                    .retain(|incoming_edge_idx| *incoming_edge_idx != edge_id);

                graph.add_edge(
                    this_node_id,
                    dummy_node_id,
                    EdgeType::DummyEdge {
                        original_edge: edge_id,
                        bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                    },
                    flow_type.clone(),
                );
                graph.add_edge(
                    dummy_node_id,
                    to_id,
                    EdgeType::DummyEdge {
                        original_edge: edge_id,
                        bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                    },
                    flow_type,
                );
            }
            EdgeType::DummyEdge { original_edge, .. } => {
                let original_edge = *original_edge;

                graph.nodes[this_node_id]
                    .outgoing
                    .retain(|outgoing_edge_idx| *outgoing_edge_idx != edge_id);
                let new_dummy_edge_id = graph.add_edge(
                    this_node_id,
                    dummy_node_id,
                    EdgeType::DummyEdge {
                        original_edge,
                        bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                    },
                    flow_type.clone(),
                );
                // Bend around
                graph.edges[edge_id].from = dummy_node_id;
                graph.nodes[dummy_node_id].outgoing.push(edge_id);
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
            }
            EdgeType::ReplacedByDummies { .. } => {
                unreachable!("This edge should not be part of the graph")
            }
        }
        let is_same_lane = from_pool_and_lane == to_pool_and_lane;
        let way_is_free = contains_only_dummy_nodes_in_intermediate_lanes(
            &graph.nodes,
            &graph.pools,
            from_layer,
            from_pool_and_lane,
            to_pool_and_lane,
        );
        match is_where {
            IsWhere::Above if is_same_lane || !way_is_free => {
                adjust_above_and_below_for_new_inbetween(
                    dummy_node_id,
                    graph.nodes[this_node_id].node_above_in_same_lane,
                    Some(this_node_id),
                    graph,
                );
            }
            IsWhere::Above => {
                // In the target lane above, go to the lower end.
                // TODO is this actually useful? Or should the bend point actually be in the
                // from_lane? Or is it wrong anyway because, if the other-lane-border-node is
                // a dummy node we might want to be on its other side, not become ourselves the
                // border node? This seems to be unhandeable at the moment, maybe needs more
                // thought later.
                for (idx, node_id) in graph.pools[to_pool].lanes[to_lane].nodes.iter().enumerate() {
                    let node = &mut graph.nodes[*node_id];
                    if node.layer_id < from_layer {
                        continue;
                    }
                    if node.layer_id > from_layer {
                        graph.pools[to_pool].lanes[to_lane]
                            .nodes
                            .insert(idx, dummy_node_id);
                        break;
                    }
                    if node.node_below_in_same_lane.is_none() {
                        node.node_below_in_same_lane = Some(dummy_node_id);
                        graph.nodes[dummy_node_id].node_above_in_same_lane = Some(*node_id);
                        graph.pools[to_pool].lanes[to_lane]
                            .nodes
                            .insert(idx, dummy_node_id);
                        break;
                    }
                }
            }
            IsWhere::Below if is_same_lane || !way_is_free => {
                adjust_above_and_below_for_new_inbetween(
                    dummy_node_id,
                    Some(this_node_id),
                    graph.nodes[this_node_id].node_below_in_same_lane,
                    graph,
                );
            }
            IsWhere::Below => {
                // In the target lane below, go to the upper end.
                // TODO is this actually useful? Or should the bend point actually be in the
                // from_lane? Or is it wrong anyway because, if the other-lane-border-node is
                // a dummy node we might want to be on its other side, not become ourselves the
                // border node? This seems to be unhandeable at the moment, maybe needs more
                // thought later.
                for (idx, node_id) in graph.pools[to_pool].lanes[to_lane].nodes.iter().enumerate() {
                    let node = &mut graph.nodes[*node_id];
                    if node.layer_id < from_layer {
                        continue;
                    }
                    if node.layer_id > from_layer {
                        graph.pools[to_pool].lanes[to_lane]
                            .nodes
                            .insert(idx, dummy_node_id);
                        break;
                    }
                    if node.node_above_in_same_lane.is_none() {
                        node.node_above_in_same_lane = Some(dummy_node_id);
                        graph.nodes[dummy_node_id].node_below_in_same_lane = Some(*node_id);
                        graph.pools[to_pool].lanes[to_lane]
                            .nodes
                            .insert(idx, dummy_node_id);
                        break;
                    }
                }
            }
        }
    }
}

fn handle_gateway_node(this_node_id: NodeId, graph: &mut Graph) {
    let mut incoming_ports = Vec::<Port>::new();
    let mut _outgoing_ports = Vec::<Port>::new();

    /*  PHASE 1: Determine how many ports are above, below, right and low  */
    /***********************************************************************/

    // Determine how many incoming ports are `above` and `below` (and implicitly `left`).
    let this_node = &mut graph.nodes[this_node_id];
    match &this_node.incoming[..] {
        [] => {
            // .. is this valid at all? Maybe when in draft mode, i.e. while creating the diagram
            // and the diagram is just rendered as the BPMD text is written? So don't panic here.
        }
        [_single] => {
            // This edge must leave as a regular flow. Nothing shall be done here.
            incoming_ports.push(Port {
                x: 0,
                y: this_node.height / 2,
                on_top_or_bottom: true,
            });
        }
        many => {
            // Ports are in the center, since right now we don't know which one will be above, and
            // which one will be below. They can only be corrected at a later staged.
            incoming_ports.extend((0..many.len()).map(|_| Port {
                x: this_node.width / 2,
                y: this_node.height / 2,
                on_top_or_bottom: true,
            }));

            let Coord3 {
                pool_and_lane: this_pool_and_lane,
                layer,
                ..
            } = this_node.coord3();

            let mut above_in_same_lane = this_node.node_above_in_same_lane;
            let mut below_in_same_lane = this_node.node_below_in_same_lane;

            // Add a borrow-checker circumvention hack.
            let incoming = std::mem::take(&mut this_node.incoming);
            // Now, we don't know which edges will be above or below. So we apply the trick to
            // simply take this_node out of this above/below chain. The gateway balancing
            // constraint however also leads to a correct y-assignment of the gateway.
            // But this only works if there are at least two edges leading to the same lane.
            // Otherwise, if there is none or just one edge, then all those dummy node shenanigans
            // should not be done for the current lane at all (but still for those nodes on other
            // lanes).
            // Also, the above and below need an additional constraint to be far enough from each
            // other to certainly let room for the gateway. This can be checked in the y-ILP by
            // looking at the gateway nodes' above/below_in_same_lane whether these also in turn
            // have the gateway node in place, or not. If not, then they need to get an additional
            // distance constraint. TODO did I implement this?
            // TODO maybe this whole sole-node stuff is irrelevant? Right now not using it.
            let sole_this_lane_node = {
                let mut it = incoming.iter().filter(|&&edge_id| {
                    graph.nodes[graph.edges[edge_id].from].pool_and_lane() == this_pool_and_lane
                });
                let a_first_node = it.next();
                let a_second_node = it.next();
                if a_second_node.is_none() {
                    a_first_node
                } else {
                    if let Some(above_in_same_lane) = above_in_same_lane {
                        graph.nodes[above_in_same_lane].node_below_in_same_lane =
                            below_in_same_lane;
                    }
                    if let Some(below_in_same_lane) = below_in_same_lane {
                        graph.nodes[below_in_same_lane].node_above_in_same_lane =
                            above_in_same_lane;
                    }
                    None
                }
            };
            for edge_id in incoming.iter().cloned() {
                //                if Some(edge_id) == sole_this_lane_node {
                //                    // We don't create dummy nodes for this one, it will just leave to the right of
                //                    // the gateway (TODO well, does it? in principle we could also apply the
                //                    continue;
                //                }
                let current_num_edges = graph.edges.len();
                let cur_edge = &mut graph.edges[edge_id];
                let flow_type = cur_edge.flow_type.clone();
                let to_id = cur_edge.to;
                let to_pool_and_lane = graph.nodes[to_id].pool_and_lane();
                let dummy_node_id = add_node(
                    &mut graph.nodes,
                    &mut graph.pools,
                    NodeType::DummyNode,
                    to_pool_and_lane,
                    Some(layer),
                );
                match &cur_edge.edge_type {
                    EdgeType::Regular { text, .. } => {
                        let text = text.clone();

                        cur_edge.edge_type = EdgeType::ReplacedByDummies {
                            first_dummy_edge: EdgeId(current_num_edges),
                            text,
                        };

                        // First remove the reference to the now-replaced edge, so there is certainly room
                        // for the new edge references, i.e. no need to allocate accidentally.
                        graph.nodes[this_node_id]
                            .outgoing
                            .retain(|outgoing_edge_idx| *outgoing_edge_idx != edge_id);
                        graph.nodes[to_id]
                            .incoming
                            .retain(|incoming_edge_idx| *incoming_edge_idx != edge_id);

                        graph.add_edge(
                            this_node_id,
                            dummy_node_id,
                            EdgeType::DummyEdge {
                                original_edge: edge_id,
                                bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                            },
                            flow_type.clone(),
                        );
                        graph.add_edge(
                            dummy_node_id,
                            to_id,
                            EdgeType::DummyEdge {
                                original_edge: edge_id,
                                bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                            },
                            flow_type,
                        );
                    }
                    EdgeType::DummyEdge { original_edge, .. } => {
                        let original_edge = *original_edge;

                        graph.nodes[this_node_id]
                            .outgoing
                            .retain(|outgoing_edge_idx| *outgoing_edge_idx != edge_id);
                        let new_dummy_edge_id = graph.add_edge(
                            this_node_id,
                            dummy_node_id,
                            EdgeType::DummyEdge {
                                original_edge,
                                bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                            },
                            flow_type.clone(),
                        );
                        // Bend around
                        graph.edges[edge_id].from = dummy_node_id;
                        graph.nodes[dummy_node_id].outgoing.push(edge_id);
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
                    }
                    EdgeType::ReplacedByDummies { .. } => {
                        unreachable!("This edge should not be part of the graph")
                    }
                }
            }
            // Undo the borrow-checker surrounding hack.
            graph.nodes[this_node_id].incoming = incoming;
        }
    }

    // Determine how many outgoing ports are `above` and `below` (and implicitly `right`).
    let this_node = &graph.nodes[this_node_id];
    match &this_node.outgoing[..] {
        _ => todo!(),
    }
}

enum VerticalEdgeDocks {
    Below,
    Above,
}

/// Returns true if the start and end of the given edge are two real nodes (i.e. not dummy nodes)
/// which are in the same layer, and have no real nodes in between (only dummy nodes, if any).
/// This goes across lanes and pools for message flows.
/// Note: In the initial version this does *not* allow message flows to go up if they reach the
/// designated inter-pool space for horizontal travel. Message flows which are not strictly
/// vertical right now still need to move through the sides. But maybe this changes at some point
/// and I forget to update this comment.
fn is_vertical_edge(
    edge_id: EdgeId,
    graph: &Graph,
    current_node: NodeId,
) -> Option<VerticalEdgeDocks> {
    let edge = &graph.edges[edge_id];
    let from = &graph.nodes[edge.from];
    let to = &graph.nodes[edge.to];
    assert_ne!(edge.from, edge.to);

    if from.layer_id != to.layer_id {
        return None;
    }

    // We look from the upper node to the lower node
    let (start_above, end_below) = match from.pool_and_lane().cmp(&to.pool_and_lane()) {
        std::cmp::Ordering::Less => (from, to),
        std::cmp::Ordering::Greater => (to, from),
        std::cmp::Ordering::Equal => {
            // We actually don't know which is above here, so we just need to search both ways.
            let mut node = from; // for the borrow checker, lol
            let mut obstacle_in_the_way = false;
            while let Some(below) = node.node_below_in_same_lane {
                if below == edge.to {
                    if obstacle_in_the_way {
                        return None;
                    } else if current_node == below {
                        return Some(VerticalEdgeDocks::Above);
                    } else {
                        return Some(VerticalEdgeDocks::Below);
                    }
                }
                node = &graph.nodes[below];
                if !node.is_dummy() {
                    obstacle_in_the_way = true;
                }
            }
            // We didn't find `to` when going downwards from `from`, so apparently `to` is above
            // `from`. Let's not repeat that search here, but do it in the general case.
            (to, from)
        }
    };

    let mut node = Some(start_above);
    for pool in graph.pools.iter().skip(node.map_or(0, |n| n.pool.0)) {
        'lane: for lane in pool.lanes.iter().skip(node.map_or(0, |n| n.lane.0)) {
            // make sure that the outer `node` is None afterwards.
            let mut node = if let Some(node) = std::mem::take(&mut node) {
                node
            } else {
                let mut it = lane.nodes.iter().cloned();
                loop {
                    if let Some(node_id) = it.next()
                        && let node = &graph.nodes[node_id]
                        && node.layer_id <= start_above.layer_id
                    {
                        if node.layer_id == start_above.layer_id
                            && node.node_above_in_same_lane.is_none()
                        {
                            break node;
                        }
                    } else {
                        // The layer within the current lane does not have any nodes. That also
                        // means that there are no obstacles, and we just continue to check in the
                        // next lane.
                        continue 'lane;
                    }
                }
            };
            while let Some(below) = node.node_below_in_same_lane {
                if below == end_below.id {
                    if current_node == below {
                        return Some(VerticalEdgeDocks::Above);
                    } else {
                        return Some(VerticalEdgeDocks::Below);
                    }
                }
                node = &graph.nodes[below];
                if !node.is_dummy() {
                    return None;
                }
            }
            // We left the lane. So let's find out what is the next lane.
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
///   points_on_segment(0.0, 10.0, 3).collect::<Vec<_>>()
///       == [2.5, 5.0, 7.5]
pub fn points_on_side(side_len: usize, k: usize) -> impl Iterator<Item = usize> {
    let step = side_len as f64 / (k as f64 + 1.0);
    (1..=k).map(move |i| ((i as f64) * step) as usize)
}

// TODO actually do another strategy: Just try to push the dummy bend points as far outside as
// possible, i.e. until the crossing count increases. The goal is to "eat" as meany dummy nodes as
// possible (without moving into the "wrong" lane)

fn do_gateway_mapping(this_node_id: NodeId, graph: &mut Graph) {
    let edges = std::mem::take(&mut n!(this_node_id).outgoing);
    let this_layer = n!(this_node_id).layer_id;
    let other_topmost_node = &to!(*edges.first().expect("caller-guaranteed this is not empty"));
    let top_most_pool_lane = other_topmost_node.pool_and_lane();
    let bottom_most_pool_lane =
        to!(*edges.last().expect("caller-guaranteed this is not empty")).pool_and_lane();

    fn is_skippable_dummy_node(cur: NodeId, graph: &Graph) -> bool {
        let node = &n!(cur);
        if !node.is_dummy() {
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

    // First we search a regular (or dummy-(loop-connected)-to-regular) node as the top barrier.
    // Later these are our own newly inserted dummy nodes.
    let mut current_top_barrier = {
        let mut cur_above_node_in_same_lane = n!(this_node_id).node_above_in_same_lane;
        let mut cur_pool_lane = n!(this_node_id).pool_and_lane();
        loop {
            match cur_above_node_in_same_lane {
                Some(cur) => {
                    if !is_skippable_dummy_node(cur, graph) {
                        break cur_above_node_in_same_lane;
                    }

                    // We found a dummy node which goes to the right, and we continue our search to
                    // go further up to find a barrier.
                    cur_above_node_in_same_lane = n!(cur).node_above_in_same_lane;
                }
                None => {
                    if cur_pool_lane < top_most_pool_lane {
                        // There simply is no barrier, can insert it at the top.
                        break None;
                    } else if let Some((above_pool_lane, bottom_node)) = graph
                        .get_nextup_higher_node_same_pool(
                            cur_pool_lane,
                            top_most_pool_lane,
                            this_layer,
                        )
                    {
                        cur_pool_lane = above_pool_lane;
                        cur_above_node_in_same_lane = Some(bottom_node);
                    } else {
                        // There simply is no obstacle at all upwards, we left the topmost lane.
                        break None;
                    }
                }
            }
        }
    };

    // This is always the same. We iterate always until we hit this one.
    let bottom_barrier = {
        let mut cur_below_node_in_same_lane = n!(this_node_id).node_below_in_same_lane;
        let mut cur_pool_lane = n!(this_node_id).pool_and_lane();
        loop {
            match cur_below_node_in_same_lane {
                Some(cur) => {
                    if !is_skippable_dummy_node(cur, graph) {
                        break cur_below_node_in_same_lane;
                    }

                    // We found a dummy node which goes to the right, and we continue our search to
                    // go further up to find a barrier.
                    cur_below_node_in_same_lane = n!(cur).node_below_in_same_lane;
                }
                None => {
                    if cur_pool_lane > bottom_most_pool_lane {
                        // There simply is no barrier, can insert it at the top.
                        break None;
                    } else if let Some((below_pool_lane, top_node)) =
                        graph.get_top_node_on_lower_lane_same_pool(cur_pool_lane, this_layer)
                    {
                        cur_pool_lane = below_pool_lane;
                        cur_below_node_in_same_lane = top_node;
                    } else {
                        // There simply is no obstacle at all upwards, we left the topmost lane.
                        break None;
                    }
                }
            }
        }
    };

    let mut above_nodes_in_other_layer = {
        let mut above_nodes_in_other_layer = HashSet::new();
        let mut current_node = other_topmost_node;
        let mut current_pool_and_lane = other_topmost_node.pool_and_lane();
        loop {
            if let Some(above) = current_node.node_above_in_same_lane {
                above_nodes_in_other_layer.insert(above);
                current_node = &n!(above);
            } else if let Some((next_pool_and_lane, above)) = graph
                .get_nextup_higher_node_same_pool(
                    current_pool_and_lane,
                    PoolAndLane::default(),
                    this_layer,
                )
            {
            }
        }
    };

    for edge_id in edges.iter().cloned() {
        let to = &to!(edge_id);
        let to_pool_lane = to.pool_and_lane();
        if let Some(current_top_barrier) = current_top_barrier
            && to_pool_lane < n!(current_top_barrier).pool_and_lane()
        {
            // TODO must simply insert the node below the top-most barrier node.
            // TODO update the current_top_barrier.
            continue;
        }

        if let Some(bottom_barrier) = bottom_barrier
            && to_pool_lane > n!(bottom_barrier).pool_and_lane()
        {
            // TODO must simply insert the node above the bottom-most barrier node.
            continue;
        }

        // Can insert the node in the actual target lane, hooray!
        //  (1) calculate necessary helper information for crossing calculation
        //  (2) hypothetically go from top to bottom and check where are the least crossings.

        let mut surrounding_nodes = HashMap::new();
        {
            let mut cur_above = to.node_above_in_same_lane;
            while let Some(above_node_id) = cur_above {
                let above_node = &n!(above_node_id);
                surrounding_nodes.insert(above_node_id, 1i64);
                cur_above = above_node.node_above_in_same_lane;
            }
        }
        {
            let mut cur_below = to.node_below_in_same_lane;
            while let Some(below_node_id) = cur_below {
                let below_node = &n!(below_node_id);
                surrounding_nodes.insert(below_node_id, -1i64);
                cur_below = below_node.node_below_in_same_lane;
            }
        }
    }

    n!(this_node_id).outgoing = edges;
}

//fn do_gateway_mapping(this_node_id: NodeId, graph: &mut Graph) {
//    #[derive(Default)]
//    struct Segment {
//        pool_lane: PoolAndLane,
//        above_in_same_lane: Option<NodeId>,
//        below_in_same_lane: Option<NodeId>,
//        // For understanding when there is a wrap around when traversing the next layer.
//        above_in_same_lane_next_layer: Option<NodeId>,
//        below_in_same_lane_next_layer: Option<NodeId>,
//        // top to bottom
//        // First, these are the next-layer nodes where there are references to,
//        // then later these are the actual dummy nodes.
//        node_id_scratchpad: Vec<NodeId>,
//    }
//
//    let mut cur_pool_lane = graph.nodes[this_node_id].pool_and_lane();
//    let mut cur_above_node_in_same_lane = graph.nodes[this_node_id].node_above_in_same_lane;
//    let mut cur_below_node_in_same_lane = graph.nodes[this_node_id].node_below_in_same_lane;
//    let mut segments = vec![];
//    loop {
//        segments.push(Segment {
//            pool_lane: cur_pool_lane,
//            above_in_same_lane: cur_above_node_in_same_lane,
//            below_in_same_lane: cur_below_node_in_same_lane,
//            ..Default::default()
//        });
//        match cur_above_node_in_same_lane {
//            Some(cur) => {
//                let node = &graph.nodes[cur];
//                if !node.is_dummy() {
//                    // A real node is a barrier through which we cannot send our edges further.
//                    break;
//                }
//                let [single_incoming] = node.incoming[..] else {
//                    unreachable!("or is it?");
//                };
//                let [single_outgoing] = node.outgoing[..] else {
//                    unreachable!("or is it?");
//                };
//                let from_layer = from!(single_incoming).layer_id;
//                let to_layer = to!(single_outgoing).layer_id;
//                // Just some sanity checks, in case this ever changes.
//                assert!(from_layer == node.layer_id || from_layer.0 + 1 == node.layer_id.0);
//                assert!(to_layer == node.layer_id || node.layer_id.0 + 1 == to_layer.0);
//                if from_layer == node.layer_id || to_layer == node.layer_id {
//                    // A dummy which is looping in the same layer. This means that it is likely a
//                    // helper node for a neighboring gateway or node with boundary events.
//                    break;
//                }
//
//                // At this point we found it is a regular dummy node which also goes to the right.
//                // All good. This is a barrier to the target node layer.
//                // TODO continue here, I just want to understand whether the Segment type is useful
//                // or not.
//            }
//        }
//    }
//
//    let mut edge_to_next_layer_node_it = graph.nodes[this_node_id].outgoing.iter().cloned();
//    let mut current_edge_to_next_layer_node = edge_to_next_layer_node_it.next().expect(
//        "There is always a first next edge, otherwise this function would not have been called.",
//    );
//    let mut current_next_layer_node = &mut to!(current_edge_to_next_layer_node);
//    let mut segments_it = segments.iter_mut();
//    let mut current_segment = segments_it.next().expect(
//        "There is always at least the central segment where the gateway node is also part of.",
//    );
//    loop {
//        if current_segment.pool_lane < current_next_layer_node.pool_and_lane() {
//            // Need to progress the segments.
//            if let Some(segment) = segments_it.next() {
//                current_segment = segment;
//                continue;
//            } else {
//                // We went through all segments (i.e. we hit a "real" other node), so what is left
//                // over from the edges must be put into the last segment. First the one we are
//                // currently looking at, ..
//                current_segment
//                    .node_id_scratchpad
//                    .push(graph.edges[current_edge_to_next_layer_node].to);
//                // .. and then the rest.
//                current_segment
//                    .node_id_scratchpad
//                    .extend(edge_to_next_layer_node_it.map(|edge_id| graph.edges[edge_id].to));
//                break;
//            }
//        } else if current_segment.pool_lane > current_next_layer_node.pool_and_lane() {
//            // Need to progress the edges.
//            if let Some(edge_id) = edge_to_next_layer_node_it.next() {
//                current_segment
//                    .node_id_scratchpad
//                    .push(current_next_layer_node.id);
//                current_edge_to_next_layer_node = edge_id;
//                current_next_layer_node = &mut to!(edge_id);
//                continue;
//            } else {
//                // Edges are finished, cannot do anything with the segments. Some simply remain
//                // empty (which is a shame, why did we create them in the first place? Cause this
//                // code is so crazy complex. Someone please simplify it.)
//                break;
//            }
//        }
//
//        // At this point the current segment is at the same place as the current next layer node.
//
//        let current_pool_lane = current_segment.pool_lane;
//        let scratchpad = Vec::new();
//        loop {}
//    }
//}

use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::graph::NodeId;
use crate::common::node::Port;
use crate::common::node::XY;

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
///  The side which has only one SF simply will be have its port on W or E (in the middle).
///  The other side will have them all connected to the new dummy nodes, they are all expected to
///  go out at the top or bottom. But with the big caveat: Since we don't know really what of them
///  shall leave below or above the node, the dummy nodes are not enforced to be above or below
///  the gateway node. Consequently, some dummy node may be located "within" the gateway node,
///  and in that case the outgoing edge will be assigned the respective E or W.
///  
fn handle_nongateway_node(node_id: NodeId, graph: &mut Graph) {
    let this_node = &graph.nodes[node_id];
    let mut incoming_ports = Vec::<Port>::new();
    let mut outgoing_ports = Vec::<Port>::new();

    let mut above_inc = 0;
    let mut above_out = 0;
    let mut below_inc = 0;
    let mut below_out = 0;

    let mut has_vertical_edge_above = false;
    let mut has_vertical_edge_below = false;

    match &this_node.incoming[..] {
        [] => (),
        [single] => match is_vertical_edge(*single, graph, node_id) {
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
            match is_vertical_edge(*first, graph, node_id) {
                None => (),
                Some(VerticalEdgeDocks::Above) => {
                    above_inc = 1;
                    has_vertical_edge_above = true;
                }
                Some(VerticalEdgeDocks::Below) => {
                    unreachable!("Can't be")
                }
            }
            match is_vertical_edge(*last, graph, node_id) {
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

    match &this_node.outgoing[..] {
        [] => (),
        [single] => match is_vertical_edge(*single, graph, node_id) {
            None => {
                // A single boundary event usually is placed on the bottom side.
                if this_node.is_boundary_event(single) {
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
            match is_vertical_edge(*first, graph, node_id) {
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
            match is_vertical_edge(*last, graph, node_id) {
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
            for (rev_idx, (_, edge_id)) in it {
                if this_node.is_boundary_event(edge_id) {
                    // rev_idx starts at 0, but if the 0th element is on `below` we want to
                    // have `below_out == 1`.
                    below_out = rev_idx + 1;
                } else if graph.edges[edge_id].is_sequence_flow() {
                    break;
                }
            }
            for (_, (idx, edge_id)) in it {
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

    // now comes another hairy part:
}

fn handle_gateway_node(node_id: NodeId, graph: &mut Graph) {
    todo!();
}

enum VerticalEdgeDocks {
    Below,
    Above,
}

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
    let (start, end) = match from.pool_and_lane().cmp(&to.pool_and_lane()) {
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
                    } else {
                        if current_node == below {
                            return Some(VerticalEdgeDocks::Above);
                        } else {
                            return Some(VerticalEdgeDocks::Below);
                        }
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

    let mut node = Some(start);
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
                        && node.layer_id <= start.layer_id
                    {
                        if node.layer_id == start.layer_id && node.node_above_in_same_lane.is_none()
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
                if below == end.id {
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

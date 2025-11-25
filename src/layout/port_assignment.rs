//! BUGS:
//! * Message flow ports are never placed above or below ... They probably should if they are
//!   looping. Then they should leave/enter above/below.

use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::Edge;
use crate::common::edge::EdgeType;
use crate::common::edge::FlowType;
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
use crate::common::node::NodeType;
use crate::common::node::RelativePort;
use itertools::Itertools;
use proc_macros::{e, from, n, to};
use std::collections::HashSet;

pub fn port_assignment(graph: &mut Graph) {
    // First handle non-gateway nodes and then gateway nodes in two separate loops. This way,
    // `is_vertical_edge` is easier as it does not need to account for gateway bend dummies which
    // make everything harder (since the gateway node "disappears").
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
/// The North, East, West, and South ports, and then the four corner areas between them.
/// ("Corner area" in the sense that it is not just the corner point, but also the parts of the
/// edges connected to the corner up to the next N/E/W/S point.)
/// ` ┌─── N ───┐ `
/// ` │┌───────┐│  <top `
/// ` ││       ││ `
/// ` W│       │E  <W and E are always on the middle of left and right. `
/// ` ││       ││ `
/// ` │└───────┘│  <bottom `
/// ` └─── S ───┘ `
/// `  ^       ^ `
/// `  left    right `
/// `      ^ `
///       N and S can be shifted (only to the left I believe) to make room for more elements on the
///       sides. ("I believe" because from the `incoming` side everything else must be on "left" so
///       there cannot be other elements on the left of N or S. For `outgoing`, the "directly
///       up/down" edge is already placed to the very beginning or very end, so again all other
///       elements are surely placed right of N or S).
///
/// "Port": The `(x, y)` coordinate where an edge starts (or ends) on the boundary of the box.
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
/// `┌────────┐ current node `
/// `│        │ `
/// `│        │ `
/// `│        ├────────── `
/// `│        │ `
/// `│        │ `
/// `└──┬─┬─┬─┘ `
/// `   │ │ │ `
/// `   │ │ │ `
/// ` ┌────────┐ `
/// ` │ │ │ └──│────────── `
/// ` └────────┘ new dummy node `
/// `   │ │ `
/// ` ┌────────┐ `
/// ` │ │ └────│────────── `
/// ` └────────┘ new dummy node `
/// `   │ `
/// `   │ `
/// `   │ `
/// `   │ `
/// ` ┌───────┐ `
/// ` │       │  Some next neighbor `
/// ` │       │ `
/// ` │       │ `
/// ` │       │ `
/// ` └───────┘ `
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
    let this_layer = this_node.layer_id;
    let this_pool = this_node.pool;

    let top_barrier = top_barrier(graph, this_node_id);
    let bottom_barrier = bottom_barrier(graph, this_node_id);

    /************************************************************************/
    /*  PHASE 1: Gather all data about the ports for easier later analysis  */
    /************************************************************************/

    let mut incoming_ports = this_node
        .incoming
        .iter()
        .cloned()
        .map(|edge_id| {
            let edge = &e!(edge_id);
            let Coord3 {
                pool_and_lane: other_pool_lane,
                layer: other_layer,
                ..
            } = from!(edge_id).coord3();
            let flow_type = match edge.flow_type {
                FlowType::SequenceFlow => PortFlowType::Sequence,
                FlowType::DataFlow(_) => PortFlowType::Data,
                FlowType::MessageFlow(_) if other_pool_lane.pool < this_node.pool => {
                    PortFlowType::MessageAbove {
                        would_like_to_go_vertical: match &top_barrier {
                            None => true,
                            Some(barrier) => {
                                graph.interpool_y(other_pool_lane.pool, this_node.pool).0
                                    >= barrier.pool_and_lane.pool
                            }
                        },
                    }
                }
                FlowType::MessageFlow(_) => PortFlowType::MessageBelow {
                    would_like_to_go_vertical: match &bottom_barrier {
                        None => true,
                        Some(barrier) => {
                            graph.interpool_y(other_pool_lane.pool, this_node.pool).0
                                < barrier.pool_and_lane.pool
                        }
                    },
                },
            };

            let must_go_vertical = top_barrier
                .as_ref()
                .map(|barrier| {
                    if barrier.blocking_real_node == from!(edge_id).id {
                        Some(VerticalEdgeDocks::Above)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    bottom_barrier.as_ref().map(|barrier| {
                        if barrier.blocking_real_node == from!(edge_id).id {
                            Some(VerticalEdgeDocks::Below)
                        } else {
                            None
                        }
                    })
                })
                .flatten();

            PortInfo {
                is_incoming: true,
                other_pool_lane,
                other_layer,
                edge_id,
                coordinate: RelativePort { x: 0, y: 0 },
                must_go_vertical,
                flow_type,
                requires_bend_dummy: false,
                is_boundary_event: graph.attached_to_boundary_event(this_node_id, edge_id),
            }
        })
        .collect::<Vec<_>>();

    let mut outgoing_ports = this_node
        .outgoing
        .iter()
        .cloned()
        .map(|edge_id| {
            let edge = &e!(edge_id);
            let Coord3 {
                pool_and_lane: other_pool_lane,
                layer: other_layer,
                ..
            } = to!(edge_id).coord3();
            let flow_type = match edge.flow_type {
                FlowType::SequenceFlow => PortFlowType::Sequence,
                FlowType::DataFlow(_) => PortFlowType::Data,
                FlowType::MessageFlow(_) if this_node.pool < other_pool_lane.pool => {
                    PortFlowType::MessageBelow {
                        would_like_to_go_vertical: match &bottom_barrier {
                            None => true,
                            Some(barrier) => {
                                graph.interpool_y(this_node.pool, other_pool_lane.pool).0
                                    < barrier.pool_and_lane.pool
                            }
                        },
                    }
                }
                FlowType::MessageFlow(_) => PortFlowType::MessageAbove {
                    would_like_to_go_vertical: match &top_barrier {
                        None => true,
                        Some(barrier) => {
                            graph.interpool_y(this_node.pool, other_pool_lane.pool).0
                                >= barrier.pool_and_lane.pool
                        }
                    },
                },
            };

            let must_go_vertical = top_barrier
                .as_ref()
                .map(|barrier| {
                    if barrier.blocking_real_node == to!(edge_id).id {
                        Some(VerticalEdgeDocks::Above)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    bottom_barrier.as_ref().map(|barrier| {
                        if barrier.blocking_real_node == to!(edge_id).id {
                            Some(VerticalEdgeDocks::Below)
                        } else {
                            None
                        }
                    })
                })
                .flatten();

            PortInfo {
                is_incoming: false,
                other_pool_lane,
                other_layer,
                edge_id,
                coordinate: RelativePort { x: 0, y: 0 },
                must_go_vertical,
                flow_type,
                requires_bend_dummy: false,
                is_boundary_event: graph.attached_to_boundary_event(this_node_id, edge_id),
            }
        })
        .collect::<Vec<_>>();

    let top_and_bottom_merger = |inc: &&mut PortInfo, outg: &&mut PortInfo| {
        let inc_first = true;
        let outg_first = false;
        if inc.must_go_vertical.is_none()
            && matches!(inc.flow_type, PortFlowType::Sequence | PortFlowType::Data)
        {
            dbg!();
            // Incoming sequence flows and data flows always come first.
            return inc_first;
        }

        if outg.must_go_vertical.is_none()
            && matches!(outg.flow_type, PortFlowType::Sequence | PortFlowType::Data)
        {
            // Outgoing sequence flows and data flows always come last.
            return inc_first;
        }

        if !matches!(
            inc.flow_type,
            PortFlowType::MessageAbove { .. } | PortFlowType::MessageBelow { .. }
        )
            // Message flows bend on their first encountered inter-pool area. This means, for longer
        // message flows we have as a result weird ordering comparisons, but in the case where only
        // one space is crossed for the incoming node, then it is rather intuitive:
        || this_node.pool.0.abs_diff(inc.other_pool_lane.pool.0) == 1
        {
            return if inc.other_layer < outg.other_layer {
                inc_first
            } else if inc.other_layer > outg.other_layer {
                outg_first
            } else {
                // This could be untrue for when we get loops. But this does in practice mean we
                // have a neighboring node with an `incoming` and an `outgoing` flow of the same kind.
                // Which is not too surprising actually when writing out this comment ... happens
                // with sub states. But these asserts are here because `Edge::tc()` right now only
                // works for MessageFlows.
                assert!(e!(inc.edge_id).is_message_flow());
                assert!(e!(outg.edge_id).is_message_flow());
                if e!(inc.edge_id).tc().start < e!(outg.edge_id).tc().start {
                    inc_first
                } else {
                    outg_first
                }
            };
        }

        if inc.other_layer < this_layer {
            if outg.other_layer < this_layer {
                dbg!();
                return outg_first;
            } else {
                dbg!();
                return inc_first;
            }
        }

        if outg.other_layer <= this_layer {
            dbg!();
            inc_first
        } else {
            dbg!();
            outg_first
        }
    };

    let mut incoming = partition_ports(&mut incoming_ports);
    let mut outgoing = partition_ports(&mut outgoing_ports);
    let mut top = incoming
        .top
        .iter_mut()
        .rev()
        .merge_by(outgoing.top.iter_mut(), top_and_bottom_merger)
        .collect::<Vec<_>>();
    let mut bottom = incoming
        .bottom
        .iter_mut()
        .merge_by(outgoing.bottom.iter_mut().rev(), top_and_bottom_merger)
        .collect::<Vec<_>>();

    /********************************************************/
    /*  PHASE 2: Assign specific XY positions to each port  */
    /********************************************************/

    // At this point we know exactly how many ports are at what side, so we can do simple math
    // stuff to place the ports using `points_on_side`.
    // above_coordinates
    for (x, port_info) in points_on_side(this_node.width, top.len()).zip(top.iter_mut()) {
        port_info.coordinate = RelativePort { x, y: 0 };
    }
    // below_coordinates
    for (x, port_info) in points_on_side(this_node.width, bottom.len()).zip(bottom.iter_mut()) {
        port_info.coordinate = RelativePort {
            x,
            y: this_node.height,
        };
    }

    for (y, port_info) in partitioned_points_on_side(
        this_node.height,
        incoming.side_upper.len(),
        incoming.side_middle_sf.is_some(),
        incoming.side_lower.len(),
    )
    .zip(
        incoming
            .side_upper
            .iter_mut()
            .chain(incoming.side_middle_sf.as_deref_mut().into_iter())
            .chain(incoming.side_lower.iter_mut()),
    ) {
        port_info.coordinate = RelativePort { x: 0, y };
    }

    for (y, port_info) in partitioned_points_on_side(
        this_node.height,
        outgoing.side_upper.len(),
        outgoing.side_middle_sf.is_some(),
        outgoing.side_lower.len(),
    )
    .zip(
        outgoing
            .side_upper
            .iter_mut()
            .chain(outgoing.side_middle_sf.as_deref_mut().into_iter())
            .chain(outgoing.side_lower.iter_mut()),
    ) {
        port_info.coordinate = RelativePort {
            x: this_node.width,
            y,
        };
    }

    /***************************************************************************/
    /*  PHASE 3: Add additional dummy nodes & edges for above and below ports  */
    /***************************************************************************/

    let this_node = &n!(this_node_id);
    let this_node_width = this_node.width;
    assert!(DUMMY_NODE_WIDTH >= this_node_width);
    let relative_port_x = |x| (x + (DUMMY_NODE_WIDTH / 2)) - (this_node_width / 2);

    let requires_bend_dummies = |port_info: &&&mut PortInfo| port_info.requires_bend_dummy;

    let Coord3 {
        pool_and_lane: from_pool_and_lane,
        layer: from_layer,
        ..
    } = this_node.coord3();

    let bend_dummy_order_split_point_position = |port_info: &&mut PortInfo| {
        if matches!(
            port_info.flow_type,
            PortFlowType::MessageAbove { .. } | PortFlowType::MessageAbove { .. }
        ) && !port_info.is_incoming
        {
            port_info.other_layer > this_layer
        } else {
            port_info.other_layer >= this_layer
        }
    };

    let top_split_point = top
        .iter()
        .position(bend_dummy_order_split_point_position)
        .unwrap_or(top.len());
    let (top_left, top_right) = top.split_at(top_split_point);
    let bottom_split_point = bottom
        .iter()
        .position(bend_dummy_order_split_point_position)
        .unwrap_or(bottom.len());
    let (bottom_left, bottom_right) = bottom.split_at(bottom_split_point);
    // TODO technically this should do the same bendpoint-upwards-shifting strategy as the gateway
    // nodes already do. But this will take some time to implement, so use this approximation of it
    // for the moment.
    // TODO this is now wrong, as it should use `top` and itself understand where is the "middle".
    for port_info in top_left
        .iter()
        .rev()
        .chain(top_right.iter())
        .filter(requires_bend_dummies)
    {
        let bendpoint_pool_and_lane = if let Some(barrier) = &top_barrier {
            port_info.other_pool_lane.max(barrier.pool_and_lane)
        } else {
            port_info.other_pool_lane
        };

        let place = if from_pool_and_lane == bendpoint_pool_and_lane {
            PlaceForBendDummy::Above(this_node_id)
        } else {
            // In the target lane above, go to the lower end.
            PlaceForBendDummy::AtTheBottom
        };

        add_bend_dummy_node(
            graph,
            port_info.edge_id,
            bendpoint_pool_and_lane,
            from_layer,
            place,
            this_node_id,
            relative_port_x(port_info.coordinate.x),
            BendDummyKind::FromBoundaryEvent,
        );
    }
    // TODO this is now wrong, as it should use `top` and itself understand where is the "middle".
    for port_info in bottom_left
        .iter()
        .rev()
        .chain(bottom_right.iter())
        .filter(requires_bend_dummies)
    {
        let bendpoint_pool_and_lane = if let Some(barrier) = &bottom_barrier {
            port_info.other_pool_lane.min(barrier.pool_and_lane)
        } else {
            port_info.other_pool_lane
        };

        let place = if from_pool_and_lane == bendpoint_pool_and_lane {
            PlaceForBendDummy::Below(this_node_id)
        } else {
            // In the target lane above, go to the lower end.
            PlaceForBendDummy::AtTheTop
        };

        add_bend_dummy_node(
            graph,
            port_info.edge_id,
            bendpoint_pool_and_lane,
            from_layer,
            place,
            this_node_id,
            relative_port_x(port_info.coordinate.x),
            BendDummyKind::FromBoundaryEvent,
        );
    }

    // Now actually write the ports.
    n!(this_node_id).incoming_ports = incoming_ports
        .into_iter()
        .map(|x| x.coordinate)
        .collect::<Vec<_>>();
    n!(this_node_id).outgoing_ports = outgoing_ports
        .into_iter()
        .map(|x| x.coordinate)
        .collect::<Vec<_>>();
}

#[derive(Debug)]
enum PortFlowType {
    // The other end message flow is in an upwards pool (smaller PoolId).
    // `would_like_to_go_vertical`: true if it has free vertical room to the inter-pool where
    // it will bend. So, weaker than `must_go_vertical`, can be overridden.
    MessageAbove { would_like_to_go_vertical: bool },
    // The other end message flow is in a downwards pool (larger PoolId).
    // `would_like_to_go_vertical`: true if it has free vertical room to the inter-pool where
    // it will bend. So, weaker than `must_go_vertical`, can be overridden.
    MessageBelow { would_like_to_go_vertical: bool },
    Data,
    Sequence,
}

#[derive(Debug)]
struct PortInfo {
    edge_id: EdgeId,
    is_incoming: bool,
    // Position of the port on the boundary of the node. (Well, for gateways and events we
    // approximate a square form here. TODO This will be taken care of in some later stage as
    // otherwise the `node.port_is_left_or_right()` function becomes complicated.)
    coordinate: RelativePort,
    must_go_vertical: Option<VerticalEdgeDocks>,
    other_pool_lane: PoolAndLane,
    other_layer: LayerId,
    flow_type: PortFlowType,
    requires_bend_dummy: bool,
    is_boundary_event: bool,
}

struct PartitionedPortInfo<'a> {
    top: &'a mut [PortInfo],
    bottom: &'a mut [PortInfo],
    side_upper: &'a mut [PortInfo],
    side_lower: &'a mut [PortInfo],
    side_middle_sf: Option<&'a mut PortInfo>,
}

fn partition_ports<'a>(ports: &'a mut [PortInfo]) -> PartitionedPortInfo<'a> {
    let split_point = ports
        .iter()
        .rposition(|port_info| {
            match &port_info.must_go_vertical {
                Some(VerticalEdgeDocks::Below) => return false,
                Some(VerticalEdgeDocks::Above) => return true,
                None => (),
            }

            match &port_info.flow_type {
                PortFlowType::MessageAbove { .. } => true,
                PortFlowType::MessageBelow { .. } => false,
                // It was not a specifically downwards-going sequence flow, so it is going right.
                // Hence, we crossed the border from below-ports to above-ports.
                PortFlowType::Sequence => !port_info.is_boundary_event,
                // Not sure. This could just as well stay on the right side which is fine, `below`
                // will be split into `right-below` as well.
                PortFlowType::Data => false,
            }
        })
        .map(|x| x + 1)
        .unwrap_or(0);
    let (above_maybe_including_sf, below) = ports.split_at_mut(split_point);

    fn mf_that_is_fine_going_horizontal(port_info: &PortInfo) -> bool {
        matches!(port_info.flow_type, PortFlowType::MessageAbove { would_like_to_go_vertical} | PortFlowType::MessageBelow {would_like_to_go_vertical} if !would_like_to_go_vertical)
    }
    fn a_none_df(port_info: &PortInfo) -> bool {
        !matches!(port_info.flow_type, PortFlowType::Data)
    }

    let split_point = below
        .iter()
        .position(|port_info| port_info.is_boundary_event || port_info.must_go_vertical.is_some())
        .or_else(|| {
            below
                .iter()
                .rposition(mf_that_is_fine_going_horizontal)
                // That port should be right, so move the index one further.
                .map(|x| x + 1)
        })
        .or_else(|| below.iter().position(a_none_df))
        .unwrap_or(below.len());
    // Ensure `below` cannot be used after this statement. All should be done through the new
    // split.
    let (side_lower, bottom) = below.split_at_mut(split_point);

    // Actually not 100% sure whether this will be true with cycles. Then there could be two
    // outgoing sequence flows (one reversed) - but maybe at least one of them is modeled via
    // some other helper dummy construct to not leave directly to the right?
    let (side_middle_sf, above) = if let Some(last) = above_maybe_including_sf.last()
        && last.must_go_vertical.is_none()
        && matches!(last.flow_type, PortFlowType::Sequence)
        && !last.is_boundary_event
    {
        let (above, &mut [ref mut sf]) =
            above_maybe_including_sf.split_at_mut(above_maybe_including_sf.len() - 1)
        else {
            unreachable!();
        };
        (Some(sf), above)
    } else {
        (None, above_maybe_including_sf)
    };

    let split_point = above
        .iter()
        .rposition(|port_info| port_info.is_boundary_event || port_info.must_go_vertical.is_some())
        // That port should be left, so move the index one further.
        .map(|x| x + 1)
        .or_else(|| above.iter().position(mf_that_is_fine_going_horizontal))
        .or_else(|| above.iter().rposition(a_none_df).map(|x| x + 1))
        .unwrap_or(0);

    let (top, side_upper) = above.split_at_mut(split_point);

    {
        let mut requires_bend_dummy = false;
        for port_info in top.iter_mut() {
            if !requires_bend_dummy
                && (port_info.is_boundary_event
                    || matches!(port_info.flow_type, PortFlowType::MessageAbove { would_like_to_go_vertical } if !would_like_to_go_vertical))
            {
                requires_bend_dummy = true;
            }
            port_info.requires_bend_dummy = requires_bend_dummy;
        }

        let mut requires_bend_dummy = false;
        for port_info in bottom.iter_mut().rev() {
            if !requires_bend_dummy
                && (port_info.is_boundary_event
                    || matches!(port_info.flow_type, PortFlowType::MessageBelow { would_like_to_go_vertical } if !would_like_to_go_vertical))
            {
                requires_bend_dummy = true;
            }
            port_info.requires_bend_dummy = requires_bend_dummy;
        }
    }

    PartitionedPortInfo {
        top,
        bottom,
        side_upper,
        side_lower,
        side_middle_sf,
    }
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
    // TODO maybe it does not need to become detached? This is just done to make xy-ilp easier, but
    // that one could simply account for the gateway bend dummies correctly. Could have the rule
    // that we simply add all the gateway bend dummies below the gateway (or above, which is
    // arbitrary and without consequence, just needs to be consistent).
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

#[derive(Debug)]
enum VerticalEdgeDocks {
    Below,
    Above,
}

struct BarrierInfo {
    pool_and_lane: PoolAndLane,
    blocking_real_node: NodeId,
}

fn top_barrier(graph: &Graph, this_node_id: NodeId) -> Option<BarrierInfo> {
    let top_barrier = graph
        .iter_upwards_all_pools(StartAt::Node(this_node_id), None)
        .find(|&node| !node.is_long_edge_dummy())?;
    match &top_barrier.node_type {
        NodeType::BendDummy {
            originating_node, ..
        } => Some(BarrierInfo {
            pool_and_lane: top_barrier.pool_and_lane(),
            blocking_real_node: *originating_node,
        }),
        NodeType::RealNode { .. } => Some(BarrierInfo {
            pool_and_lane: top_barrier.pool_and_lane(),
            blocking_real_node: top_barrier.id,
        }),
        _ => unreachable!(),
    }
}

fn bottom_barrier(graph: &Graph, this_node_id: NodeId) -> Option<BarrierInfo> {
    let top_barrier = graph
        .iter_downwards_all_pools(StartAt::Node(this_node_id), None)
        .find(|&node| !node.is_long_edge_dummy())?;
    match &top_barrier.node_type {
        NodeType::BendDummy {
            originating_node, ..
        } => Some(BarrierInfo {
            pool_and_lane: top_barrier.pool_and_lane(),
            blocking_real_node: *originating_node,
        }),
        NodeType::RealNode { .. } => Some(BarrierInfo {
            pool_and_lane: top_barrier.pool_and_lane(),
            blocking_real_node: top_barrier.id,
        }),
        _ => unreachable!(),
    }
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
    points_before_midpoint: usize,
    has_midpoint: bool,
    points_after_midpoint: usize,
) -> impl Iterator<Item = usize> + Clone {
    // if has_midpoint {
    points_on_side(side_len / 2, points_before_midpoint)
        .chain(std::iter::once(side_len / 2))
        .chain(
            points_on_side(side_len / 2, points_after_midpoint)
                .map(move |point| point + side_len / 2),
        )
        .filter(move |_| has_midpoint)
        // } else {
        .chain(
            points_on_side(side_len, points_before_midpoint + points_after_midpoint)
                .filter(move |_| !has_midpoint),
        )
    // }
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
        .find(|&node| !node.is_long_edge_dummy())
        .map(|node| node.id);

    // This is always the same. We iterate always until we hit this one.
    let bottom_barrier = graph
        .iter_downwards_same_pool(StartAt::Node(this_node_id), Some(bottom_most_pool_lane))
        .find(|node| !node.is_long_edge_dummy())
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
        // nodes, then it should move to the better position.
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
    let is_reversed = cur_edge.is_reversed;
    match &cur_edge.edge_type {
        EdgeType::Regular { text, .. } => {
            let text = text.clone();
            cur_edge.edge_type = EdgeType::ReplacedByDummies {
                first_dummy_edge: EdgeId(current_num_edges),
                text,
            };

            // This value is actually unused, so I might have messed up the logic here.
            // Still here to have "complete" graph transformations.
            let attached_to_boundary_event1 = if graph.attached_to_boundary_event(from_id, edge_id)
            {
                e!(edge_id).attached_to_boundary_event.clone()
            } else {
                None
            };
            // This value is actually unused, so I might have messed up the logic here.
            // Still here to have "complete" graph transformations.
            let attached_to_boundary_event2 = if graph.attached_to_boundary_event(to_id, edge_id) {
                e!(edge_id).attached_to_boundary_event.clone()
            } else {
                None
            };

            // First remove the reference to the now-replaced edge, so there is certainly room
            // for the new edge references, i.e. no need to allocate accidentally.
            {
                // XXX Don't use `add_edge` here as we want to preserve the `outgoing`/`incoming`
                // ordering.
                let edge1 = EdgeId(graph.edges.len());
                graph.edges.push(Edge {
                    from: from_id,
                    to: dummy_node_id,
                    edge_type: EdgeType::DummyEdge {
                        original_edge: edge_id,
                        bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                    },
                    flow_type: flow_type.clone(),
                    is_vertical: reference_node == from_id,
                    is_reversed,
                    stroke_color: None,
                    stays_within_lane: n!(from_id).pool_and_lane() == pool_and_lane,
                    attached_to_boundary_event: attached_to_boundary_event1,
                });
                for outgoing_edge_idx in &mut n!(from_id).outgoing {
                    if *outgoing_edge_idx == edge_id {
                        *outgoing_edge_idx = edge1;
                        break;
                    }
                }
                n!(dummy_node_id).incoming.push(edge1);
            }
            {
                // XXX Don't use `add_edge` here as we want to preserve the `outgoing`/`incoming`
                // ordering.
                let edge2 = EdgeId(graph.edges.len());
                graph.edges.push(Edge {
                    from: dummy_node_id,
                    to: to_id,
                    edge_type: EdgeType::DummyEdge {
                        original_edge: edge_id,
                        bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                    },
                    flow_type,
                    is_vertical: reference_node == to_id,
                    is_reversed,
                    stroke_color: None,
                    stays_within_lane: pool_and_lane == n!(to_id).pool_and_lane(),
                    attached_to_boundary_event: attached_to_boundary_event2,
                });

                for incoming_edge_idx in &mut n!(to_id).incoming {
                    if *incoming_edge_idx == edge_id {
                        *incoming_edge_idx = edge2;
                        break;
                    }
                }

                n!(dummy_node_id).outgoing.push(edge2);
            }
        }
        EdgeType::DummyEdge { original_edge, .. } => {
            let original_edge = *original_edge;
            let attached_to_boundary_event = if !is_reversed {
                // Not `.clone()` but really `.take()`: `cur_edge` will be bent around, so it no
                // longer will be attached to the boundary event.
                cur_edge.attached_to_boundary_event.take()
            } else {
                None
            };

            let new_dummy_edge_id = {
                // XXX Don't use `add_edge` here as we want to preserve the `outgoing`/`incoming`
                // ordering.
                let new_dummy_edge_id = EdgeId(graph.edges.len());
                graph.edges.push(Edge {
                    from: from_id,
                    to: dummy_node_id,
                    edge_type: EdgeType::DummyEdge {
                        original_edge,
                        bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
                    },
                    flow_type: flow_type.clone(),
                    is_vertical: true,
                    is_reversed,
                    stroke_color: None,
                    stays_within_lane: n!(from_id).pool_and_lane() == pool_and_lane,
                    attached_to_boundary_event,
                });
                for outgoing_edge_idx in &mut n!(from_id).outgoing {
                    if *outgoing_edge_idx == edge_id {
                        *outgoing_edge_idx = new_dummy_edge_id;
                        break;
                    }
                }
                n!(dummy_node_id).incoming.push(new_dummy_edge_id);
                new_dummy_edge_id
            };
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

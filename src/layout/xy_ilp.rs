use std::cmp::Ordering;

use crate::common::config::Config;
use crate::common::edge::Edge;
use crate::common::edge::FlowType;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::graph::LaneId;
use crate::common::graph::MAX_NODE_HEIGHT;
use crate::common::graph::NodeId;
use crate::common::graph::PoolId;
use crate::common::graph::StartAt;
use crate::common::lane::Lane;
use crate::common::node::BendDummyKind;
use crate::common::node::Node;
use crate::common::node::NodePhaseAuxData;
use crate::common::node::NodeType;
use crate::common::node::classify_barrier_node_for_gateway;
use good_lp::solvers::SolverModel;
use good_lp::*;
use itertools::Either;
use itertools::Itertools;
use itertools::iproduct;
use proc_macros::e;
use proc_macros::from;
use proc_macros::n;
use proc_macros::to;

type PaddingVarsExpandedAux = (
    /* above */
    Vec<(
        Variable,
        /* y padding */ usize,
        /* above height */ usize,
    )>,
    /* below */
    Vec<(
        Variable,
        /* y padding */ usize,
        /* above height */ usize,
    )>,
);

#[derive(Debug)]
pub struct XyIlpNodeData {
    var: Variable,
    // Since gateway nodes have a confusing bunch of bend dummies of whom we don't know which is the
    // topmost, we record the gateway node itself, and from incoming and outgoing the top (and bottom).
    // So the gateway has up to three for above and below.
    // For easier writing of code (less confusing iterator golfing) the regular nodes use this same
    // construct, but for them the information is redundant with just `var`. But hey, easier code.
    // I hate code.
    padding_vars_expanded: PaddingVarsExpandedAux,
}

struct DiffVar {
    // Makes comparison easier.
    min_id: NodeId,
    max_id: NodeId,
    diff_var: Variable,
    edge_weight: f64,
    active: bool,
    edge_id: EdgeId,
    from_id: NodeId,
    to_id: NodeId,
}

// TODO should solve the ILP for every lane independently.
pub fn assign_xy_ilp(graph: &mut Graph) {
    let mut min_y_value = 0;
    let mut min_x_value = 0;
    for pool_idx in 0..graph.pools.len() {
        if !graph.pools[pool_idx].is_right_of_the_previous_pool {
            min_x_value = 0;
        } else {
            // Continue to grow to the right.
            // TODO this does not seem right.
            min_x_value += graph.config.pool_x_margin;
        }

        // Just store it for easier access.
        let pool_y = min_y_value;
        let pool_x = min_x_value;
        graph.pools[pool_idx].y = pool_y;
        graph.pools[pool_idx].x = pool_x;
        min_x_value += graph.config.pool_header_width;
        for lane_idx in 0..graph.pools[pool_idx].lanes.len() {
            let lane_y = min_y_value;
            graph.pools[pool_idx].lanes[lane_idx].y = lane_y;

            // Call the ILP to assign y values to nodes of this lane
            min_y_value += graph.config.lane_y_padding;
            let lane_internal_height =
                assign_y(graph, PoolId(pool_idx), LaneId(lane_idx), min_y_value);

            let lane_height = if lane_internal_height == 0 {
                graph.config.height_of_empty_lane
            } else {
                lane_internal_height + 2 * graph.config.lane_y_padding
            };

            graph.pools[pool_idx].lanes[lane_idx].height = lane_height;
            min_y_value = lane_y + lane_height;
        }
        assert!(min_y_value >= pool_y);
        if min_y_value == pool_y {
            min_y_value += graph.config.height_of_empty_pool;
            graph.pools[pool_idx].height = graph.config.height_of_empty_pool;
        } else {
            graph.pools[pool_idx].height = min_y_value - pool_y;
        };
        min_y_value += graph.config.vertical_space_between_pools;
    }

    assign_x(graph);
}

#[track_caller]
fn aux(node: &Node) -> Variable {
    match node.aux {
        NodePhaseAuxData::XyIlpNodeData(ref a) => a.var,
        _ => panic!("{node:#?}"),
    }
}

fn get_correct_padding_vars_node<'a>(graph: &'a Graph, node: &'a Node) -> &'a Node {
    match &node.node_type {
        NodeType::BendDummy {
            originating_node, ..
                // At some point the bend dummy will be used for non-gateway nodes as well.
        } => {
let originating_node = &n!(*originating_node);
if originating_node.is_gateway() && originating_node.pool_and_lane() == node.pool_and_lane() {
    originating_node } else { node }
            }
        _ => node,
    }
}

#[track_caller]
fn padding_vars_above<'a>(graph: &'a Graph, node: &'a Node) -> &'a [(Variable, usize, usize)] {
    match get_correct_padding_vars_node(graph, node).aux {
        NodePhaseAuxData::XyIlpNodeData(ref a) => &a.padding_vars_expanded.0,
        _ => panic!("{node:#?}"),
    }
}

#[track_caller]
fn padding_vars_below<'a>(graph: &'a Graph, node: &'a Node) -> &'a [(Variable, usize, usize)] {
    match get_correct_padding_vars_node(graph, node).aux {
        NodePhaseAuxData::XyIlpNodeData(ref a) => &a.padding_vars_expanded.1,
        _ => panic!("{node:#?}"),
    }
}

#[track_caller]
fn middle(node: &Node) -> Expression {
    aux(node) + (node.height / 2) as f64
}

const DEBUG_ILP_CONSTRUCTION: bool = false;

macro_rules! d {
    ($($tt:tt)*) => {{
        if DEBUG_ILP_CONSTRUCTION {
            $($tt)*
        }
    }};
}

#[track_caller]
fn c<T: SolverModel>(problem: &mut T, constraint: Constraint) {
    d! {
        let location = std::panic::Location::caller();
        println!("[line {}] {constraint:?}", location.line());
    }
    problem.add_constraint(constraint);
}

fn assign_y(graph: &mut Graph, pool: PoolId, lane: LaneId, min_y_value: usize) -> usize {
    d!(dbg!(&graph););
    let mut vars = variables!();
    let node_ids_iter = graph.pools[pool.0].lanes[lane.0].nodes.iter().cloned();

    // TODO the allocation could be moved out of this function.
    let mut diff_vars = Vec::new();

    let mut objective = Expression::from(0.0);
    // Keep the diagram height compact. Ensures that the solver does not stretch the diagram across
    // infinity.
    const HEIGHT_MINIMIZATION_FACTOR: f64 = 0.01;
    // NOT an integer variable!
    let height_minimization_var = vars.add(variable().min(0));
    objective += HEIGHT_MINIMIZATION_FACTOR * height_minimization_var;
    d!(eprintln!(
        "height_minimization_var (HMV) factor: {HEIGHT_MINIMIZATION_FACTOR}"
    ));

    fn y_padding(n: &Node, cfg: &Config) -> usize {
        if n.is_any_dummy() {
            cfg.dummy_node_y_padding
        } else {
            cfg.regular_node_y_padding
        }
    }

    // `var_idx_minus_one` has minus one because the 0th variable was created for
    // `height_minimization_var`.
    for (var_idx_minus_one, node_id) in node_ids_iter.clone().enumerate() {
        let node = &mut n!(node_id);
        assert!(node.pool == pool, "{pool:?}, {lane:?} -> {}", node);
        assert!(node.lane == lane, "{pool:?}, {lane:?} -> {}", node);
        assert!(node.height <= MAX_NODE_HEIGHT);

        // Make sure nodes are nicely aligned at the top.
        let min_y_value = min_y_value + (MAX_NODE_HEIGHT - node.height) / 2;
        // NOT an integer variable! Otherwise, this lead to unsatisfiable problems with nested
        // gateways (t0029.bpmd). We just round, this must be good enough.
        let var = vars.add(variable().min(min_y_value as f64));
        node.aux = NodePhaseAuxData::XyIlpNodeData(XyIlpNodeData {
            var,
            padding_vars_expanded: (
                vec![(var, y_padding(node, &graph.config), node.height)],
                vec![(var, y_padding(node, &graph.config), node.height)],
            ),
        });
        d!(eprintln!(
            "minimum y for n({} - ilp var v{}): {min_y_value}",
            node.id.0,
            var_idx_minus_one + 1
        ));
    }

    // Minimize the vertical length of edges. I.e. ideally as a result they go
    // directly to the right (have a vertical length of 0) and hence have no
    // bend points.
    for (edge_id, edge) in node_ids_iter
        .clone()
        .flat_map(|node_id| graph.nodes[node_id].outgoing.iter().cloned())
        .map(|edge| (edge, &graph.edges[edge]))
        .filter(|(_, edge)| edge.stays_within_lane)
    {
        assert!(!edge.is_replaced_by_dummies());

        // TODO gateway nodes seem to be picky about this height diff minimization in microlp, so
        // remove gateways if possible. But the bend-dummy and the other node should be fixed to the
        // same y coordinate as well, hence between those the diff stuff should be removed as well.
        let diff_var = vars.add(variable().min(0.0));

        let from_node = &n!(edge.from);
        let to_node = &n!(edge.to);
        // TODO verify if it is correct that only for bend dummies this is excluded.
        // There is the case of back-loops and S-bisect dummies, I just need to test them.
        // TODO #2 maybe just do a `continue` here? Or does they serve some purpose elsewhere?
        let active = !((from_node.is_gateway() || to_node.is_gateway())
            && (from_node.is_bend_dummy() || to_node.is_bend_dummy()));
        // Maybe it should just check for the same layer ...
        let is_same_layer = n!(edge.from).layer_id == n!(edge.to).layer_id;

        let mut edge_weight = match (edge.flow_type.clone(), edge.is_dummy(), is_same_layer) {
            (FlowType::SequenceFlow, false, _) => graph.config.short_sequence_flow_weight,
            (FlowType::SequenceFlow, true, false) => graph.config.long_sequence_flow_weight,
            (FlowType::SequenceFlow, true, true) => graph.config.same_layer_sequence_flow_weight,
            (FlowType::DataFlow(_), false, _) => graph.config.short_data_flow_weight,
            (FlowType::DataFlow(_), true, _)
                if n!(edge.from).is_data() && n!(edge.from).uses_half_layer =>
            {
                // We want to go the extra mile to ensure, if reasonable, that the next dummy edge
                // is at the same height as the data node. Only if this is the case can the data
                // node really be moved into the half-layer. Otherwise, it will be very awkward to
                // route the data edge out of the data element. Possible, but nothing which I want
                // to solve at the moment.
                1.5 * graph
                    .config
                    .long_data_edge_weight
                    .max(graph.config.short_data_flow_weight)
                    .max(graph.config.long_sequence_flow_weight)
                    .max(graph.config.short_sequence_flow_weight)
            }
            (FlowType::DataFlow(_), true, _) => graph.config.long_data_edge_weight,
            (FlowType::MessageFlow(_), _, _) => graph.config.message_edge_weight,
        };
        if edge.is_vertical && edge.is_message_flow() {
            // Vertical edges should also be kept short, but not at the expense of malaligning
            // the bend dummy node and the neighboring node.
            edge_weight *= 0.1;
        }
        diff_vars.push(DiffVar {
            edge_id,
            from_id: edge.from,
            to_id: edge.to,
            min_id: std::cmp::min(edge.from, edge.to),
            max_id: std::cmp::max(edge.from, edge.to),
            diff_var,
            active,
            edge_weight,
        });
        objective += diff_var * edge_weight;
    }

    // We want to balance gateway nodes between their outgoing/incoming branching nodes of the same
    // lane. For X gateways actually a better strategy would be to align it with the first `->` in
    // the BPMD, as this is likely the success case and should just go straight. But we will come
    // to that later.
    // In practice: Alignment is done between the outermost helper bend points whose targets stay in
    // the same lane. And alignment means that the gateway node is put precisely between the two nodes.
    // I tried to make this a little bit more flexible by using the |dist| with integration into
    // the objective, but that lead to problems with the solver. In practice however, in 90% of the
    // cases the precisely-in-the-middle visuals is what we are after anyway.
    let mut cached_constraints = Vec::new();
    let lane = &graph.pools[pool.0].lanes[lane.0];
    for node_id in node_ids_iter.clone() {
        let gateway = &graph.nodes[node_id];
        if !gateway.is_gateway() {
            continue;
        }
        let gateway_additional =
            handle_gateway(graph, lane, &mut vars, &mut cached_constraints, gateway);

        let NodePhaseAuxData::XyIlpNodeData(aux) = &mut &mut graph.nodes[node_id].aux else {
            unreachable!();
        };
        // Override this with the expanded gateway information.
        aux.padding_vars_expanded = gateway_additional;
    }

    println!("Num of vars: {}", vars.len());
    let mut problem = vars.minimise(objective).using(default_solver);

    // Add padding constraints between neighboring nodes.
    node_ids_iter
        .clone()
        .map(|node_id| &n!(node_id))
        .flat_map(|n| {
            n.node_below_in_same_lane
                .map(|next| (n, &graph.nodes[next.0]))
                .into_iter()
        })
        .filter(|(above, below)| {
            // We don't want to add padding constraints between a gateway and its own bend dummies,
            // as this is handled in the `handle_gateway` function.
            let above_gateway_id = if above.is_gateway() {
                above.id
            } else {
                match &above.node_type {
                    NodeType::BendDummy {
                        originating_node, ..
                    } if n!(*originating_node).is_gateway() => *originating_node,
                    _ => return true,
                }
            };
            let below_gateway_id = if below.is_gateway() {
                below.id
            } else {
                match &below.node_type {
                    NodeType::BendDummy {
                        originating_node, ..
                    } if n!(*originating_node).is_gateway() => *originating_node,
                    _ => return true,
                }
            };
            above_gateway_id != below_gateway_id
        })
        .for_each(|(above, below)| {
            iproduct!(
                padding_vars_below(graph, above),
                padding_vars_above(graph, below)
            )
            .for_each(
                |(&(above_aux, above_padding, above_height), &(below_aux, below_padding, _))| {
                    let padding = above_padding.max(below_padding);
                    c(
                        &mut problem,
                        (below_aux - above_aux).geq((above_height + padding) as f64),
                    );
                    d! {
                        let real_above = node_ids_iter
                            .clone()
                            .find(|n| aux(&n!(*n)) == above_aux)
                            .unwrap();
                        let real_below = node_ids_iter
                            .clone()
                            .find(|n| aux(&n!(*n)) == below_aux)
                            .unwrap();
                        eprintln!(
                            "padding above node({}) <dist {}> below node({})",
                            real_above.0, padding, real_below.0
                        );
                    };
                },
            );
        });

    node_ids_iter.clone().for_each(|node_id| {
        c(
            &mut problem,
            (aux(&n!(node_id)) + (&n!(node_id).height / 2) as f64)
                .into_expression()
                .leq(height_minimization_var),
        )
    });

    // Helper construct to resolve the minimization of |from.y - to.y|
    for DiffVar {
        diff_var,
        edge_weight,
        active,
        edge_id,
        from_id,
        to_id,
        ..
    } in diff_vars.iter()
    {
        if !active {
            //|| edge_id.0 == 28 || edge_id.0 == 1 {
            continue;
        }
        let from_node = &graph.nodes[*from_id];
        let to_node = &graph.nodes[*to_id];
        d!(eprintln!(
            "minimize edge y height: {edge_weight} * e({} | {} -> {} / \"{}\" -> \"{}\")",
            edge_id.0,
            from_id.0,
            to_id.0,
            from_node.display_text_or_dummy_kind(),
            to_node.display_text_or_dummy_kind(),
        ));
        let from_var = aux(from_node);
        let to_var = aux(to_node);
        let from_offset = if from_node.is_real() {
            let port = from_node.relative_port_of_outgoing(*edge_id);
            if from_node.relative_port_is_left_or_right(&port) {
                port.y as f64
            } else {
                (from_node.height / 2) as f64
            }
        } else {
            (from_node.height / 2) as f64
        };
        let to_offset = if to_node.is_real() {
            let port = to_node.relative_port_of_incoming(*edge_id);
            if to_node.relative_port_is_left_or_right(&port) {
                port.y as f64
            } else {
                (to_node.height / 2) as f64
            }
        } else {
            (to_node.height / 2) as f64
        };
        let from_y = from_var + from_offset;
        let to_y = to_var + to_offset as f64;
        c(&mut problem, (from_y.clone() - to_y.clone()).leq(diff_var));
        c(&mut problem, (to_y - from_y).leq(diff_var));
    }

    for constraint in cached_constraints {
        c(&mut problem, constraint);
    }

    let solution = problem.solve().unwrap();

    let mut min_y_encountered = usize::MAX;
    let mut max_y_plus_height_encountered = usize::MIN;
    for node_id in node_ids_iter.clone() {
        d!(eprintln!(
            "solution n({}) y: {} (non-rounded: {})",
            node_id.0,
            solution.value(aux(&n!(node_id))) as usize,
            solution.value(aux(&n!(node_id))),
        ));
    }
    for node_id in node_ids_iter.clone() {
        let node = &mut n!(node_id);
        node.y = solution.value(aux(node)) as usize;
        min_y_encountered = min_y_encountered.min(node.y);
        max_y_plus_height_encountered = max_y_plus_height_encountered.max(node.y + node.height);
    }

    // Check if the ILP actually assigned the lowest possible y value. This happens if the highest
    // nodes have a smaller height than MAX_NODE_HEIGHT. If not,
    // we need to manually shift all the nodes upwards a bit to fit correctly
    // into the available space ("pixel perfect").
    if min_y_encountered > min_y_value {
        let diff = min_y_encountered - min_y_value;
        lane.nodes.iter().for_each(|n| graph.nodes[n.0].y -= diff);
    }
    max_y_plus_height_encountered - min_y_encountered
}

fn assign_x(graph: &mut Graph) {
    // Note: Does not need to account for pools on the same horizontal line, since
    // this was already taken care for in the layer assignment phase.
    for node in graph.nodes.iter_mut() {
        node.x = graph.config.layer_center(node.layer_id) - node.width / 2;
    }
    for pool in &mut graph.pools {
        pool.width = graph.config.pool_width(graph.num_layers);
        for lane in &mut pool.lanes {
            lane.x = graph.config.pool_header_width;
            lane.width = pool.width - graph.config.pool_header_width;
        }
    }
}

/// Ignores vertical edges as they are handled separately.
enum GatewayNeighborLayerConnectivity<'a, I> {
    NoSameLaneEdges,
    OnlyOneSameLaneEdge(&'a Node),
    MultipleSameLaneEdges {
        top_node: &'a Node,
        in_between_nodes: I,
        bottom_node: &'a Node,
    },
}

fn analyse_gateway_neighbor_layer_connectivity<'a>(
    graph: &'a Graph,
    edges: &[EdgeId],
    from_or_to: impl Fn(&Edge) -> NodeId + Clone,
) -> GatewayNeighborLayerConnectivity<'a, impl Iterator<Item = &'a Node>> {
    // Walk inward from both ends until we have both matches or we exhaust the iterator.

    let target_node = move |edge_id: &EdgeId| -> Option<&Node> {
        let node = &n!(from_or_to(&e!(*edge_id)));
        if let NodeType::BendDummy {
            kind: BendDummyKind::FromGatewayToSameLane { .. },
            ..
        } = node.node_type
        {
            Some(node)
        } else {
            None
        }
    };

    // If only one match exists overall, mirror it so both are equal.
    let mut rev = edges.iter().rev();
    if let Some(first) = edges.iter().find_map(&target_node)
        && let Some(last) = rev.find_map(&target_node)
    {
        if !std::ptr::eq(first, last) {
            GatewayNeighborLayerConnectivity::MultipleSameLaneEdges {
                top_node: first,
                bottom_node: last,
                // No `filter` for layer_id required, as the in between nodes are guaranteed to be
                // non-vertical.
                in_between_nodes: edges
                    .iter()
                    .flat_map(target_node)
                    .take_while(|t: &&Node| !std::ptr::eq(*t, last)),
            }
        } else {
            GatewayNeighborLayerConnectivity::OnlyOneSameLaneEdge(first)
        }
    } else {
        GatewayNeighborLayerConnectivity::NoSameLaneEdges
    }
}

enum LoneElementPosition {
    Top,
    Side(NodeId),
    Bottom,
}
fn handle_gateway(
    graph: &Graph,
    lane: &Lane,
    vars: &mut ProblemVariables,
    cached_constraints: &mut Vec<Constraint>,
    gateway: &Node,
) -> PaddingVarsExpandedAux {
    let mut gateway_additional = (
        vec![(
            aux(gateway),
            graph.config.regular_node_y_padding,
            gateway.height,
        )],
        vec![(
            aux(gateway),
            graph.config.regular_node_y_padding,
            gateway.height,
        )],
    );

    let (top_slot, bottom_slot) = {
        let mut top_slot = None;
        let mut bottom_slot = None;
        // Doing the iteration is expensive, so only do it if necessary.
        let mut top_barrier = None;
        let top_barrier_calc = || {
            graph
                .iter_upwards_same_pool(StartAt::Node(gateway.id), Some(gateway.pool_and_lane()))
                .find_map(|node| classify_barrier_node_for_gateway(gateway.id, node))
        };
        let mut bottom_barrier = None;
        let bottom_barrier_calc = || {
            graph
                .iter_downwards_same_pool(StartAt::Node(gateway.id), Some(gateway.pool_and_lane()))
                .find_map(|node| classify_barrier_node_for_gateway(gateway.id, node))
        };

        enum WhichSlot {
            NoIdea,
            Top,
            Bottom,
        }

        // Find the top and bottom slots.
        // Regarding the `.filter` predicate in the end: Note that a gateway is only directly connected
        // to a real node if that real node is vertically connected. All other nodes are separated via
        // bend dummies. This also means that _all_ edges themselves are vertical, i.e. can't use that
        // as the filter criteria.
        let it = None
            .into_iter()
            .chain(gateway.incoming.first().map(|e| {
                (
                    if gateway.incoming.len() > 2 {
                        WhichSlot::Top
                    } else {
                        WhichSlot::NoIdea
                    },
                    &from!(*e),
                )
            }))
            .chain(gateway.incoming.last().map(|e| {
                (
                    if gateway.incoming.len() > 2 {
                        WhichSlot::Bottom
                    } else {
                        WhichSlot::NoIdea
                    },
                    &from!(*e),
                )
            }))
            .chain(gateway.outgoing.first().map(|e| {
                (
                    if gateway.outgoing.len() > 2 {
                        WhichSlot::Top
                    } else {
                        WhichSlot::NoIdea
                    },
                    &to!(*e),
                )
            }))
            .chain(gateway.outgoing.last().map(|e| {
                (
                    if gateway.outgoing.len() > 2 {
                        WhichSlot::Bottom
                    } else {
                        WhichSlot::NoIdea
                    },
                    &to!(*e),
                )
            }))
            .dedup_by(|left, right| left.1.id == right.1.id)
            .filter(|(_, node)| node.is_real());

        for (which_slot, node) in it {
            match which_slot {
                WhichSlot::Top => {
                    assert!(top_slot.is_none(), "{top_slot:?},\n{node:?},\n{graph:?}");
                    top_slot = Some(node.id);
                }
                WhichSlot::Bottom => {
                    assert!(
                        bottom_slot.is_none(),
                        "{bottom_slot:?},\n{node:?},\n{graph:?}"
                    );
                    bottom_slot = Some(node.id);
                }
                WhichSlot::NoIdea => match (top_slot.is_none(), bottom_slot.is_none()) {
                    (true, true) => match node.pool_and_lane().cmp(&gateway.pool_and_lane()) {
                        Ordering::Less => top_slot = Some(node.id),
                        Ordering::Greater => bottom_slot = Some(node.id),
                        Ordering::Equal => {
                            if top_barrier.is_none() {
                                top_barrier = Some(top_barrier_calc());
                            }
                            if Some(Some(node.id)) == top_barrier {
                                top_slot = Some(node.id);
                                continue;
                            }
                            if bottom_barrier.is_none() {
                                bottom_barrier = Some(bottom_barrier_calc());
                            }
                            if Some(Some(node.id)) == bottom_barrier {
                                bottom_slot = Some(node.id);
                                continue;
                            }
                        }
                    },
                    (false, true) => {
                        bottom_slot = Some(node.id);
                        assert!(
                            node.pool_and_lane() >= gateway.pool_and_lane(),
                            "{top_slot:?},{bottom_slot:?},\n{node:?},\n{graph:?}"
                        );
                    }
                    (true, false) => {
                        top_slot = Some(node.id);
                        assert!(
                            node.pool_and_lane() <= gateway.pool_and_lane(),
                            "{top_slot:?},{bottom_slot:?},\n{node:?},\n{graph:?}"
                        );
                    }
                    (false, false) => {
                        panic!(
                            "Three vertical nodes??? {top_slot:?},{bottom_slot:?},\n{node:?},\n{graph:?}"
                        );
                    }
                },
            }
        }

        (top_slot, bottom_slot)
    };

    let (top_slot_is_data, bottom_slot_is_data) = (
        top_slot.is_some_and(|node_id| n!(node_id).is_data()),
        bottom_slot.is_some_and(|node_id| n!(node_id).is_data()),
    );

    // Lone non-data element.
    // Note: If the lone element is at the top or bottom, then it does say so in both the left and
    // right variable.
    let (left_lone_element_position, right_lone_element_position) = {
        let mut inc_count = 0;
        let mut outg_count = 0;
        let mut last_inc = None;
        let mut last_outg = None;
        for (position, other_node_id, is_incoming) in gateway
            .incoming
            .iter()
            .enumerate()
            .map(|(position, edge_id)| (position, &e!(*edge_id)))
            .flat_map(|(position, edge)| {
                edge.is_sequence_flow().then_some((
                    Either::Left(position),
                    edge.from,
                    !edge.is_reversed,
                ))
            })
            .chain(
                gateway
                    .outgoing
                    .iter()
                    .enumerate()
                    .map(|(position, edge_id)| (position, &e!(*edge_id)))
                    .flat_map(|(position, edge)| {
                        edge.is_sequence_flow().then_some((
                            Either::Right(position),
                            edge.to,
                            edge.is_reversed,
                        ))
                    }),
            )
        {
            if is_incoming {
                inc_count += 1;
                last_inc = Some((position, other_node_id));
            } else {
                outg_count += 1;
                last_outg = Some((position, other_node_id));
            }
        }

        let mut lone_element = None;
        let left_lone_element_position = 'block: {
            let last_some = if inc_count == 1 {
                last_inc.unwrap()
            } else if outg_count == 1 {
                last_outg.unwrap()
            } else {
                break 'block None;
            };
            if let (Either::Left(position), other_node_id) = last_some {
                lone_element = Some(other_node_id);
                if Some(other_node_id) == top_slot {
                    Some(LoneElementPosition::Top)
                } else if Some(other_node_id) == bottom_slot {
                    Some(LoneElementPosition::Bottom)
                } else {
                    Some(LoneElementPosition::Side(other_node_id))
                }
            } else {
                None
            }
        };
        let right_lone_element_position = 'block: {
            let last_some = if inc_count == 1 {
                last_inc.unwrap()
            } else if outg_count == 1 {
                last_outg.unwrap()
            } else {
                break 'block None;
            };
            if let (Either::Right(position), other_node_id) = last_some {
                lone_element = Some(other_node_id);
                if Some(other_node_id) == top_slot {
                    Some(LoneElementPosition::Top)
                } else if Some(other_node_id) == bottom_slot {
                    Some(LoneElementPosition::Bottom)
                } else {
                    Some(LoneElementPosition::Side(other_node_id))
                }
            } else {
                None
            }
        };

        match (left_lone_element_position, right_lone_element_position) {
            (Some(LoneElementPosition::Top), _) | (_, Some(LoneElementPosition::Top)) => (
                Some(LoneElementPosition::Top),
                Some(LoneElementPosition::Top),
            ),
            (Some(LoneElementPosition::Bottom), _) | (_, Some(LoneElementPosition::Bottom)) => (
                Some(LoneElementPosition::Bottom),
                Some(LoneElementPosition::Bottom),
            ),
            a => a,
        }
    };

    let (top_is_blocked_for_non_lones, bottom_is_blocked_for_non_lones) =
        match left_lone_element_position {
            Some(LoneElementPosition::Top) => (true, bottom_slot_is_data),
            Some(LoneElementPosition::Bottom) => (top_slot_is_data, true),
            _ => (top_slot_is_data, bottom_slot_is_data),
        };

    handle_gateway_neighbor_layer_connectivity(
        graph,
        lane,
        vars,
        cached_constraints,
        gateway,
        &gateway.incoming,
        |edge| edge.from,
        left_lone_element_position,
        top_is_blocked_for_non_lones,
        bottom_is_blocked_for_non_lones,
        top_slot,
        bottom_slot,
        top_slot_is_data,
        bottom_slot_is_data,
        &mut gateway_additional,
    );

    handle_gateway_neighbor_layer_connectivity(
        graph,
        lane,
        vars,
        cached_constraints,
        gateway,
        &gateway.outgoing,
        |edge| edge.to,
        right_lone_element_position,
        top_is_blocked_for_non_lones,
        bottom_is_blocked_for_non_lones,
        top_slot,
        bottom_slot,
        top_slot_is_data,
        bottom_slot_is_data,
        &mut gateway_additional,
    );
    gateway_additional
}

fn handle_gateway_neighbor_layer_connectivity(
    graph: &Graph,
    lane: &Lane,
    vars: &mut ProblemVariables,
    cached_constraints: &mut Vec<Constraint>,
    gateway: &Node,
    edges: &[EdgeId],
    from_or_to: impl Fn(&Edge) -> NodeId + Clone,
    lone_element_position: Option<LoneElementPosition>,
    top_is_blocked_for_non_lones: bool,
    bottom_is_blocked_for_non_lones: bool,
    top_slot: Option<NodeId>,
    bottom_slot: Option<NodeId>,
    top_slot_is_data: bool,
    bottom_slot_is_data: bool,
    gateway_additional: &mut PaddingVarsExpandedAux,
) {
    let mut first_other = None;
    let mut last_other = None;
    let inc_iter = edges
        .iter()
        .map(|edge_id| &e!(*edge_id))
        .map(|edge| (edge, from_or_to(edge)))
        .filter(|&(edge, other_node_id)| {
            Some(other_node_id) != top_slot
                && Some(other_node_id) != bottom_slot
                && edge.is_sequence_flow()
        })
        .map(|(_, other_node_id)| &n!(other_node_id))
        .inspect(|other| {
            last_other = Some(*other);
            first_other.get_or_insert(*other);
        });

    match (
        &lone_element_position,
        top_is_blocked_for_non_lones,
        bottom_is_blocked_for_non_lones,
    ) {
        (Some(LoneElementPosition::Side(other_node_id)), _, _) => {
            inc_iter.fold(false, |lone_encountered, cur| {
                if *other_node_id == cur.id {
                    assert!(gateway.pool_and_lane() == cur.pool_and_lane());
                    if cur.is_bend_dummy() {
                        cached_constraints.push((middle(gateway) - middle(cur)).eq(0.0));
                        d!(eprintln!(
                            "gateway fix lone bend node to same y coordinate: gateway node({}) - bend node({}) / \"{}\" - \"{}\"",
                            gateway.id.0,
                            cur.id.0,
                            gateway.display_text_or_dummy_kind(),
                            cur.display_text_or_dummy_kind()
                        ));
                    } else {
                        // In this case we have a loop lone element, i.e. `cur` is somewhere
                        // above or below the gateway node. Cannot fix it to the same y coordinate.
                        assert!(cur.is_back_edge_corner_dummy());
                    }
                    true // set `lone_encountered := true`
                } else if !lone_encountered {
                    assert!(!top_slot_is_data); // Graph validation insufficient.
                    if gateway.pool_and_lane() == cur.pool_and_lane() {
                        cached_constraints.push((middle(gateway) - middle(cur)).geq(graph.config.min_vertical_space_between_gateway_bendpoints as f64 / 2.0));
                        d!(eprintln!(
                            "gateway below non-lone bend node: gateway node({}) - bend node({}) / \"{}\" - \"{}\"",
                            gateway.id.0,
                            cur.id.0,
                            gateway.display_text_or_dummy_kind(),
                            cur.display_text_or_dummy_kind()
                        ));
                    }
                    lone_encountered
                } else {
                    assert!(!bottom_slot_is_data); // Graph validation insufficient.
                    if gateway.pool_and_lane() == cur.pool_and_lane() {
                        cached_constraints.push((middle(cur) - middle(gateway)).geq(graph.config.min_vertical_space_between_gateway_bendpoints as f64 / 2.0));
                        d!(eprintln!(
                            "gateway above non-lone bend node: gateway node({}) - bend node({}) / \"{}\" - \"{}\"",
                            gateway.id.0,
                            cur.id.0,
                            gateway.display_text_or_dummy_kind(),
                            cur.display_text_or_dummy_kind()
                        ));
                    }
                    lone_encountered
                }
            });
        }
        (_, true, true) => {
            inc_iter.enumerate().for_each(|(nr, cur)| {
                // Assert: Otherwise bend dummy placement logic is flawed, should not have pushed
                // the bend dummy into another lane if we cannot leave from the top/bottom at all.
                assert!(gateway.pool_and_lane() == cur.pool_and_lane());
                cached_constraints.push((middle(gateway) - middle(cur)).eq(0.0));
                d!(eprintln!(
                    "gateway fix non-lone bend node to same y: gateway node({}) - bend node({}) / \"{}\" - \"{}\"",
                    gateway.id.0,
                    cur.id.0,
                    gateway.display_text_or_dummy_kind(),
                    cur.display_text_or_dummy_kind()
                ));

                if nr == 1 {
                    // Already second edge. But also only print on the second edge to not spam.
                    eprintln!(
                        "Gateway {} is forced to have multiple edges on one side",
                        gateway.display_text_or_dummy_kind()
                    );
                }
            });
        }
        (_, true, false) | (_, false, true) => {
            inc_iter.fold(Option::<&Node>::None, |prev, cur| {
                if top_is_blocked_for_non_lones {
                    if gateway.pool_and_lane() == cur.pool_and_lane() {
                        cached_constraints.push((middle(gateway) - middle(cur)).leq(0.0));
                        d!(eprintln!(
                            "gateway below non-lone bend node: gateway node({}) - bend node({}) / \"{}\" - \"{}\"",
                            gateway.id.0,
                            cur.id.0,
                            gateway.display_text_or_dummy_kind(),
                            cur.display_text_or_dummy_kind()
                        ));
                    } else {
                        // Assert: Otherwise bend dummy placement logic is flawed, should not have pushed
                        // the bend dummy into another lane if we cannot leave from the top/bottom at all.
                        assert!(cur.pool_and_lane() > gateway.pool_and_lane());
                    }
                } else {
                    if gateway.pool_and_lane() == cur.pool_and_lane() {
                        cached_constraints.push((middle(gateway) - middle(cur)).geq(0.0));
                        d!(eprintln!(
                            "gateway above non-lone bend node: gateway node({}) - bend node({}) / \"{}\" - \"{}\"",
                            gateway.id.0,
                            cur.id.0,
                            gateway.display_text_or_dummy_kind(),
                            cur.display_text_or_dummy_kind()
                        ));
                    } else {
                        // Assert: Otherwise bend dummy placement logic is flawed, should not have pushed
                        // the bend dummy into another lane if we cannot leave from the top/bottom at all.
                        assert!(cur.pool_and_lane() < gateway.pool_and_lane());
                    }
                }
                if let Some(prev) = prev && cur.pool_and_lane() == prev.pool_and_lane() && cur.pool_and_lane() == gateway.pool_and_lane() {
                    cached_constraints.push(
                        (middle(cur) - middle(prev)).geq(graph.config.dummy_node_y_padding as f64),
                    );
                    d!(eprintln!(
                        "padding above node({}) <dist {}> below node({})",
                        prev.id.0, graph.config.dummy_node_y_padding, cur.id.0
                    ));
                }
                Some(cur)
            });
        }
        (_, false, false) => {
            inc_iter.fold(Option::<&Node>::None, |prev, cur| {
                if let Some(prev) = prev
                    && prev.pool_and_lane() == cur.pool_and_lane()
                {
                    cached_constraints.push(
                        (middle(cur) - middle(prev)).geq(graph.config.dummy_node_y_padding as f64),
                    );
                    d!(eprintln!(
                        "padding above node({}) <dist {}> below node({})",
                        prev.id.0, graph.config.dummy_node_y_padding, cur.id.0
                    ));
                }
                Some(cur)
            });
        }
    }

    if let Some(first_other) = first_other.take()
        && first_other.pool_and_lane() == gateway.pool_and_lane()
    {
        if first_other.is_bend_dummy() {
            gateway_additional.0.push((
                aux(first_other),
                graph.config.dummy_node_y_padding,
                first_other.height,
            ));
        } else {
            assert!(
                first_other.is_back_edge_corner_dummy(),
                "{graph:?},\nfirst_other: {}, top_slot: {top_slot:?}, bottom_slot: {bottom_slot:?}",
                first_other.id,
            );
        }
    }

    if let Some(last_other) = last_other.take()
        && last_other.pool_and_lane() == gateway.pool_and_lane()
    {
        if last_other.is_bend_dummy() {
            gateway_additional.1.push((
                aux(last_other),
                graph.config.dummy_node_y_padding,
                last_other.height,
            ));
        } else {
            assert!(
                last_other.is_back_edge_corner_dummy(),
                "{graph:?},\nlast_other: {}, top_slot: {top_slot:?}, bottom_slot: {bottom_slot:?}",
                last_other.id,
            );
        }
    }

    if top_is_blocked_for_non_lones && bottom_is_blocked_for_non_lones {
        // The non-lones on this side are then already forced to the top or bottom half. No further
        // constraints are necessary (would only result in conflicts).
        return;
    }

    if matches!(&lone_element_position, Some(LoneElementPosition::Side(_))) {
        // The lone element is on this side, hence we don't enforce any further constraints.
        // It *might* be that we should still add a small pull factor between the gateway and its
        // staying-within-lane bend points, but let's first see how in practice it looks like.
        return;
    }

    match analyse_gateway_neighbor_layer_connectivity(graph, edges, from_or_to) {
        GatewayNeighborLayerConnectivity::NoSameLaneEdges => {}
        GatewayNeighborLayerConnectivity::OnlyOneSameLaneEdge(node) => {
            // The bend node shall stay on the same height as the gateway node, so the edge leaves
            // nicely at the right corner of the gateway symbol.
            cached_constraints.push((middle(gateway) - middle(node)).eq(0.0));

            //let id1 = std::cmp::min(gateway.id, node.id);
            //let id2 = std::cmp::max(gateway.id, node.id);
            // Activate the diff_var for this edge. Previously it was deactivated because it was
            // connected to a gateway, but it is the only edge.
            //diff_vars
            //    .iter_mut()
            //    .find(|diff_var| diff_var.min_id == id1 && diff_var.max_id == id2)
            //    .unwrap()
            //    .active = true;
            d!(eprintln!(
                "gateway fix lone same-lane bend node to same y coordinate: gateway node({}) - bend node({}) / \"{}\" - \"{}\"",
                gateway.id.0,
                node.id.0,
                gateway.display_text_or_dummy_kind(),
                node.display_text_or_dummy_kind()
            ));
            // TODO in principle it would be cool to make the connected edge a bit less rigid. The
            // other side of the gateway is expected to be branching, and to allow the gateway node
            // to be positioned better, without disrupting the rest of the layout, it might be good
            // to detach it from `node`.
        }
        GatewayNeighborLayerConnectivity::MultipleSameLaneEdges {
            top_node,
            in_between_nodes,
            bottom_node,
        } => {
            if !top_is_blocked_for_non_lones && !bottom_is_blocked_for_non_lones {
                // gateway == (top_node + bottom_node) / 2 <==> 2 * gateway - top_node - bottom_node == 0
                cached_constraints
                    .push((2.0 * middle(gateway) - middle(top_node) - middle(bottom_node)).eq(0.0));

                d!(eprintln!(
                    "gateway balance between top node({0}) - gateway node({1}) - bottom node({2}) (and distance between {0} and {2} > {3})",
                    top_node.id.0,
                    gateway.id.0,
                    bottom_node.id.0,
                    graph.config.min_vertical_space_between_gateway_bendpoints
                ));

                // An additional constraint to ensure that the branches are not too close to the gateway
                // node, otherwise it looks awkward.
                cached_constraints.push(
                    (aux(top_node)
                        + graph.config.min_vertical_space_between_gateway_bendpoints as f64)
                        .leq(aux(bottom_node)),
                );
            } else {
                let (above_node, below_node) = match (
                    top_is_blocked_for_non_lones,
                    bottom_is_blocked_for_non_lones,
                ) {
                    (true, false) => (gateway, top_node),
                    (false, true) => (bottom_node, gateway),
                    _ => unreachable!("Conditions already checked above (even above the match)."),
                };
                // This might be too restrictive. But the idea is that the gateway might look
                // awkward if no edge goes to the right if it were possible. Must check in practice.
                // Alternative would be to put a larger penalty on this edge's length than on other
                // edges, thus only nudging the ILP to shorten it.
                // But with the current forced same height we don't need the rest of the constraints
                // which are just there to put the intermediate nodes not at an awkward position
                // with respect to gateway (since now they are all pushed to the side anyway).
                cached_constraints.push((middle(above_node) - middle(below_node)).eq(0.0));
                d!(eprintln!(
                    "gateway and bend dummy forced on same y: gateway node({}) - other node({})",
                    gateway.id.0,
                    if above_node.id == gateway.id {
                        below_node.id.0
                    } else {
                        above_node.id.0
                    },
                ));
                return;
            }

            let min = graph.config.min_vertical_space_between_gateway_bendpoints / 2;
            let max = lane.nodes.len()
                * (MAX_NODE_HEIGHT
                    + std::cmp::max(
                        graph.config.regular_node_y_padding,
                        graph.config.dummy_node_y_padding,
                    ));
            let mut in_between_count = 0;
            for in_between_node in in_between_nodes {
                in_between_count += 1;
                let z0 = vars.add(variable().binary());
                let zp = vars.add(variable().binary());
                let zm = vars.add(variable().binary());
                gateway_forbidden_offset_constraints(
                    cached_constraints,
                    gateway,
                    in_between_node,
                    min,
                    max,
                    z0,
                    zp,
                    zm,
                );
            }
            if in_between_count > 0 {
                d!(eprintln!(
                    "gateway forbidden offset constraint for gateway node {} with {} other intermediate bend nodes",
                    gateway.id, in_between_count
                ));
            }
        }
    }
}

fn gateway_forbidden_offset_constraints(
    cached_constraints: &mut Vec<Constraint>,
    gateway: &Node,
    other_node: &Node,
    min: usize,
    max: usize,
    z0: Variable,
    zp: Variable,
    zm: Variable,
) {
    // Gateway neighbors: Ensure that the target node of a gateway is either exactly at the same
    // height (so the edge is straight to the right) or sufficiently offset to the top or bottom
    // such that the connecting edge can leave to the top or bottom of the gateway node.
    // Otherwise, it will result in diagonal edges (which is only for gateway nodes due to the
    // collapsing logic (`VerticalCollapsed`)).
    // How it works:
    // First introduce helper variables:
    //  (1) `min := (min_vertical_space_between_gateway_bendpoints/2)`
    //  (2) `max := num_nodes * greatest distance * vertical_space`
    //  (3) `dist := ((aux(gateway) + (gateway.height / 2) as f64) - (aux(first) + (first.height / 2) as f64))`
    //
    // The goal is either:
    //  * `dist == 0` or
    //  * `|dist| >= min`
    //
    // This can be achieved with the big M method (ChatGPT helped me here!) using two constraints:
    //  (1) `d >= 0 * z0 + min * z+ +   L * z-`
    //  (2) `d <= 0 * z0 +   U * z+ - min * z-`
    //  (3) `z0 + z+ + z- == 1`: To choose just one of the three modes.
    //
    // The `z0`, `z+` and `z-` binary variables are helper variables to select the mode of the ILP.
    // Replacing constants `L := -max`, `U := max`, and eliminating the redundant `0*z0`:
    //  (1) `d >= min * z+ - max * z-`
    //  (2) `d <= max * z+ - min * z-`
    //
    // gives us:
    //  (1) `z0 == 1 ==> d >= 0 && d <= 0`, i.e. `d \in [0, 0]`
    //  (1) `z+ == 1 ==> d >= min && d <= max`, i.e. `d \in [min, max]`
    //  (1) `z- == 1 ==> d >= -max && d <= -min`, i.e. `d \in [-max, -min]`

    cached_constraints.push((z0 + zp + zm).eq(1));

    //  (1) `d >= min * z+ - max * z-`
    cached_constraints
        .push((middle(gateway) - middle(other_node)).geq((min as f64) * zp - (max as f64) * zm));
    //  (2) `d <= max * z+ - min * z-`
    cached_constraints
        .push((middle(gateway) - middle(other_node)).leq((max as f64) * zp - (min as f64) * zm));
}

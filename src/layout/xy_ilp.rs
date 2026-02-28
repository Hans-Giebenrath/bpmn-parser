use crate::common::config::Config;
use crate::common::edge::FlowType;
use crate::common::graph::Graph;
use crate::common::graph::LaneId;
use crate::common::graph::MAX_NODE_HEIGHT;
use crate::common::graph::NodeId;
use crate::common::graph::PoolId;
use crate::common::lane::Lane;
use crate::common::node::BendDummyKind;
use crate::common::node::Node;
use crate::common::node::NodeIdOrEdgeId;
use crate::common::node::NodePhaseAuxData;
use crate::common::node::NodeType;
use core::slice::Iter;
use good_lp::solvers::SolverModel;
use good_lp::*;
use proc_macros::from;
use proc_macros::n;
use proc_macros::to;
use std::iter::Cloned;

#[derive(Debug)]
pub struct XyIlpNodeData {
    var: Variable,
}

/// See `gateway_forbidden_offset_constraints_inner` for documentation.
struct GatewayBigMHelpers {
    z0: Variable,
    zp: Variable,
    zm: Variable,
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

fn c<T: SolverModel>(problem: &mut T, constraint: Constraint) {
    //problem.add_constraint(dbg!(constraint));
    problem.add_constraint(constraint);
}

const DEBUG_ILP_CONSTRUCTION: bool = true;

macro_rules! d {
    ($($tt:tt)*) => {{
        if DEBUG_ILP_CONSTRUCTION {
            $($tt)*
        }
    }};
}

fn assign_y(graph: &mut Graph, pool: PoolId, lane: LaneId, min_y_value: usize) -> usize {
    d!(dbg!(&graph););
    let mut vars = variables!();
    let node_ids_iter = graph.pools[pool.0].lanes[lane.0].nodes.iter().cloned();

    // TODO the allocation could be moved out of this function.
    let mut diff_vars = Vec::new();
    let mut gateway_big_ms = Vec::new();

    let mut objective = Expression::from(0.0);
    // Keep the diagram height compact. Ensures that the solver does not stretch the diagram across
    // infinity.
    const HEIGHT_MINIMIZATION_FACTOR: f64 = 0.0001;
    let height_minimization_var = vars.add(variable().integer().min(0));
    objective += HEIGHT_MINIMIZATION_FACTOR * height_minimization_var;
    d!(eprintln!(
        "height_minimization_var (HMV) factor: {HEIGHT_MINIMIZATION_FACTOR}"
    ));

    // Must cancel out the PULL_UP_FACTOR, and then push further, but not so much as to move the
    // other nodes back down to infinity. I.e. must be smaller than Specifically. g0011.bpmd upper
    // gateway the edge should leave downward instead of to the side.
    //const PUSH_GATEWAY_BENDPOINT_DOWN_FACTOR: f64 = 0.01 * HEIGHT_MINIMIZATION_FACTOR;

    // Note: One could argue that this ordering should be part of the graph. But it turns out that
    // this is not necessary in any other phase, just here, so we can compute it just locally.
    // Keeps the common graph types less cluttered.
    //let mut all_nodes: HashMap</*layer*/ LayerId, NodeId> = HashMap::new();
    for node_id in node_ids_iter.clone() {
        let node = &mut n!(node_id);
        assert!(node.pool == pool, "{pool:?}, {lane:?} -> {}", node);
        assert!(node.lane == lane, "{pool:?}, {lane:?} -> {}", node);
        assert!(node.height <= MAX_NODE_HEIGHT);

        // Make sure nodes are nicely aligned at the top.
        let min_y_value = min_y_value + (MAX_NODE_HEIGHT - node.height) / 2;
        let var = vars.add(variable().integer().min(min_y_value as f64));
        node.aux = NodePhaseAuxData::XyIlpNodeData(XyIlpNodeData { var });
        d!(eprintln!("minimum y for n({}): {min_y_value}", node.id.0));
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

        let diff_var = vars.add(variable().min(0.0));
        diff_vars.push((edge.from.0, edge.to.0, diff_var));
        // Maybe it should just check for the same layer ...
        let is_same_layer = n!(edge.from).layer_id == n!(edge.to).layer_id;

        let mut edge_weight = match (edge.flow_type.clone(), edge.is_dummy(), is_same_layer) {
            (FlowType::SequenceFlow, false, _) => graph.config.short_sequence_flow_weight,
            (FlowType::SequenceFlow, true, false) => graph.config.long_sequence_flow_weight,
            (FlowType::SequenceFlow, true, true) => graph.config.same_layer_sequence_flow_weight,
            (FlowType::DataFlow(_), false, _) => graph.config.short_data_flow_weight,
            (FlowType::DataFlow(_), true, _) => graph.config.long_data_edge_weight,
            (FlowType::MessageFlow(_), _, _) => graph.config.message_edge_weight,
        };
        if edge.is_vertical && edge.is_message_flow() {
            // Vertical edges should also be kept short, but not at the expense of malaligning
            // the bend dummy node and the neighboring node.
            edge_weight *= 0.1;
        }
        objective += diff_var * edge_weight;
        d!(eprintln!(
            "minimize edge y height: {edge_weight} * e({} | {} -> {} / \"{}\" -> \"{}\")",
            edge_id.0,
            edge.from.0,
            edge.to.0,
            n!(edge.from).display_text_or_dummy_kind(),
            n!(edge.to).display_text_or_dummy_kind(),
        ));
    }

    // We want to balance gateway nodes between their outgoing/incoming branching nodes of the same
    // lane. For X gateways actually a better strategy would be to align it with the first `->` in
    // the BPMD, as this is likely the success case and should just go straight. But we will come
    // to that later.
    // Also, right now the alignment is actually done against the helper bendpoint dummy nodes.
    let mut gateway_balancing_constraint_vars = Vec::new();
    for gateway in node_ids_iter
        .clone()
        .map(|node_id| &graph.nodes[node_id])
        .filter(|n| n.is_gateway())
    {
        fn target_node(node: &Node) -> Option<&Node> {
            if let NodeType::BendDummy {
                kind: BendDummyKind::FromGatewayToSameLane { .. },
                ..
            } = &node.node_type
            {
                Some(node)
            } else {
                None
            }
        }

        let iter = find_first_last(gateway.incoming.iter(), |edge_id| {
            target_node(&from!(*edge_id))
        })
        .chain(find_first_last(gateway.outgoing.iter(), |edge_id| {
            target_node(&to!(*edge_id))
        }))
        .filter(|(first, last)| first.id != last.id);
        for (first, last) in iter {
            let var = vars.add(variable().min(0.0));
            gateway_balancing_constraint_vars.push(
                ((aux(gateway) + (gateway.height / 2) as f64)
                    - (aux(first) + (first.height / 2) as f64))
                    .leq(var),
            );
            gateway_balancing_constraint_vars.push(
                ((aux(last) + (last.height / 2) as f64)
                    - (aux(gateway) + (gateway.height / 2) as f64))
                    .leq(var),
            );
            objective += var;
            d!(eprintln!(
                "gateway balance between top node({0}) - gateway node({1}) - bottom node({2}) (and distance between {0} and {2} > {3})",
                first.id.0,
                gateway.id.0,
                last.id.0,
                graph.config.min_vertical_space_between_gateway_bendpoints
            ));

            // An additional constraint to ensure that the branches are not too close to the gateway
            // node, otherwise it looks awkward.
            gateway_balancing_constraint_vars.push(
                (aux(first) + graph.config.min_vertical_space_between_gateway_bendpoints as f64)
                    .leq(aux(last)),
            );
        }
    }

    let gateway_offset_iterator = gateway_forbidden_offset_constraints_prepare_big_ms(
        &mut vars,
        graph,
        node_ids_iter.clone(),
        &mut gateway_big_ms,
    );
    let mut problem = vars.minimise(objective).using(default_solver);

    fn y_padding(n: &Node, cfg: &Config) -> usize {
        if n.is_any_dummy() {
            cfg.dummy_node_y_padding
        } else {
            cfg.regular_node_y_padding
        }
    }

    // Add padding constraints between neighboring nodes.
    node_ids_iter
        .clone()
        .map(|node_id| &n!(node_id))
        .flat_map(|n| {
            n.node_below_in_same_lane
                .map(|next| (n, &graph.nodes[next.0]))
                .into_iter()
                .chain(
                    // Gateway nodes are "detached" in some cases, as they are replaced by bend
                    // dummies. But they must have a padding constraint with their above (and below
                    // - this is already handled) as well in case the bend dummies don't need as
                    // much of a height as the gateway node would.
                    //
                    // TODO this whole "detachedness" is confusing. Instead, this neighbor search
                    // should just do an up/down traversal if necessary.
                    n.is_gateway()
                        .then_some(())
                        .and(n.node_above_in_same_lane)
                        .map(|prev| (&n!(prev), n)),
                )
        })
        .for_each(|(above, below)| {
            let padding = y_padding(above, &graph.config).max(y_padding(below, &graph.config));
            c(
                &mut problem,
                (aux(below) - aux(above)).geq((above.height + padding) as f64),
            );
            d!(eprintln!(
                "padding above node({0}) <dist {1}> below node({2})",
                above.id.0, padding, below.id.0
            ));
        });

    gateway_forbidden_offset_constraints(
        &mut problem,
        graph,
        gateway_offset_iterator,
        &graph.pools[pool.0].lanes[lane.0],
        &mut gateway_big_ms,
    );

    node_ids_iter.clone().for_each(|node_id| {
        c(
            &mut problem,
            (aux(&n!(node_id)) + (&n!(node_id).height / 2) as f64)
                .into_expression()
                .leq(height_minimization_var),
        )
    });

    // Helper construct to resolve the minimization of |from.y - to.y|
    for (from_idx, to_idx, diff_var) in diff_vars.iter() {
        let from_node = &graph.nodes[*from_idx];
        let to_node = &graph.nodes[*to_idx];
        let from_var = aux(from_node);
        let to_var = aux(to_node);
        let from_y = from_var + (from_node.height / 2) as f64;
        let to_y = to_var + (to_node.height / 2) as f64;
        c(&mut problem, (from_y.clone() - to_y.clone()).leq(diff_var));
        c(&mut problem, (to_y - from_y).leq(diff_var));
    }

    for constraint in gateway_balancing_constraint_vars {
        c(&mut problem, constraint);
    }

    let solution = problem.solve().unwrap();

    let mut min_y_encountered = usize::MAX;
    let mut max_y_plus_height_encountered = usize::MIN;
    for node_id in node_ids_iter.clone() {
        d!(eprintln!(
            "solution n({}) y: {}",
            node_id.0,
            solution.value(aux(&n!(node_id))) as usize,
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
        graph.pools[pool.0].lanes[lane.0]
            .nodes
            .iter()
            .for_each(|n| graph.nodes[n.0].y -= diff);
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

/// Find the first and last elements (from the front and back, respectively)
/// of a `DoubleEndedIterator` that satisfy `pred`.
///
/// Returns `(first, last)`, each as an `Option`.
///
/// If exactly one matching element exists, both `first` and `last` will be
/// that same element (requires `Clone` to duplicate the owned item).
pub fn find_first_last<I, F, T>(mut iter: I, mut pred: F) -> impl Iterator<Item = (T, T)>
where
    I: DoubleEndedIterator + Clone,
    F: FnMut(I::Item) -> Option<T>,
{
    // Walk inward from both ends until we have both matches or we exhaust the iterator.

    // If only one match exists overall, mirror it so both are equal.
    let mut rev = iter.clone().rev();
    if let Some(first) = iter.find_map(&mut pred)
        && let Some(last) = rev.find_map(&mut pred)
    {
        Some((first, last)).into_iter()
    } else {
        None.into_iter()
    }
}

fn gateway_forbidden_offset_constraints_prepare_big_ms<'a>(
    vars: &mut ProblemVariables,
    graph: &'a Graph,
    node_ids_iter: Cloned<Iter<'a, NodeId>>,
    gateway_big_ms: &mut Vec<GatewayBigMHelpers>,
) -> impl Iterator<Item = (&'a Node, &'a Node)> + 'a {
    gateway_big_ms.clear();
    let iter_result = node_ids_iter
        .map(|node_id| &n!(node_id))
        .filter(|node| node.is_gateway())
        .flat_map(move |gateway_node| {
            gateway_node
                .incoming
                .iter()
                .map(move |incoming_edge_id| &from!(*incoming_edge_id))
                .chain(
                    gateway_node
                        .outgoing
                        .iter()
                        .map(move |outgoing_edge_id| &to!(*outgoing_edge_id)),
                )
                .flat_map(move |bend_or_vertical_node| {
                    if !bend_or_vertical_node.is_bend_dummy()
                        || bend_or_vertical_node.lane != gateway_node.lane
                    {
                        // !bend_dummy: It is a real node which is directly above or below the gateway,
                        // so they are spaced differently.
                        // Different lanes: This means that the other_node (going further from the
                        // bend dummy) is also on another lane, and so we don't need spacing
                        // constraints.
                        // Further, if only `other_node` is on another lane, then we also
                        // don't want to connect to `incoming_node` as a backup.
                        return None;
                    }
                    let other_node = &n!(bend_or_vertical_node
                        .hop_to_next_node(graph, NodeIdOrEdgeId::NodeId(gateway_node.id))
                        .1);
                    if other_node.layer_id == gateway_node.layer_id {
                        return None;
                    }
                    if other_node.lane == gateway_node.lane {
                        Some((gateway_node, other_node))
                    } else {
                        // The other node is on another lane, but it might be that the bend dummy is
                        // still on this lane.
                        Some((gateway_node, bend_or_vertical_node))
                    }
                })
        });
    for _ in 0..iter_result.clone().count() {
        gateway_big_ms.push(GatewayBigMHelpers {
            z0: vars.add(variable().binary()),
            zp: vars.add(variable().binary()),
            zm: vars.add(variable().binary()),
        });
    }
    // So the usage side can just `.pop()` them.
    gateway_big_ms.reverse();

    // Question is: is `collect::<Vec<_>>` cheaper or doing the iteration twice? I would bet it is
    // the first ... But Rust is about zero copy, so let's blindly follow idioms here :rocket:
    iter_result
}
fn gateway_forbidden_offset_constraints<'a>(
    problem: &mut impl SolverModel,
    graph: &Graph,
    nodes_iter: impl Iterator<Item = (&'a Node, &'a Node)> + 'a,
    lane: &Lane,
    gateway_big_ms: &mut Vec<GatewayBigMHelpers>,
) {
    let min = graph.config.min_vertical_space_between_gateway_bendpoints / 2;
    let max = lane.nodes.len()
        * (MAX_NODE_HEIGHT
            + std::cmp::max(
                graph.config.regular_node_y_padding,
                graph.config.dummy_node_y_padding,
            ));
    for (gateway_node, constraint_reference_node) in nodes_iter {
        gateway_forbidden_offset_constraints_inner(
            problem,
            gateway_node,
            &gateway_big_ms.pop().unwrap(),
            constraint_reference_node,
            min,
            max,
        );
    }
}

fn gateway_forbidden_offset_constraints_inner(
    problem: &mut impl SolverModel,
    gateway: &Node,
    helpers: &GatewayBigMHelpers,
    other_node: &Node,
    min: usize,
    max: usize,
) {
    // Gateway neighbors: Ensure that the target node of a gateway is either exactly at the same
    // height (so the edge is straight to the right) or sufficiently offset to the top or bottom
    // such that the connecting edge can leave to the top or bottom of the gateway node.
    // Otherwise, it will result in diagonal edges (which is only for gateway nodes due to the
    // collapsing logic).
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

    let z0 = helpers.z0;
    let zp = helpers.zp;
    let zm = helpers.zm;

    problem.add_constraint((z0 + zp + zm).eq(1));

    //  (1) `d >= min * z+ - max * z-`
    problem.add_constraint(
        ((aux(gateway) + (gateway.height / 2) as f64)
            - (aux(other_node) + (other_node.height / 2) as f64))
            .geq((min as f64) * zp - (max as f64) * zm),
    );
    //  (2) `d <= max * z+ - min * z-`
    problem.add_constraint(
        ((aux(gateway) + (gateway.height / 2) as f64)
            - (aux(other_node) + (other_node.height / 2) as f64))
            .leq((max as f64) * zp - (min as f64) * zm),
    );
}

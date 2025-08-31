use crate::common::edge::Edge;
use crate::common::edge::FlowType;
use crate::common::graph::Graph;
use crate::common::graph::LaneId;
use crate::common::graph::PoolId;
use crate::common::iter_ext::IteratorExt;
use crate::common::node::Node;
use crate::common::node::NodePhaseAuxData;
use good_lp::*;

#[derive(Debug)]
pub struct XyIlpNodeData {
    var: Variable,
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
        graph.pools[pool_idx].height = if min_y_value == pool_y {
            graph.config.height_of_empty_pool
        } else {
            min_y_value - pool_y
        };
        min_y_value += graph.config.pool_y_margin;
    }

    assign_x(graph);
}

#[track_caller]
fn aux(node: &Node) -> &XyIlpNodeData {
    match node.aux {
        NodePhaseAuxData::XyIlpNodeData(ref a) => a,
        _ => panic!(""),
    }
}

fn assign_y(graph: &mut Graph, pool: PoolId, lane: LaneId, min_y_value: usize) -> usize {
    let mut vars = variables!();
    let node_ids_iter = graph.pools[pool.0].lanes[lane.0].nodes.iter().cloned();

    // Note: One could argue that this ordering should be part of the graph. But it turns out that
    // this is not necessary in any other phase, just here, so we can compute it just locally.
    // Keeps the common graph types less cluttered.
    //let mut all_nodes: HashMap</*layer*/ LayerId, NodeId> = HashMap::new();
    for node_id in node_ids_iter.clone() {
        assert!(
            graph.nodes[node_id].pool == pool,
            "{pool:?}, {lane:?} -> {}",
            graph.nodes[node_id]
        );
        assert!(
            graph.nodes[node_id].lane == lane,
            "{pool:?}, {lane:?} -> {}",
            graph.nodes[node_id]
        );
        graph.nodes[node_id].aux = NodePhaseAuxData::XyIlpNodeData(XyIlpNodeData {
            var: vars.add(variable().integer().min(min_y_value as f64)),
        });
    }

    // TODO the allocation could be moved out of this function.
    let mut diff_vars = Vec::new();

    let mut objective = Expression::from(0.0);

    // Minimize the vertical length of edges. I.e. ideally as a result they go
    // directly to the right (have a vertical length of 0) and hence have no
    // bend points.
    for edge in node_ids_iter
        .clone()
        .flat_map(|node_id| graph.nodes[node_id].outgoing.iter().cloned())
        .map(|e| &graph.edges[e])
        .filter(|e| e.stays_within_lane)
    {
        assert!(!edge.is_replaced_by_dummies());

        if is_gateway_branching_flow_within_same_lane(edge, graph) {
            // The weight for gateway-connected edges is assigned through the
            // "gateway_balancing_constraint_vars", so don't assign anything additional here.
            // Otherwise this interferes in weird ways.
            // NB: Prioritizing some branch to be "straight to the right" should be done
            // separately as well.
            continue;
        }

        let diff_var = vars.add(variable().min(0.0));
        diff_vars.push((edge.from.0, edge.to.0, diff_var));

        let edge_weight = match (edge.flow_type.clone(), edge.is_dummy()) {
            (FlowType::SequenceFlow, false) => graph.config.short_sequence_flow_weight,
            (FlowType::SequenceFlow, true) => graph.config.long_sequence_flow_weight,
            (FlowType::DataFlow(_), false) => graph.config.short_data_flow_weight,
            (FlowType::DataFlow(_), true) => graph.config.long_data_edge_weight,
            (FlowType::MessageFlow(_), _) => graph.config.message_edge_weight,
        };
        objective += diff_var * edge_weight;
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
        {
            let first = gateway
                .incoming
                .iter()
                .map(|e| &graph.edges[*e])
                .filter(|e| e.flow_type == FlowType::SequenceFlow)
                .map(|e| &graph.nodes[e.from])
                .find(|n| n.pool_and_lane() == gateway.pool_and_lane());
            let last = gateway
                .incoming
                .iter()
                .map(|e| &graph.edges[*e])
                .filter(|e| e.flow_type == FlowType::SequenceFlow)
                .map(|e| &graph.nodes[e.from])
                .rfind(|n| n.pool_and_lane() == gateway.pool_and_lane());
            if let (Some(first), Some(last)) = (first, last)
                && first.id != last.id
            {
                let var = vars.add(variable().min(0.0));
                gateway_balancing_constraint_vars.push((
                    var,
                    ((aux(gateway).var + (gateway.height / 2) as f64)
                        - (aux(first).var + (first.height / 2) as f64))
                        .leq(var),
                ));
                gateway_balancing_constraint_vars.push((
                    var,
                    ((aux(last).var + (last.height / 2) as f64)
                        - (aux(gateway).var + (gateway.height / 2) as f64))
                        .leq(var),
                ));
                objective += var;
            }
        }

        {
            let first = gateway
                .outgoing
                .iter()
                .map(|e| &graph.edges[*e])
                .filter(|e| e.flow_type == FlowType::SequenceFlow)
                .map(|e| &graph.nodes[e.to])
                .find(|n| n.pool_and_lane() == gateway.pool_and_lane());
            let last = gateway
                .outgoing
                .iter()
                .map(|e| &graph.edges[*e])
                .filter(|e| e.flow_type == FlowType::SequenceFlow)
                .map(|e| &graph.nodes[e.to])
                .rfind(|n| n.pool_and_lane() == gateway.pool_and_lane());
            if let (Some(first), Some(last)) = (first, last)
                && first.id != last.id
            {
                let var = vars.add(variable().min(0.0));
                gateway_balancing_constraint_vars.push((
                    var,
                    ((aux(gateway).var + (gateway.height / 2) as f64)
                        - (aux(first).var + (first.height / 2) as f64))
                        .leq(var),
                ));
                gateway_balancing_constraint_vars.push((
                    var,
                    ((aux(last).var + (last.height / 2) as f64)
                        - (aux(gateway).var + (gateway.height / 2) as f64))
                        .leq(var),
                ));
                objective += var;
            }
        }
    }

    let mut problem = vars.minimise(objective).using(default_solver);
    problem.set_parameter("loglevel", "0");

    // Add padding constraints between neighboring nodes.
    node_ids_iter
        .clone()
        .map(|node_id| &graph.nodes[node_id.0])
        .flat_map(|n| {
            n.node_below_in_same_lane
                .map(|next| (n, &graph.nodes[next.0]))
        })
        .for_each(|(above, below)| {
            let padding = match (above.is_dummy(), below.is_dummy()) {
                (true, true) => graph.config.dummy_node_y_padding,
                (false, false) => graph.config.regular_node_y_padding,
                _ => graph
                    .config
                    .dummy_node_y_padding
                    .max(graph.config.regular_node_y_padding),
            };
            problem.add_constraint(
                (aux(below).var - aux(above).var).geq((above.height + padding) as f64),
            );
        });

    // TODO the thing is that when gateways start to have edges come out of the top and bottom,
    // then we need to push up/down the direct neighbors of the gateway. Right now gateway
    // edges go horizontally and then spread out vertically, so there is no collision risk.
    //        for gateway_node in
    //    graph.pools[pool.0].lanes[lane.0].nodes
    //            .iter()
    //        .map(|n| &graph.nodes[n.0])
    //            .filter(|n| n.is_gateway())
    //        {
    //            let incoming_or_outgoing = match (gateway_node.incoming.len(), gateway_node.outgoing.len()) {
    //            // Should go out just horizontally, so no special casing.
    //              (1,1) => continue,
    //            (1,_) => gateway_node.outgoing.iter().map(|e| &graph.nodes[graph.edges[e.0].to.0]). WELLP at this place we would need to check whether something crosses the lane?
    //            (_,1) => gateway_node.incoming.iter(),
    //(_,_) => todo!("A gateway has multiple incoming and multiple outgoing edges. Is that possible? Comment edge or something like that?")
    //            };
    //            incoming_or_outgoing.clone().map(|e| &graph.edges[e.0])
    //            let highest = gateway_node.incoming
    //            let lowest = prev_layer.last().unwrap();
    //            let highest_var = highest.var;
    //            let lowest_var = lowest.var;
    //            //let next_layer = all_nodes
    //            //    .get(&(
    //            //        gateway_node.pool,
    //            //        gateway_node.lane,
    //            //        Some(gateway_node.layer_id.unwrap() + 1),
    //            //    ))
    //            //    .unwrap();
    //            //let next_highest = next_layer.first().unwrap();
    //            //let next_lowest = next_layer.last().unwrap();
    //            //let next_highest_var =
    //            //    get_y_var(&y_vars, &next_highest.id, &next_highest.is_datanode).unwrap();
    //            //let next_lowest_var =
    //            //    get_y_var(&y_vars, &next_lowest.id, &next_lowest.is_datanode).unwrap();
    //            for (j, node) in nodes.iter().enumerate() {
    //                let node_var = aux(&graph.nodes[node.id]).var;
    //                if node.is_dummy {
    //                    continue;
    //                }
    //                if j < i {
    //                    problem = problem
    //                        .with((highest_var - node_var).geq(graph.config.min_space_in_gateway_layer))
    //                } else if j > i {
    //                    problem = problem
    //                        .with((node_var - lowest_var).geq(graph.config.min_space_in_gateway_layer))
    //                } else {
    //                    // problem = problem.with(((highest_var + lowest_var) / 2).eq(node_var));
    //                    // problem =
    //                    //     problem.with(((next_highest_var + next_lowest_var) / 2).eq(node_var));
    //                }
    //            }
    //        }

    // Helper construct to resolve the minimization of |from.y - to.y|
    for (from_idx, to_idx, diff_var) in diff_vars.iter() {
        let from_node = &graph.nodes[*from_idx];
        let to_node = &graph.nodes[*to_idx];
        let from_var = aux(from_node).var;
        let to_var = aux(to_node).var;
        let from_y = from_var + (from_node.height / 2) as f64;
        let to_y = to_var + (to_node.height / 2) as f64;
        problem.add_constraint((from_y.clone() - to_y.clone()).leq(diff_var));
        problem.add_constraint((to_y - from_y).leq(diff_var));
    }

    for (_, constraint) in gateway_balancing_constraint_vars {
        problem.add_constraint(constraint);
    }

    let solution = problem.solve().unwrap();

    let mut min_y_encountered = usize::MAX;
    let mut max_y_plus_height_encountered = usize::MIN;
    for node_id in node_ids_iter.clone() {
        let node = &mut graph.nodes[node_id.0];
        node.y = solution.value(aux(node).var) as usize;
        min_y_encountered = min_y_encountered.min(node.y);
        max_y_plus_height_encountered = max_y_plus_height_encountered.max(node.y + node.height);
    }

    // Check if the ILP actually assigned the lowest possible y value. If not,
    // we need to manually shift all the nodes upwards a bit to fit correctly
    // into the available space ("pixel perfect").
    if min_y_encountered > min_y_value {
        let diff = min_y_encountered - min_y_value;
        graph.pools[pool.0].lanes[lane.0]
            .nodes
            .iter()
            .for_each(|n| graph.nodes[n.0].y -= diff);
        max_y_plus_height_encountered -= diff;
    }
    max_y_plus_height_encountered - min_y_encountered
}

fn assign_x(graph: &mut Graph) {
    // Note: Does not need to account for pools on the same horizontal line, since
    // this was already taken care for in the layer assignment phase.
    let min_x_value = graph.config.pool_header_width + graph.config.lane_x_padding;
    for node in graph.nodes.iter_mut() {
        node.x =
            node.layer_id.0 * graph.config.layer_width + min_x_value + graph.config.layer_width / 2
                - node.width / 2;
    }
    for pool in &mut graph.pools {
        pool.width = graph.num_layers * graph.config.layer_width
            + graph.config.pool_header_width
            + graph.config.lane_x_padding;
        for lane in &mut pool.lanes {
            lane.x = graph.config.pool_header_width;
            lane.width = pool.width - graph.config.pool_header_width;
        }
    }
}

fn is_gateway_branching_flow_within_same_lane(edge: &Edge, graph: &Graph) -> bool {
    let from_is_branching = || {
        let n = &graph.nodes[edge.from];
        n.is_gateway()
            && n.outgoing
                .iter()
                .map(|e| &graph.edges[*e])
                .filter(|e| e.flow_type == FlowType::SequenceFlow)
                .map(|e| &graph.nodes[e.to])
                .filter(|other| other.pool_and_lane() == n.pool_and_lane())
                .has_at_least(2)
    };
    let to_is_joining = || {
        let n = &graph.nodes[edge.to];
        n.is_gateway()
            && n.incoming
                .iter()
                .map(|e| &graph.edges[*e])
                .filter(|e| e.flow_type == FlowType::SequenceFlow)
                .map(|e| &graph.nodes[e.from])
                .filter(|other| other.pool_and_lane() == n.pool_and_lane())
                .has_at_least(2)
    };
    edge.is_sequence_flow()
        && (graph.nodes[edge.from].pool_and_lane() == graph.nodes[edge.to].pool_and_lane())
        && (from_is_branching() || to_is_joining())
}

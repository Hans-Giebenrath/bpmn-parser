use crate::same_layaer_lane_crossings_within_cluster::same_layer_lane_crossings_within_cluster;
use bpmd_graph::ParseError;
use bpmd_graph::bpmn_node::BpmnNode;
use bpmd_graph::bpmn_node::EventType;
use bpmd_graph::bpmn_node::EventVisual;
use bpmd_graph::graph::Graph;
use bpmd_graph::graph::PoolAndLane;
use bpmd_graph::node::Node;
use bpmd_graph::node::NodeType;
use bpmd_util::vecset::VecSet;
use proc_macros::{e, from, n, to};

pub fn validate_graph_correctness(graph: &Graph) -> Result<(), ParseError> {
    // TODO
    //
    // (1) there is no situation where an edge's from has multiple edges in its outgoing vec, and
    // the edge's to has multiple edges in its incoming vec. TODO in the future this should
    // actually work.
    // (4) Data names should be consistent (when sending and receiving something)
    // (5) Not all activities can have all boundary events
    //

    let mut errors = Vec::<ParseError>::new();

    {
        for message_flow in graph.edges.iter().filter(|e| e.is_message_flow()) {
            check_if_valid_message_flow_start(&graph.nodes[message_flow.from], &mut errors);
            check_if_valid_message_flow_end(&graph.nodes[message_flow.to], &mut errors);
        }
    }

    {
        // No self-loops: An edge's from is different from to.
        for e in graph.edges.iter() {
            if e.from == e.to {
                errors.push(vec![(
                    "Self edges are forbidden. There is a self edge on this node".to_string(),
                    n!(e.from).tc(),
                )]);
            }
        }
    }

    {
        // Gateways should only have on one side multiple edges.
        // There are special layout possibilities to actually allow this, but I believe that they
        // are surprising, i.e. not idiomatic. Better is to dedicate a gateway always to either
        // branching or either joining.
        for node in graph.nodes.iter() {
            if node.is_gateway() {
                let in_sf_count = node
                    .incoming
                    .iter()
                    .filter(|edge| e!(**edge).is_sequence_flow())
                    .count();
                let out_sf_count = node
                    .outgoing
                    .iter()
                    .filter(|edge| e!(**edge).is_sequence_flow())
                    .count();
                if in_sf_count > 1 && out_sf_count > 1 {
                    errors.push(vec![(
                        "This gateway has multiple incoming sequence flows and multiple outgoing sequence flows. This is forbidden, as there must be exactly one incoming sequence flow or exactly one outgoing sequence flow. Split the joining and branching into two separate gateways to keep visuals more idiomatic.".to_string(),
                        node.tc(),
                    )]);
                }
                if in_sf_count == 0 {
                    errors.push(vec![(
                        "This gateway has no incoming sequence flows. This is forbidden, as there must be at least one incoming sequence flow.".to_string(),
                        node.tc(),
                    )]);
                }
                if out_sf_count == 0 {
                    errors.push(vec![(
                        "This gateway has no outgoing sequence flows. This is forbidden, as there must be at least one outgoing sequence flow.".to_string(),
                        node.tc(),
                    )]);
                }
            }
        }
    }

    // Regular nodes don't branch nor join. This is the job for gateways. In principle, I think BPMN
    // with Style said that it is okay to join, but my tool is opinionated.
    for node in &graph.nodes {
        if node.is_gateway() {
            continue;
        }

        let inc_iter = node
            .incoming
            .iter()
            .map(|edge_id| (&e!(*edge_id), &from!(*edge_id)))
            .filter(|(e, _)| e.is_sequence_flow() && e.attached_to_boundary_event.is_none());
        let out_iter = node
            .outgoing
            .iter()
            .map(|edge_id| (&e!(*edge_id), &to!(*edge_id)))
            .filter(|(e, _)| e.is_sequence_flow() && e.attached_to_boundary_event.is_none());
        if inc_iter.clone().count() > 1 {
            let err = errors.push_mut(vec![(
                "This node has more than one incoming sequence flow which is forbidden. Use a gateway for joining."
                    .to_string(),
                node.tc(),
            )]);
            for (_, from) in inc_iter {
                err.push(("One sequence flow comes from here".to_string(), from.tc()));
            }
        }

        if out_iter.clone().count() > 1 {
            let err = errors.push_mut(vec![(
                "This node has more than one outgoing sequence flow which is forbidden. Use a gateway for branching."
                    .to_string(),
                node.tc(),
            )]);
            for (_, from) in out_iter {
                err.push(("One sequence flow goes here".to_string(), from.tc()));
            }
        }
    }

    {
        // A gateway shall not have more than at most two above/same-layer constraints with
        // connected nodes. That ensures that they can be placed directly next to each other, one
        // above and one below, and not create funky situations with wonky-wonky back and forth
        // edges. Really, if there are more such nodes, then they should just move into the next
        // layer, as the visuals really just get spagetthified otherwise.
        for gateway in graph.nodes.iter().filter(|n| n.is_gateway()) {
            let mut all_connected = VecSet::new();
            for other in gateway
                .incoming
                .iter()
                .map(|e| e!(*e).from)
                .chain(gateway.outgoing.iter().map(|e| e!(*e).to))
            {
                all_connected.insert(other);
            }
            let Some(cluster) = graph
                .layout_constraints
                .same_layer_clusters
                .iter()
                .find(|c| c.contains(&gateway.id))
            else {
                // No cluster contains the gateway node. This means that all its others will
                // be placed on different layers.
                continue;
            };

            let iter = all_connected.iter().filter(|n| cluster.contains(n));
            let count = iter.clone().count();
            if count >= 3 {
                //This is the bad situation.
                let err = errors.push_mut(vec![]);
                err.push(("This gateway is forced on the same layer as more than two of his incoming or outgoing connected nodes. This leads to confusing layouts. Please remove the constraints instead, to let them move to the next layer instead".to_string(), gateway.tc()));
                for node in iter {
                    err.push((
                        "This connected node is forced onto the same layer as the gateway"
                            .to_string(),
                        n!(*node).tc(),
                    ));
                }
            }
        }
    }

    {
        // A complicated one: Currently I believe that there is no need for S edges. But that also
        // means, that we cannot have a same-layer cluster where edges would overlap. I mean,
        // within the same lane you can just order them nicely. But in the case of a lane crossing,
        // you can not order them: To allow for only-vertical edges (not S edges), there can only
        // be one edge per cluster to do a given lane crossing.
        // Complementing this: If a lane crossing spans over another lane, then there
        // cannot be another node forced into that lane at all.
        let mut all_lane_crossings = Vec::new();
        for cluster in &graph.layout_constraints.same_layer_clusters {
            all_lane_crossings.clear();
            all_lane_crossings.extend(same_layer_lane_crossings_within_cluster(graph, cluster));
            all_lane_crossings.sort_unstable();

            for crossing in &all_lane_crossings {
                assert_eq!(crossing.top_pool_lane.pool, crossing.bot_pool_lane.pool);
                let in_between_lane_range =
                    crossing.top_pool_lane.lane.0 + 1..crossing.bot_pool_lane.lane.0;
                if in_between_lane_range.is_empty() {
                    continue;
                }
                for node_id in cluster.iter().cloned() {
                    let node = &n!(node_id);
                    let PoolAndLane { pool, lane } = node.pool_and_lane();
                    if pool != crossing.top_pool_lane.pool
                        || !in_between_lane_range.contains(&lane.0)
                    {
                        continue;
                    }
                    errors.push(vec![("Two connected nodes are forced into the same layer, but they span across another lane. This means that the algorithm must push other nodes into other layers. However, this node is forced as well into the same layer (via above or same-layer `place` constraints). Please relax some of the same-layer and/or above constraints.".to_string(), node.tc()),
                    ("This is the top node of the two connected nodes".to_string(), crossing.top_tc),
                    ("This is the bottom node of the two connected nodes".to_string(), crossing.bot_tc),
                    ]);
                }
            }

            for [left, right] in all_lane_crossings.array_windows() {
                assert_ne!(left, right); // Make sure that the construction is correct.

                if left.top_pool_lane == left.bot_pool_lane {
                    // This cannot result in a problem. The `right_*` could be on the same
                    // pool_lane, or just on some next.
                    continue;
                }
                if !(left.top_pool_lane == right.top_pool_lane
                    && left.bot_pool_lane == right.bot_pool_lane)
                {
                    // The other case is covered by the above lane-crossing check already.
                    // So in here we are only left with lane crossings that span from the same lane
                    // to the same other lane.
                    continue;
                }
                let err =  errors.push_mut(vec![("Two connected nodes are put into the same layer. However, they cross from one lane to another lane, and in that case no other nodes within the same layer are allowed to do the same lane crossing. But there is another node pair which is forced into the same layer due to `above of` or `same layer` constraints and which does the same lane crossing. Please think through your above and same-layer place constraints again. This is the top node of the first pair:".to_string(),right.top_tc)]);
                err.push((
                    "This is the bottom node of the first pair:".to_string(),
                    right.bot_tc,
                ));
                err.push((
                    "This the top node of the second pair:".to_string(),
                    left.top_tc,
                ));
                err.push((
                    "This the bottom node of the second pair:".to_string(),
                    left.bot_tc,
                ));
            }
        }
    }

    if let Some(first) = errors.into_iter().next() {
        // TODO well, at some point the parser should be rewritten to allow for returning multiple
        // errors. Right now it is one error at a time, but if multiple things are broken then a
        // more exhaustive list would be nice.
        Err(first)
    } else {
        Ok(())
    }
}

fn check_if_valid_message_flow_start(node: &Node, errors: &mut Vec<ParseError>) {
    if let NodeType::RealNode { event, tc, .. } = &node.node_type {
        match event {
            BpmnNode::Event(EventType::Message, EventVisual::Throw | EventVisual::End) => (),
            BpmnNode::Activity(..) => (),
            _ => {
                errors.push(vec![(
                "This node type cannot send messages. Only message events (M#, M.) or tasks (e.g. .-) can be used as message flow starts. Note that shorthand events (#) are automatically transformed into message events when they are used in a message flow.".to_string(),
                    *tc,

                )]);
            }
        }
    }
}

fn check_if_valid_message_flow_end(node: &Node, errors: &mut Vec<ParseError>) {
    if let NodeType::RealNode { event, tc, .. } = &node.node_type {
        match event {
            BpmnNode::Event(EventType::Message, EventVisual::Start(_) | EventVisual::Catch(_)) => {}
            BpmnNode::Event(EventType::Blank, _) => (),
            BpmnNode::Activity(..) => (),
            _ => {
                errors.push(vec![(
                "This node type cannot catch messages. Only message events (M#) or tasks (e.g. .-) can be used as message flow ends. Note that shorthand events (#) are automatically transformed into message events when they are used in a message flow.".to_string(),
                    *tc,

                )]);
            }
        }
    }
}

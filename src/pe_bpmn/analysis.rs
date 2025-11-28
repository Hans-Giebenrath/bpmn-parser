use crate::common::graph::{Graph, LaneId, PoolId};
use crate::common::node::NodeType;
use crate::lexer::TokenCoordinate;
use crate::pe_bpmn::parser::{
    ComputationCommon, Mpc, PeBpmn, PeBpmnSubType, PeBpmnType, Protection, SecureChannel, Tee,
};
use crate::{
    common::{
        edge::FlowType,
        graph::{EdgeId, NodeId, SdeId},
    },
    lexer::{PeBpmnMeta, PeBpmnProtection},
    parser::ParseError,
};
use itertools::Itertools;
use proc_macros::{e, n};
use std::collections::HashSet;
use std::iter::Extend;

pub fn analyse(graph: &mut Graph) -> Result<(), ParseError> {
    create_transported_data(graph);
    let pebpmn_definitions = std::mem::take(&mut graph.pe_bpmn_definitions);
    for pebpmn_definition in &pebpmn_definitions {
        analyse_single(pebpmn_definition, graph)?;
    }
    graph.pe_bpmn_definitions = pebpmn_definitions;

    Ok(())
}

pub fn analyse_single(pe_bpmn: &PeBpmn, graph: &mut Graph) -> Result<(), ParseError> {
    let enforce_reach_end = true;
    match &pe_bpmn.r#type {
        PeBpmnType::SecureChannel(secure_channel) => {
            analyse_secure_channel(secure_channel, &pe_bpmn.meta, graph)?;
        }
        PeBpmnType::Tee(Tee { common }) => {
            compute_visibility_tee_or_mpc(graph, common, PeBpmnProtection::Tee(common.tc))?;
            check_that_protection_is_visually_applied_tee_or_mpc(graph, common)?;
            match &common.pebpmn_type {
                &PeBpmnSubType::Pool(pool_id) => parse_pebpmn_pool_or_lane(
                    graph,
                    common,
                    &pe_bpmn.meta,
                    enforce_reach_end,
                    PeBpmnProtection::Tee(common.tc),
                    pool_id,
                    None,
                )?,
                &PeBpmnSubType::Lane { pool_id, lane_id } => parse_pebpmn_pool_or_lane(
                    graph,
                    common,
                    &pe_bpmn.meta,
                    enforce_reach_end,
                    PeBpmnProtection::Tee(common.tc),
                    pool_id,
                    Some(lane_id),
                )?,
                PeBpmnSubType::Tasks(tasks) => {
                    for (node_id, _) in tasks {
                        if let NodeType::RealNode {
                            pe_bpmn_hides_protection_operations,
                            ..
                        } = &mut graph.nodes[*node_id].node_type
                        {
                            *pe_bpmn_hides_protection_operations = true;
                        }
                    }
                    parse_pebpmn_tasks(
                        graph,
                        tasks,
                        common,
                        &pe_bpmn.meta,
                        enforce_reach_end,
                        PeBpmnProtection::Tee(common.tc),
                    )?
                }
            }
        }
        PeBpmnType::Mpc(Mpc { common }) => {
            compute_visibility_tee_or_mpc(graph, common, PeBpmnProtection::Mpc(common.tc))?;
            check_that_protection_is_visually_applied_tee_or_mpc(graph, common)?;
            match &common.pebpmn_type {
                &PeBpmnSubType::Pool(pool_id) => parse_pebpmn_pool_or_lane(
                    graph,
                    common,
                    &pe_bpmn.meta,
                    enforce_reach_end,
                    PeBpmnProtection::Mpc(common.tc),
                    pool_id,
                    None,
                )?,
                &PeBpmnSubType::Lane { pool_id, lane_id } => parse_pebpmn_pool_or_lane(
                    graph,
                    common,
                    &pe_bpmn.meta,
                    enforce_reach_end,
                    PeBpmnProtection::Mpc(common.tc),
                    pool_id,
                    Some(lane_id),
                )?,
                PeBpmnSubType::Tasks(tasks) => {
                    for (node_id, _) in tasks {
                        if let NodeType::RealNode {
                            pe_bpmn_hides_protection_operations,
                            ..
                        } = &mut graph.nodes[*node_id].node_type
                        {
                            *pe_bpmn_hides_protection_operations = true;
                        }
                    }
                    parse_pebpmn_tasks(
                        graph,
                        tasks,
                        common,
                        &pe_bpmn.meta,
                        enforce_reach_end,
                        PeBpmnProtection::Mpc(common.tc),
                    )?;
                }
            }
        }
    }
    Ok(())
}

pub fn analyse_secure_channel(
    secure_channel: &SecureChannel,
    meta: &PeBpmnMeta,
    graph: &mut Graph,
) -> Result<(), ParseError> {
    let enforce_reach_end = true;
    let is_reverse = true;
    let protection = PeBpmnProtection::SecureChannel(secure_channel.tc);
    let filter = |sde_id: &SdeId| {
        secure_channel.permitted_ids.is_empty() || contains(&secure_channel.permitted_ids, sde_id)
    };
    if let Some((sender_id, sender_tc)) = secure_channel.sender {
        check_secure_channel_permitted_sdes_are_valid(
            graph,
            sender_id,
            sender_tc,
            &secure_channel.permitted_ids,
            "sender",
        )?;
        check_that_protection_is_visually_applied(
            graph,
            sender_id,
            sender_tc,
            !is_reverse,
            filter,
        )?;
    }

    if let Some((receiver_id, receiver_tc)) = secure_channel.receiver {
        check_secure_channel_permitted_sdes_are_valid(
            graph,
            receiver_id,
            receiver_tc,
            &secure_channel.permitted_ids,
            "receiver",
        )?;
        check_that_protection_is_visually_applied(
            graph,
            receiver_id,
            receiver_tc,
            is_reverse,
            filter,
        )?;
    }

    match (secure_channel.sender, secure_channel.receiver) {
        (Some((sender, sender_tc)), Some((receiver, _))) => check_protection_paths(
            graph,
            sender,
            sender_tc,
            !is_reverse,
            meta,
            &[receiver],
            protection,
            filter,
            enforce_reach_end,
        )?,
        (Some((sender, sender_tc)), None) => check_protection_paths(
            graph,
            sender,
            sender_tc,
            !is_reverse,
            meta,
            &[],
            protection,
            filter,
            !enforce_reach_end,
        )?,
        (None, Some((receiver, receiver_tc))) => check_protection_paths(
            graph,
            receiver,
            receiver_tc,
            is_reverse,
            meta,
            &[receiver],
            protection,
            filter,
            !enforce_reach_end,
        )?,
        (None, None) => {
            if secure_channel.permitted_ids.is_empty() {
                return Err(vec![(
                    "You need to define IDs when you use pre-sent and post-received simultaneously to identify data which should be marked as protected (or omit the `[pe-bpmn ...]` statement if there is nothing to protect). Example: `(secure-channel pre-sent post-received @data-obj1 @data-obj2)`"
                        .to_string(),
                    secure_channel.tc,

                )]);
            }

            // Add protection to data elements and their data flow edges
            secure_channel.permitted_ids.iter().for_each(|(sde_id, _)| {
                let sde = &mut graph.data_elements[*sde_id];
                sde.data_element.iter().for_each(|node_id| {
                    let node = &mut graph.nodes[*node_id];
                    node.fill_color = meta.fill_color.clone();
                    node.stroke_color = meta.stroke_color.clone();
                    node.set_pebpmn_protection(protection);
                    for edge_id in node.incoming.iter().chain(&node.outgoing) {
                        graph.edges[*edge_id].set_pebpmn_protection(*sde_id, protection);
                        graph.edges[*edge_id].stroke_color = meta.stroke_color.clone();
                    }
                });
            });

            // Add protection to message flows transporting data elements
            for edge in graph.edges.iter_mut().filter(|edge| edge.is_message_flow()) {
                let sde_ids: Vec<_> = edge
                    .get_transported_data()
                    .iter()
                    .copied()
                    .filter(|sde_id| contains(&secure_channel.permitted_ids, sde_id))
                    .collect();

                for sde_id in sde_ids {
                    edge.set_pebpmn_protection(sde_id, protection);
                }
            }
        }
    }
    Ok(())
}

fn compute_visibility_tee_or_mpc(
    graph: &mut Graph,
    computation: &ComputationCommon,
    protection: PeBpmnProtection,
) -> Result<(), ParseError> {
    let admin = computation.admin;
    // For all in-protect nodes that the admin either owns or are unassigned, give the admin visibility A to all the SDEs those nodes carry.
    graph.pools[admin]
        .tee_admin_has_pe_bpmn_visibility_A_for
        .extend(
            computation
                .in_protect
                .iter()
                .filter(|data_flow_annotation| {
                    data_flow_annotation.rv_source.is_none()
                        || data_flow_annotation.rv_source == Some(admin)
                })
                .flat_map(|data_flow_annotation| {
                    graph.nodes[data_flow_annotation.node]
                        .get_node_transported_data()
                        .iter()
                        .copied()
                })
                .map(|sde_id| (sde_id, protection)),
        );

    // For all out-unprotect nodes that the admin either owns or are unassigned, give the admin visibility H to all the SDEs those nodes carry.
    graph.pools[admin]
        .tee_admin_has_pe_bpmn_visibility_H_for
        .extend(
            computation
                .out_unprotect
                .iter()
                .filter(|data_flow_annotation| {
                    data_flow_annotation.rv_source.is_none()
                        || data_flow_annotation.rv_source == Some(admin)
                })
                .flat_map(|data_flow_annotation| {
                    graph.nodes[data_flow_annotation.node]
                        .get_node_transported_data()
                        .iter()
                        .copied()
                })
                .map(|sde_id| (sde_id, protection)),
        );

    let mut all_sdes = HashSet::<SdeId>::new();
    // Consider all data which is transported to/within/out of the TEE/MPC.
    for node in &graph.nodes {
        let consider = match &computation.pebpmn_type {
            &PeBpmnSubType::Pool(pool_id) => node.pool == pool_id,
            &PeBpmnSubType::Lane { pool_id, lane_id } => {
                node.pool == pool_id && node.lane == lane_id
            }
            PeBpmnSubType::Tasks(tasks) => tasks.iter().any(|(node_id, _)| *node_id == node.id),
        };
        if consider {
            all_sdes.extend(
                node.incoming
                    .iter()
                    .chain(node.outgoing.iter())
                    .flat_map(|edge_id| e!(*edge_id).get_transported_data())
                    .cloned(),
            );
        }
    }

    for pool in &computation.external_root_access {
        graph.pools[*pool]
            .tee_external_root_access
            .extend(all_sdes.iter().copied().map(|sde_id| (sde_id, protection)));
    }
    Ok(())
}

fn parse_pebpmn_pool_or_lane(
    graph: &mut Graph,
    computation: &ComputationCommon,
    meta: &PeBpmnMeta,
    enforce_reach_end: bool,
    protection: PeBpmnProtection,
    pool_id: PoolId,
    lane_id: Option<LaneId>,
) -> Result<(), ParseError> {
    let filter = computation_filter(computation);
    if let Some(lane_id) = lane_id {
        graph.pools[pool_id].lanes[lane_id].fill_color = meta.fill_color.clone();
        graph.pools[pool_id].lanes[lane_id].stroke_color = meta.stroke_color.clone();
    } else {
        graph.pools[pool_id].fill_color = meta.fill_color.clone();
        graph.pools[pool_id].stroke_color = meta.stroke_color.clone();
    };
    let mut check_protected = |protect_nodes: &Vec<Protection>,
                               unprotect_nodes: &Vec<Protection>|
     -> Result<(), ParseError> {
        let unprotected_nodes = unprotect_nodes.iter().map(|n| n.node).collect::<Vec<_>>();
        for node in protect_nodes {
            let is_reverse = true;
            check_protection_paths(
                graph,
                node.node,
                node.tc,
                !is_reverse,
                meta,
                &unprotected_nodes,
                protection,
                &filter,
                enforce_reach_end,
            )?
        }
        Ok(())
    };

    check_protected(&computation.in_protect, &computation.in_unprotect)?;
    check_protected(&computation.out_protect, &computation.out_unprotect)?;

    Ok(())
}

fn parse_pebpmn_tasks(
    graph: &mut Graph,
    tasks: &[(NodeId, TokenCoordinate)],
    computation: &ComputationCommon,
    meta: &PeBpmnMeta,
    enforce_reach_end: bool,
    protection: PeBpmnProtection,
) -> Result<(), ParseError> {
    let filter = computation_filter(computation);
    let end_out = computation
        .out_unprotect
        .iter()
        .map(|tee_node| tee_node.node)
        .collect_vec();

    let end_in = computation
        .in_protect
        .iter()
        .map(|tee_node| tee_node.node)
        .collect_vec();

    for (node_id, node_tc) in tasks.iter().cloned() {
        let node = &mut graph.nodes[node_id];
        node.fill_color = meta.fill_color.clone();
        node.stroke_color = meta.stroke_color.clone();
        node.set_pebpmn_protection(protection);

        for (is_reverse, ends) in [(false, end_out.as_slice()), (true, end_in.as_slice())] {
            check_protection_paths(
                graph,
                node_id,
                node_tc,
                is_reverse,
                meta,
                ends,
                protection,
                &filter,
                enforce_reach_end,
            )?
        }
    }

    Ok(())
}

fn protection_channel(
    graph: &Graph,
    edge_id: EdgeId,
    cur_sde: SdeId,
    end: &[NodeId],
    state: &mut ProtectionPathTraversalState,
    is_reverse: bool,
) -> Result<(), ParseError> {
    // TODO this function should do a more broad graph traversal:
    // On a data-element, follow all (in and out) edges. On a node, (1) if coming in (through MF or
    // data-flow) then follow all data-flow and message-flow which are in or out of that node. (2)
    // If it came backwards through outgoing, then only traverse through other in/out of that node
    // if there was some incoming of that data into the node (transported_data contains the sde), as
    // otherwise this node produced the data, and it is totally possible that it produced a
    // protected and a non-protected version of the data.
    let edge = &e!(edge_id);
    let next_node_id = if is_reverse { edge.from } else { edge.to };

    let is_data_or_msg = edge.is_message_flow() || edge.is_data_flow();
    let transports_sde = edge.get_transported_data().contains(&cur_sde);

    if !is_data_or_msg || !transports_sde {
        return Ok(());
    }

    state.edges_to_color.push((edge_id, cur_sde));

    // XXX: First augment/check `visited`, and *only then* check `end`, as the caller checks what
    // end nodes were visited, for error reporting.
    if !state.visited.insert(next_node_id) || end.contains(&next_node_id) {
        return Ok(());
    }

    let next_node = &n!(next_node_id);
    let (traverse_incoming, traverse_outgoing) = if next_node.is_data() {
        state.nodes_to_color.push(next_node.id);
        // This data-icon is protected, so wherever it came from or goes to is considered part of the
        // protection path.
        (true, true)
    } else if !is_reverse {
        // We are moving forward. So we just look at other outgoing edges. We must consider other
        // incoming instances of the same SDE as unprotected version, as they might totally be. If
        // they should still be protected, one can use the "already-protected" version.
        (false, true)
    } else if next_node.get_node_transported_data().contains(&cur_sde) {
        // We come reverse into the node, and we know that this SDE travelled through this node. So
        // we know that *some* of the incoming flows transported the protected data (this is
        // not an end node, so one *must* be protected). We can't tell which one exactly, so we have
        // to assume the worst and just say that all incoming paths of that SDE are protected. And
        // then we also follow all outgoing ones. It is in that regard inconsistent with the
        // previous `!is_reverse` block, as more of the incoming paths are considered. Not sure if
        // this is a problem in practice - maybe one should consider expanding the `!is_revers`
        // version to make it more consistent ...
        (true, true)
    } else {
        // The node did not transport the data, so we must assume this is some black box node which
        // contains an extension of the protection path. This also means we don't follow any of the
        // other paths.
        (false, false)
    };

    for (next_edges, traverse, is_reverse) in [
        (&next_node.incoming, traverse_incoming, true),
        (&next_node.outgoing, traverse_outgoing, false),
    ] {
        if !traverse {
            continue;
        }
        for next_edge_id in next_edges
            .iter()
            .cloned()
            .filter(|next_edge_id| *next_edge_id != edge_id)
        {
            protection_channel(graph, next_edge_id, cur_sde, end, state, is_reverse)?;
        }
    }
    Ok(())
}

/// Encapsulates the mutable aspects of the traversal, so the graph itself is not mutated during
/// traversal (circumvents the borrow checker).
#[derive(Default)]
struct ProtectionPathTraversalState {
    edges_to_color: Vec<(EdgeId, SdeId)>,
    nodes_to_color: Vec<NodeId>,
    visited: HashSet<NodeId>,
}

#[track_caller]
#[allow(clippy::too_many_arguments)]
fn check_protection_paths(
    graph: &mut Graph,
    node_id: NodeId,
    node_pebpmn_tc: TokenCoordinate,
    is_reverse: bool,
    meta: &PeBpmnMeta,
    ends: &[NodeId],
    protection_type: PeBpmnProtection,
    filter: impl Fn(&SdeId) -> bool,
    enforce_reach_end: bool,
) -> Result<(), ParseError> {
    let edges = if is_reverse {
        &graph.nodes[node_id].incoming
    } else {
        &graph.nodes[node_id].outgoing
    };

    let mut state = ProtectionPathTraversalState::default();

    for edge_id in edges.clone() {
        for sde_id in e!(edge_id)
            .get_transported_data()
            .iter()
            .cloned()
            .filter(&filter)
        {
            // This is a new search, so reset the `visited` buffer.
            state.visited.clear();
            state.visited.insert(node_id);

            protection_channel(graph, edge_id, sde_id, ends, &mut state, is_reverse)?;
            if enforce_reach_end && ends.iter().all(|end| !state.visited.contains(end)) {
                assert_ne!(ends.len(), 0); // If `enforce_reach_end` is `true` but no `end` was
                // provided by the caller, then this is a programming error on the caller side.
                return Err(create_protection_error_message(
                    graph,
                    node_id,
                    node_pebpmn_tc,
                    sde_id,
                    is_reverse,
                    protection_type,
                    ends,
                ));
            }
        }
    }

    for (edge_id, sde_id) in state.edges_to_color.iter().cloned() {
        let edge = &mut e!(edge_id);
        if edge.is_message_flow() {
            edge.set_pebpmn_protection(sde_id, protection_type);
        }
        edge.stroke_color = meta.stroke_color.clone();
    }
    for node_id in state.nodes_to_color.iter().cloned() {
        let node = &mut n!(node_id);
        node.fill_color = meta.fill_color.clone();
        node.stroke_color = meta.stroke_color.clone();
        node.set_pebpmn_protection(protection_type);
    }
    Ok(())
}

// TODO is data_without_protection and data_with_protection used anywhere? Not specifically in this
// function.
fn check_that_protection_is_visually_applied_tee_or_mpc(
    graph: &Graph,
    computation: &ComputationCommon,
) -> Result<(), ParseError> {
    let is_reverse = true;
    let filter = computation_filter(computation);
    for Protection { node, tc, .. } in computation
        .in_protect
        .iter()
        .chain(computation.out_protect.iter())
    {
        check_that_protection_is_visually_applied(graph, *node, *tc, !is_reverse, &filter)?;
    }

    for Protection { node, tc, .. } in computation
        .in_unprotect
        .iter()
        .chain(computation.out_unprotect.iter())
    {
        check_that_protection_is_visually_applied(graph, *node, *tc, is_reverse, &filter)?;
    }

    Ok(())
}

// A node that applies protection to some data-object should have this data object also
// attached as an incoming data object. I.e. it should visually travel through the node and be
// transformed from an unprotected icon to a protected icon.
// And vice versa for un-protect nodes.
fn check_that_protection_is_visually_applied(
    graph: &Graph,
    node_id: NodeId,
    // The specific `(tee-in-protect ..)` (or similar) line.
    pe_protection_tc: TokenCoordinate,
    is_reverse: bool,
    filter: impl Fn(&SdeId) -> bool,
) -> Result<(), ParseError> {
    assert!(
        matches!(&graph.nodes[node_id].node_type, NodeType::RealNode {
        pe_bpmn_hides_protection_operations,
        ..
    } if !pe_bpmn_hides_protection_operations),
        "got node: {:?}\n{:?}",
        &graph.nodes[node_id],
        &graph
    );

    let edges = if is_reverse {
        &graph.nodes[node_id].incoming
    } else {
        &graph.nodes[node_id].outgoing
    };

    let node_transported_data = graph.nodes[node_id].get_node_transported_data();

    if let Some(detached_sde_id) = edges
        .iter()
        .flat_map(|edge_id| graph.edges[*edge_id].get_transported_data())
        .cloned()
        .filter(filter)
        .find(|sde_id| !node_transported_data.contains(sde_id))
    {
        let full_data_declaration = graph.data_elements[detached_sde_id]
            .data_element
            .iter()
            .fold(
                TokenCoordinate {
                    start: usize::MAX,
                    end: usize::MIN,
                    source_file_idx: 0,
                },
                |acc, el| {
                    let tc = &n!(*el).tc();
                    TokenCoordinate {
                        start: acc.start.min(tc.start),
                        end: acc.end.max(tc.end),
                        // Should anyway always be the same.
                        source_file_idx: tc.source_file_idx,
                    }
                },
            );
        // Maybe this text needs to be modified one day to account for message flows, but I
        // think this is not relevant. Just will become more complicated to provide a hyper
        // specific error description (though it would help make the tool more usable ...).
        let error_string = if is_reverse {
            "This unprotect node has an incoming (protected) data element (`OD ... ->@node_id` or similar is present), but the same (unprotected) data object is missing on the outgoing side (`OD ... <-@node_id` or similar is missing). The unprotected object must be visually present to not confuse the reader."
        } else {
            "This protect node has an outgoing (protected) data element (`OD ... <-@node_id` or similar is present), but the same (unprotected) data object is missing on the incoming side (`OD ... ->@node_id` or similar is missing). The unprotected object must be visually present to not confuse the reader."
        };
        Err(vec![
            (error_string.to_string(), n!(node_id).tc()),
            (
                "The PE-BPMN instruction is here".to_string(),
                pe_protection_tc,
            ),
            (
                "The data element is declared here".to_string(),
                full_data_declaration,
            ),
        ])
    } else {
        Ok(())
    }
}

pub fn create_transported_data(graph: &mut Graph) {
    for edge_id in 0..graph.edges.len() {
        if !graph.edges[edge_id].is_message_flow() {
            continue;
        }
        let edge = &graph.edges[edge_id];

        let incoming_nodes: HashSet<NodeId> = graph.nodes[edge.from]
            .incoming
            .iter()
            .map(|e| graph.edges[*e].clone())
            .filter(|e| e.is_data_flow())
            .map(|e| e.from)
            .collect();

        let incoming_sde: HashSet<SdeId> = incoming_nodes
            .iter()
            .filter_map(|node_id| graph.nodes[*node_id].get_data_aux())
            .map(|aux| aux.sde_id)
            .collect();

        let outgoing_nodes: HashSet<NodeId> = graph.nodes[edge.to]
            .outgoing
            .iter()
            .map(|e| graph.edges[*e].clone())
            .filter(|e| e.is_data_flow())
            .map(|e| e.to)
            .collect();

        let outgoing_sde: HashSet<SdeId> = outgoing_nodes
            .iter()
            .filter_map(|node_id| graph.nodes[*node_id].get_data_aux())
            .map(|aux| aux.sde_id)
            .collect();

        let intersection: Vec<SdeId> = incoming_sde.intersection(&outgoing_sde).cloned().collect();

        if let FlowType::MessageFlow(message_flow_aux) = &mut graph.edges[edge_id].flow_type {
            message_flow_aux.transported_data.extend(intersection);
            message_flow_aux.transported_data.sort();
            message_flow_aux.transported_data.dedup();
        }
    }

    for i in 0..graph.nodes.len() {
        let node = &graph.nodes[i];

        if node.is_gateway() {
            continue;
        }

        let incoming = get_edge_data_ids(graph, &node.incoming);
        let outgoing = get_edge_data_ids(graph, &node.outgoing);

        let intersect: Vec<SdeId> = incoming.intersection(&outgoing).copied().collect();
        graph.nodes[i].add_node_transported_data(&intersect);
    }
}

fn get_edge_data_ids(graph: &Graph, edge_ids: &[EdgeId]) -> HashSet<SdeId> {
    let mut ids = HashSet::new();

    for edge_id in edge_ids {
        let edge = &graph.edges[*edge_id];
        ids.extend(edge.get_transported_data());
    }
    ids
}

// Technically this should be in the parser, but this relies on `transported_data` calculation which
// is done after all parsing is finished, so this is carried out in the late phase.
fn check_secure_channel_permitted_sdes_are_valid(
    graph: &Graph,
    node_id: NodeId,
    sender_or_receiver_tc: TokenCoordinate,
    permitted_sdes: &[(SdeId, TokenCoordinate)],
    sender_or_receiver: &str,
) -> Result<(), ParseError> {
    for (sde_id, sde_tc) in permitted_sdes {
        let sde = &graph.data_elements[*sde_id];

        if !n!(node_id).get_node_transported_data().contains(sde_id) {
            let data_element = if sde.name.is_empty() {
                " "
            } else {
                sde.name.as_str()
            };

            return Err(vec![
                (
                    format!(
                        "Data element{data_element}is not connected to the {sender_or_receiver} node",
                    ),
                    *sde_tc,
                ),
                (
                    "This is the semantic data element".to_string(),
                    sde.tc(graph),
                ),
                ("This is the sender node ..".to_string(), n!(node_id).tc()),
                (
                    ".. which was selected by this identifier".to_string(),
                    sender_or_receiver_tc,
                ),
            ]);
        }
    }

    Ok(())
}

fn create_protection_error_message(
    graph: &Graph,
    node_id: NodeId,
    node_pebpmn_tc: TokenCoordinate,
    sde_id: SdeId,
    is_reverse: bool,
    protection: PeBpmnProtection,
    ends: &[NodeId],
) -> ParseError {
    let what_to_do_instead = match protection {
        PeBpmnProtection::SecureChannel(..) => "If this data element should not be protected at all, you can specify a list of protected data like (secure-channel @sender @receiver @protected-data1 @protected-data2) where you omit this data element.".to_string(),
        PeBpmnProtection::Mpc(..) | PeBpmnProtection::Tee(..) => format!("If this data element should not be protected at all, you can specify an explicit list of excluded data via ({protection}-data-without-protection @data-id1 @data-id2)"),
    };
    let mut result = vec![
        (
            format!(
                "Protection analysis found a node which {} data, but the data does not reach any of the respective {} nodes. {what_to_do_instead}",
                if is_reverse {
                    "un-protects"
                } else {
                    "protects"
                },
                if is_reverse { "protect" } else { "un-protect" }
            ),
            node_pebpmn_tc,
        ),
        (
            "This node was matched by the node selector".to_string(),
            n!(node_id).tc(),
        ),
        (
            "The traversal path of this data element was followed".to_string(),
            graph.data_elements[sde_id].tc(graph),
        ),
    ];
    for end in ends {
        result.push(("This end was considered".to_string(), n!(*end).tc()));
    }

    result
}

fn contains<T: std::cmp::PartialEq>(haystack: &[(T, TokenCoordinate)], needle: &T) -> bool {
    haystack.iter().any(|(straw, _)| *straw == *needle)
}

fn computation_filter(computation: &ComputationCommon) -> impl Fn(&SdeId) -> bool + '_ {
    |sde_id| {
        computation
            .data_without_protection
            .iter()
            .all(|(without_prot, _)| *sde_id != *without_prot)
    }
}

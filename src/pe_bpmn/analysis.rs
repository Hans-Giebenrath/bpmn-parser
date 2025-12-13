use crate::lexer::TokenCoordinate;
use crate::pe_bpmn::PoolOrProtection;
use crate::pe_bpmn::parser::{
    ComputationCommon, Mpc, PeBpmn, PeBpmnSubType, PeBpmnType, Protection, SecureChannel, Tee,
};
use crate::pe_bpmn::{ProtectionPaths, VisibilityTableInput};
use crate::{
    common::graph::{EdgeId, NodeId, SdeId},
    lexer::PeBpmnProtection,
    parser::ParseError,
};
use crate::{
    common::{
        graph::{Graph, LaneId, PoolId},
        node::NodeType,
    },
    pe_bpmn::ProtectionGraphCmp,
};
use itertools::Itertools;
use proc_macros::{e, from, n, to};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::iter::Extend;

pub fn analyse(graph: &mut Graph) -> Result<VisibilityTableInput, ParseError> {
    let mut state = State::default();
    let pebpmn_definitions = std::mem::take(&mut graph.pe_bpmn_definitions);
    for pebpmn_definition in &pebpmn_definitions {
        analyse_single(pebpmn_definition, graph, &mut state)?;
    }
    graph.pe_bpmn_definitions = pebpmn_definitions;
    apply_colors(graph, &state);

    compute_accessible_data(graph, &mut state)?;

    for ((_, sde_id), protections) in state.message_flow_protection.into_iter() {
        state
            .result
            .network_message_protections
            .entry(sde_id)
            .or_default()
            .insert(protections);
    }

    Ok(state.result)
}

#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy)]
enum GraphElement {
    NonData(NodeId),
    Data(NodeId),
    // Both message flows and data flows.
    Edge(EdgeId),
    // `tee-tasks`. Those are primarily for adding fill color to the nodes.
    Task(NodeId),
    // `tee-pool`.
    Pool(PoolId),
    // `tee-lane`.
    Lane(PoolId, LaneId),
}

#[derive(Default)]
struct State {
    // A `data` node represents just one piece of data, but it can be protected simultaneously by
    // multiple protections (both secure channel and TEE).
    data_node_protection: HashMap<NodeId, HashSet<PeBpmnProtection>>,
    message_flow_protection: HashMap<(EdgeId, SdeId), BTreeSet<PeBpmnProtection>>,
    data_flow_protection: HashMap<EdgeId, (SdeId, BTreeSet<PeBpmnProtection>)>,
    // The reverse of `data_node_protection` and `message_flow_protection`. To apply coloring after
    // the graph is analysed (enables to traverse over `&Graph` instead of `&mut Graph`).
    protection_graph: HashMap<PeBpmnProtection, HashSet<GraphElement>>,
    // Like `protection_graph` but more fine-grained. Just contains edges to realise whether
    // different protections are either correctly nested or disjoint (if some protection path of
    // pe_bpmn_protection_1 is strictly smaller than some protection path of pe_bpmn_protection_2,
    // then no protection path of pe_bpmn_protection_2 shall be strictly smaller than some
    // protection path of pe_bpmn_protection_1).
    protection_paths_graphs: HashMap<PeBpmnProtection, ProtectionPaths>,
    result: VisibilityTableInput,
}

impl State {
    fn set_data_node_protection(&mut self, data_node_id: NodeId, protection: PeBpmnProtection) {
        self.data_node_protection
            .entry(data_node_id)
            .or_default()
            .insert(protection);
        self.protection_graph
            .entry(protection)
            .or_default()
            .insert(GraphElement::Data(data_node_id));
    }

    fn set_nondata_node_protection(
        &mut self,
        nondata_node_id: NodeId,
        protection: PeBpmnProtection,
    ) {
        self.protection_graph
            .entry(protection)
            .or_default()
            .insert(GraphElement::NonData(nondata_node_id));
    }

    fn set_message_flow_protection(
        &mut self,
        mf_id: EdgeId,
        sde_id: SdeId,
        protection: PeBpmnProtection,
    ) {
        self.message_flow_protection
            .entry((mf_id, sde_id))
            .or_default()
            .insert(protection);
        self.protection_graph
            .entry(protection)
            .or_default()
            .insert(GraphElement::Edge(mf_id));
    }

    fn set_data_flow_protection(
        &mut self,
        df_id: EdgeId,
        sde_id: SdeId,
        protection: PeBpmnProtection,
    ) {
        let df = self.data_flow_protection.entry(df_id).or_default();
        df.0 = sde_id;
        df.1.insert(protection);
        self.protection_graph
            .entry(protection)
            .or_default()
            .insert(GraphElement::Edge(df_id));
    }
}

fn analyse_single(
    pe_bpmn: &PeBpmn,
    graph: &mut Graph,
    state: &mut State,
) -> Result<(), ParseError> {
    let enforce_reach_end = true;
    match &pe_bpmn.r#type {
        PeBpmnType::SecureChannel(secure_channel) => {
            analyse_secure_channel(secure_channel, graph, state)?;
        }
        PeBpmnType::Tee(Tee { common }) => {
            compute_visibility_tee_or_mpc(graph, state, common, PeBpmnProtection::Tee(common.tc))?;
            check_that_protection_is_visually_applied_tee_or_mpc(graph, common)?;
            match &common.pebpmn_type {
                &PeBpmnSubType::Pool(pool_id) => parse_pebpmn_pool_or_lane(
                    graph,
                    state,
                    common,
                    enforce_reach_end,
                    PeBpmnProtection::Tee(common.tc),
                    pool_id,
                    None,
                )?,
                &PeBpmnSubType::Lane { pool_id, lane_id } => parse_pebpmn_pool_or_lane(
                    graph,
                    state,
                    common,
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
                        state,
                        tasks,
                        common,
                        enforce_reach_end,
                        PeBpmnProtection::Tee(common.tc),
                    )?
                }
            }
        }
        PeBpmnType::Mpc(Mpc { common }) => {
            compute_visibility_tee_or_mpc(graph, state, common, PeBpmnProtection::Mpc(common.tc))?;
            check_that_protection_is_visually_applied_tee_or_mpc(graph, common)?;
            match &common.pebpmn_type {
                &PeBpmnSubType::Pool(pool_id) => parse_pebpmn_pool_or_lane(
                    graph,
                    state,
                    common,
                    enforce_reach_end,
                    PeBpmnProtection::Mpc(common.tc),
                    pool_id,
                    None,
                )?,
                &PeBpmnSubType::Lane { pool_id, lane_id } => parse_pebpmn_pool_or_lane(
                    graph,
                    state,
                    common,
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
                        state,
                        tasks,
                        common,
                        enforce_reach_end,
                        PeBpmnProtection::Mpc(common.tc),
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn analyse_secure_channel(
    secure_channel: &SecureChannel,
    graph: &Graph,
    state: &mut State,
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
            state,
            sender,
            sender_tc,
            !is_reverse,
            &[receiver],
            protection,
            filter,
            enforce_reach_end,
        )?,
        (Some((sender, sender_tc)), None) => check_protection_paths(
            graph,
            state,
            sender,
            sender_tc,
            !is_reverse,
            &[],
            protection,
            filter,
            !enforce_reach_end,
        )?,
        (None, Some((receiver, receiver_tc))) => check_protection_paths(
            graph,
            state,
            receiver,
            receiver_tc,
            is_reverse,
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
                let sde = &graph.data_elements[*sde_id];
                sde.data_element.iter().for_each(|node_id| {
                    let node = &graph.nodes[*node_id];
                    state.set_data_node_protection(*node_id, protection);
                    for edge_id in node.incoming.iter().chain(&node.outgoing) {
                        state.set_data_flow_protection(*edge_id, *sde_id, protection);
                    }
                });
            });

            // Add protection to message flows transporting data elements
            for (edge_idx, edge) in graph
                .edges
                .iter()
                .enumerate()
                .filter(|(_, edge)| edge.is_message_flow())
            {
                for sde_id in edge
                    .get_transported_data()
                    .iter()
                    .copied()
                    .filter(|sde_id| contains(&secure_channel.permitted_ids, sde_id))
                {
                    state.set_message_flow_protection(EdgeId(edge_idx), sde_id, protection);
                }
            }
        }
    }
    Ok(())
}

fn compute_accessible_data(graph: &Graph, analysis_state: &mut State) -> Result<(), ParseError> {
    let mut task_protections = Vec::new();
    for node in &graph.nodes {
        // Data nodes can be placed funkily. If a TEE lane shares data with the non-TEE lane then I
        // don't know where the respective data node is drawn. So ignore it and instead look at the
        // real nodes' incoming and outgoing data edges. (Not node_transported_data because it might
        // be created in a node and then directly shared with the TEE lane, so need to really
        // inspect the edges.)
        if node.is_data() {
            continue;
        }
        let mut pool_protection = None;
        let mut lane_protection = None;
        task_protections.clear();

        {
            for pebpmn in &graph.pe_bpmn_definitions {
                match &pebpmn.r#type {
                    PeBpmnType::Tee(Tee { common }) | PeBpmnType::Mpc(Mpc { common }) => {
                        match &common.pebpmn_type {
                            PeBpmnSubType::Pool(pool_id) if *pool_id == node.pool => {
                                assert!(pool_protection.is_none());
                                pool_protection = Some(pebpmn.r#type.protection());
                            }
                            PeBpmnSubType::Lane { pool_id, lane_id }
                                if *pool_id == node.pool && *lane_id == node.lane =>
                            {
                                assert!(lane_protection.is_none());
                                lane_protection = Some(pebpmn.r#type.protection());
                            }
                            PeBpmnSubType::Tasks(tasks) if contains(tasks, &node.id) => {
                                task_protections.push(pebpmn.r#type.protection());
                            }
                            _ => continue,
                        }
                    }
                    PeBpmnType::SecureChannel(sc) => {
                        if sc
                            .sender
                            .iter()
                            .chain(sc.receiver.iter())
                            .any(|(node_id, _)| *node_id == node.id)
                        {
                            task_protections.push(pebpmn.r#type.protection());
                        }
                    }
                };
            }

            for (a, b) in std::iter::once((pool_protection, lane_protection)).chain(
                lane_protection.or(pool_protection).iter().flat_map(|prot| {
                    task_protections
                        .iter()
                        .map(|task_protection| (Some(*prot), Some(*task_protection)))
                }),
            ) {
                if let Some(a) = a
                    && let Some(b) = b
                {
                    let cmp = analysis_state
                        .protection_paths_graphs
                        .get(&a)
                        .unwrap()
                        .compare(analysis_state.protection_paths_graphs.get(&b).unwrap());
                    match cmp {
                        Err(e) => todo!("nicer error {e:?}, {a:?}, {b:?}"),
                        Ok(ProtectionGraphCmp::Sub) => todo!("nicer error {a:?}, {b:?}"),
                        Ok(ProtectionGraphCmp::Super) => { /* all good */ }
                        Ok(ProtectionGraphCmp::Disjoint) => ( /* weird but all good */),
                    }
                }
            }

            task_protections.sort_by(|a, b| {
                match analysis_state
                    .protection_paths_graphs
                    .get(a)
                    .unwrap()
                    .compare(analysis_state.protection_paths_graphs.get(b).unwrap())
                {
                    Err(e) => todo!("Write a good error message, {e}"),
                    // Smallest to the right.
                    Ok(ProtectionGraphCmp::Sub) => std::cmp::Ordering::Greater,
                    Ok(ProtectionGraphCmp::Super) => std::cmp::Ordering::Less,
                    // TODO This is not allowed since it makes analysis rather hard. It could be
                    // that there is a TEE which could conditionally execute one MPC algorithm or
                    // another algorithm. But I believe this is a headache to implement, so just
                    // forbid it for the moment. Maybe no-one will ever ask.
                    Ok(ProtectionGraphCmp::Disjoint) => todo!("Write a good error message, pe-bpmns covering the same node shall not be disjoint"),
                }
            });

            pool_protection
                .iter()
                .chain(lane_protection.iter())
                .chain(
                    task_protections
                        .iter()
                        .filter(|protection| !protection.is_secure_channel()),
                )
                .tuple_windows()
                .for_each(|(a, b)| {
                    analysis_state
                        .result
                        .software_operator
                        .insert((PoolOrProtection::Protection(*a), *b));
                });

            // A secure channel should not be the first protection, or only if the rest is also
            // secure channels. Because if there are TEE/MPC tasks, then those make the task
            // implicit (the whole encryption/decryption stuff), but semantically the secure channel
            // is outside of them, so it should also end (or start) at another task. But if the
            // secure channel is within the TEE/MPC implicit task, then that's fine, as we then know
            // that it does not need to have the visible outcoming/incoing data icon.
            // TODO the `data` icon exception is probably not present at the moment in
            // `check_protection_paths`? Maybe that check should move into this function here?
            if let Some(first) = task_protections.first()
                && first.is_secure_channel()
                && let Some(non_sc) = task_protections
                    .iter()
                    .find(|arg0| !PeBpmnProtection::is_secure_channel(*arg0))
            {
                return Err(vec![
                    (
                        "This node is part of a secure channel and a nested (mpc|tee)-tasks protection. This is not allowed. Move the end of the secure channel to another node to make the secure channel more understandable. Or did you mix up the nesting? It would be ok the have the secure channel nested inside of the (mpc|tee)-tasks protection.".to_string(),
                        node.tc(),
                    ),
                    (
                        "This is the (outer) secure channel".to_string(),
                        first.tc()
                    ),
                    (
                        "This is the (inner) {non_sc}-tasks".to_string(),
                        non_sc.tc()
                    ),
                ]);
            }

            let mut analyse = |sde_id: SdeId,
                               mut protections: BTreeSet<PeBpmnProtection>,
                               is_message_flow: bool,
                               is_cross_lane_flow: bool| {
                if let Some(lane_protection) = lane_protection {
                    analysis_state
                        .result
                        .directly_accessible_data
                        .entry(PoolOrProtection::Protection(lane_protection))
                        .or_default()
                        .entry(sde_id)
                        .or_default()
                        .insert(protections.clone());
                }

                if is_message_flow || is_cross_lane_flow || lane_protection.is_none() {
                    // The container gets to see what moves in and out of the lane.
                    let pool_or_protection = if let Some(pool_protection) = pool_protection {
                        PoolOrProtection::Protection(pool_protection)
                    } else {
                        PoolOrProtection::Pool(node.pool)
                    };
                    analysis_state
                        .result
                        .directly_accessible_data
                        .entry(pool_or_protection)
                        .or_default()
                        .entry(sde_id)
                        .or_default()
                        .insert(protections.clone());
                }

                // The nested protections are all just secure channels, so there is no "owner".
                if task_protections
                    .first()
                    .map(PeBpmnProtection::is_secure_channel)
                    .unwrap_or(true)
                {
                    return;
                }

                let mut i = 0;
                while let Some(task_protection) = task_protections.get(i) {
                    // If the next (and next-next etc) is a
                    // secure channel, then their protections should be removed in the context
                    // of the current protection. But then the above secure channel must not be
                    // filtered out (see added TODO).
                    // Treat it as a message flow for all the protections.
                    protections.remove(task_protection);
                    // Now, there might be nested secure channels
                    let mut j = i + 1;
                    while let Some(peek) = task_protections.get(j)
                        && peek.is_secure_channel()
                    {
                        protections.remove(task_protection);
                        // We skip this one (the `+1` part is coming at the end).
                        i = j;
                        j += 1;
                    }
                    analysis_state
                        .result
                        .directly_accessible_data
                        .entry(PoolOrProtection::Protection(*task_protection))
                        .or_default()
                        .entry(sde_id)
                        .or_default()
                        .insert(protections.clone());
                    i += 1;
                }
            };

            // A `data` icon might span across lanes: Start in one lane, and end in another lane. Now
            // it could be that both lanes are separate `tee-lane`s. Hence the hosting pool would
            // not see it in the analysis just by looking at the source and target tasks. Instead,
            // we must identify that there is some implicit channel present. If the data shall be
            // shared securely, then it must be done with a secure channel. Easy as pie. Otherwise,
            // the host pool sees the unprotected data.
            let is_cross_lane_data = |edge_id: EdgeId| {
                let data_node = if from!(edge_id).is_data() {
                    &from!(edge_id)
                } else {
                    &to!(edge_id)
                };
                assert!(data_node.is_data());
                data_node
                    .incoming
                    .iter()
                    .map(|e| &from!(*e))
                    .chain(data_node.outgoing.iter().map(|e| &to!(*e)))
                    .any(|other_node| other_node.lane != node.lane)
            };

            for edge_id in node.incoming.iter().chain(node.outgoing.iter()).cloned() {
                let edge = &e!(edge_id);
                // TODO it would make this loop easier if `data_flow_protection` and
                // `message_flow_protection` were merged into one ...
                if edge.is_data_flow() {
                    if let Some((sde_id, protections)) =
                        analysis_state.data_flow_protection.get(&edge_id)
                    {
                        analyse(
                            *sde_id,
                            protections.clone(),
                            false,
                            is_cross_lane_data(edge_id),
                        );
                    } else {
                        analyse(
                            edge.get_transported_data()[0],
                            Default::default(),
                            false,
                            is_cross_lane_data(edge_id),
                        );
                    }
                } else if edge.is_message_flow() {
                    for sde_id in edge.get_transported_data().iter().cloned() {
                        if let Some(protections) = analysis_state
                            .message_flow_protection
                            .get(&(edge_id, sde_id))
                        {
                            analyse(sde_id, protections.clone(), true, false);
                        } else {
                            analyse(sde_id, Default::default(), true, false);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn compute_visibility_tee_or_mpc(
    graph: &Graph,
    analysis_state: &mut State,
    computation: &ComputationCommon,
    protection: PeBpmnProtection,
) -> Result<(), ParseError> {
    for software_operator in computation.software_operators.iter().cloned() {
        analysis_state.result.tee_vulnerable_rv.extend(
            computation
                .in_protect
                .iter()
                .filter(|data_flow_annotation| {
                    data_flow_annotation.rv_source.is_none()
                        || data_flow_annotation.rv_source == Some(software_operator)
                })
                .flat_map(|data_flow_annotation| {
                    graph.nodes[data_flow_annotation.node]
                        .get_node_transported_data()
                        .iter()
                        .copied()
                })
                .map(|sde_id| (software_operator, sde_id, protection)),
        );
    }

    analysis_state.result.software_operator.extend(
        computation
            .software_operators
            .iter()
            .cloned()
            .map(|pool| (PoolOrProtection::Pool(pool), protection)),
    );

    analysis_state.result.tee_hardware_operator.extend(
        computation
            .hardware_operators
            .iter()
            .cloned()
            .map(|pool| (pool, protection)),
    );

    // For all out-unprotect nodes that the admin either owns or are unassigned, give the admin visibility H to all the SDEs those nodes carry.

    for e in computation.external_root_access.iter().cloned() {
        analysis_state
            .result
            .tee_external_root_access
            .entry(e)
            .or_default()
            .insert(protection);
    }
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
    Ok(())
}

fn parse_pebpmn_pool_or_lane(
    graph: &Graph,
    analysis_state: &mut State,
    computation: &ComputationCommon,
    enforce_reach_end: bool,
    protection: PeBpmnProtection,
    pool_id: PoolId,
    lane_id: Option<LaneId>,
) -> Result<(), ParseError> {
    let filter = computation_filter(computation);
    if let Some(lane_id) = lane_id {
        analysis_state
            .protection_graph
            .entry(protection)
            .or_default()
            .insert(GraphElement::Lane(pool_id, lane_id));
    } else {
        analysis_state
            .protection_graph
            .entry(protection)
            .or_default()
            .insert(GraphElement::Pool(pool_id));
    };
    let mut check_protected = |protect_nodes: &Vec<Protection>,
                               unprotect_nodes: &Vec<Protection>|
     -> Result<(), ParseError> {
        let unprotected_nodes = unprotect_nodes.iter().map(|n| n.node).collect::<Vec<_>>();
        for node in protect_nodes {
            let is_reverse = true;
            check_protection_paths(
                graph,
                analysis_state,
                node.node,
                node.tc,
                !is_reverse,
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
    graph: &Graph,
    analysis_state: &mut State,
    tasks: &[(NodeId, TokenCoordinate)],
    computation: &ComputationCommon,
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
        analysis_state
            .protection_graph
            .entry(protection)
            .or_default()
            .insert(GraphElement::Task(node_id));

        for (is_reverse, ends) in [(false, end_out.as_slice()), (true, end_in.as_slice())] {
            check_protection_paths(
                graph,
                analysis_state,
                node_id,
                node_tc,
                is_reverse,
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
    analysis_state: &mut State,
    edge_id: EdgeId,
    // Could be in `state` but then we would need to deal with lifetimes ... narp.
    end: &[NodeId],
    state: &mut ProtectionPathTraversalState,
    is_reverse: bool,
) -> Result<(), ParseError> {
    let edge = &e!(edge_id);
    let next_node_id = if is_reverse { edge.from } else { edge.to };

    let is_data = edge.is_data_flow();
    let is_msg = edge.is_message_flow();
    let transports_sde = edge.get_transported_data().contains(&state.cur_sde);

    if !(is_data || is_msg) || !transports_sde {
        return Ok(());
    }

    state.visited_edges.insert(edge_id);
    if is_data {
        analysis_state.set_data_flow_protection(edge_id, state.cur_sde, state.protection);
    } else {
        analysis_state.set_message_flow_protection(edge_id, state.cur_sde, state.protection);
    }

    // XXX: First augment/check `visited`, and *only then* check `end`, as the caller checks what
    // end nodes were visited, for error reporting.
    if !state.visited_nodes.insert(next_node_id) || end.contains(&next_node_id) {
        return Ok(());
    }

    let next_node = &n!(next_node_id);
    let is_data_node = next_node.is_data();
    if is_data_node {
        analysis_state.set_data_node_protection(next_node_id, state.protection);
    } else {
        analysis_state.set_nondata_node_protection(next_node_id, state.protection);
    }

    let (traverse_incoming, traverse_outgoing) = if is_data_node {
        // This data-icon is protected, so wherever it came from or goes to is considered part of the
        // protection path.
        (true, true)
    } else if !is_reverse {
        // We are moving forward. So we just look at other outgoing edges. We must consider other
        // incoming instances of the same SDE as unprotected version, as they might totally be. If
        // they should still be protected, one can use the "already-protected" version.
        (false, true)
    } else if next_node
        .get_node_transported_data()
        .contains(&state.cur_sde)
    {
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
            protection_channel(graph, analysis_state, next_edge_id, end, state, is_reverse)?;
        }
    }
    Ok(())
}

/// Encapsulates the mutable aspects of the traversal, so the graph itself is not mutated during
/// traversal (circumvents the borrow checker).
struct ProtectionPathTraversalState {
    visited_nodes: HashSet<NodeId>,
    protection: PeBpmnProtection,
    cur_sde: SdeId,
    visited_edges: BTreeSet<EdgeId>,
}

#[track_caller]
#[allow(clippy::too_many_arguments)]
fn check_protection_paths(
    graph: &Graph,
    analysis_state: &mut State,
    node_id: NodeId,
    node_pebpmn_tc: TokenCoordinate,
    is_reverse: bool,
    ends: &[NodeId],
    protection: PeBpmnProtection,
    filter: impl Fn(&SdeId) -> bool,
    enforce_reach_end: bool,
) -> Result<(), ParseError> {
    let edges = if is_reverse {
        &graph.nodes[node_id].incoming
    } else {
        &graph.nodes[node_id].outgoing
    };

    for node_id in std::iter::once(&node_id).chain(ends).cloned() {
        // This will likely run multiple times for `ends` if the function is run multiple times. But
        // overall this is in the range of O(3 x 3) per protection I'd say (which is O(1) but you
        // get the point).
        // TODO is this check present in the pe-bpmn parser?
        assert!(!n!(node_id).is_data());
        analysis_state.set_nondata_node_protection(node_id, protection);
    }

    for edge_id in edges.clone() {
        for sde_id in e!(edge_id)
            .get_transported_data()
            .iter()
            .cloned()
            .filter(&filter)
        {
            if e!(edge_id).is_message_flow() {
                analysis_state.set_message_flow_protection(edge_id, sde_id, protection);
            } else {
                assert!(e!(edge_id).is_data_flow());
                analysis_state.set_data_flow_protection(edge_id, sde_id, protection);
            }

            let mut state = ProtectionPathTraversalState {
                visited_nodes: HashSet::from([node_id]),
                visited_edges: Default::default(),
                protection,
                cur_sde: sde_id,
            };

            protection_channel(graph, analysis_state, edge_id, ends, &mut state, is_reverse)?;
            if enforce_reach_end && ends.iter().all(|end| !state.visited_nodes.contains(end)) {
                assert_ne!(ends.len(), 0); // If `enforce_reach_end` is `true` but no `end` was
                // provided by the caller, then this is a programming error on the caller side.
                return Err(create_protection_error_message(
                    graph,
                    node_id,
                    node_pebpmn_tc,
                    sde_id,
                    is_reverse,
                    protection,
                    ends,
                ));
            }

            analysis_state
                .protection_paths_graphs
                .entry(protection)
                .or_default()
                .subgraphs
                .insert(state.visited_edges);
        }
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

fn apply_colors(graph: &mut Graph, state: &State) {
    for definition in &graph.pe_bpmn_definitions {
        let protection = definition.r#type.protection();
        for graph_element in state.protection_graph[&protection].iter().cloned() {
            match graph_element {
                GraphElement::Edge(edge_id) => {
                    let edge = &mut e!(edge_id);
                    edge.stroke_color = definition.meta.stroke_color.clone();
                }
                GraphElement::NonData(node_id) => {
                    let node = &mut n!(node_id);
                    node.stroke_color = definition.meta.stroke_color.clone();
                }
                GraphElement::Data(node_id) | GraphElement::Task(node_id) => {
                    let node = &mut n!(node_id);
                    node.stroke_color = definition.meta.stroke_color.clone();
                    node.fill_color = definition.meta.fill_color.clone();
                }
                GraphElement::Pool(pool_id) => {
                    let pool = &mut graph.pools[pool_id];
                    pool.stroke_color = definition.meta.stroke_color.clone();
                    pool.fill_color = definition.meta.fill_color.clone();
                }
                GraphElement::Lane(pool_id, lane_id) => {
                    let lane = &mut graph.pools[pool_id].lanes[lane_id];
                    lane.stroke_color = definition.meta.stroke_color.clone();
                    lane.fill_color = definition.meta.fill_color.clone();
                }
            }
        }
    }
}

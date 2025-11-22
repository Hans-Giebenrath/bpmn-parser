use annotate_snippets::Level;
use itertools::Itertools;
use std::collections::HashSet;
use crate::common::graph::{LaneId, PoolId};
use crate::common::node::NodeType;
use crate::lexer::TokenCoordinate;
use crate::{
    common::{
        edge::FlowType,
        graph::{EdgeId, NodeId, SdeId},
    },
    lexer::PeBpmnMeta,
    parser::ParseError,
};

pub fn analyse(pe_bpmn: lexer::PeBpmn) -> Result<PeBpmn, ParseError> {
    let enforce_reach_end = true;
    self.create_transported_data();
    match pe_bpmn.r#type {
        lexer::PeBpmnType::SecureChannel(secure_channel) => {
            let sender_id = secure_channel
                .sender
                .as_ref()
                .map_or(Ok(None), |sender_name| {
                    self.find_node_id(sender_name).map(Some).ok_or(vec![(
                        format!("Sender node with ID ({sender_name}) was not found. Have you defined it?"),
                        self.context.current_token_coordinate,
                        Level::Error,
                    )])
                })?;

            let receiver_id = secure_channel
                .receiver
                .as_ref()
                .map_or(Ok(None), |receiver_name| {
                    self.find_node_id(receiver_name).map(Some).ok_or(vec![(
                        format!("Receiver node with ID ({receiver_name}) was not found. Have you defined it?"),
                        self.context.current_token_coordinate,
                        Level::Error,
                    )])
                })?;

            let permitted_sdes: HashSet<SdeId> = secure_channel
                .argument_ids
                .iter()
                .map(|string_id| {
                    let node_id = self.find_node_id(string_id).ok_or(vec![(
                        format!("Data element with ID ({string_id}) was not found. Have you defined it?"),
                        self.context.current_token_coordinate,
                        Level::Error,
                    )])?;
                    if !self.graph.nodes[node_id].is_data() {
                        return Err(vec![(
                            format!("Only Data elements IDs are allowed! Node with ID ({string_id}) is not a data element"),
                            self.context.current_token_coordinate,
                            Level::Error,
                        )]);
                    }
                    Ok(node_id)
                })
                .collect::<Result<Vec<_>, _>>()? // Only continue if all NodeIds found and valid
                .into_iter()
                .filter_map(|node_id| {
                    self.graph
                        .data_elements
                        .iter()
                        .position(|sde| sde.contains(node_id))
                        .map(SdeId)
                })
                .collect();

            if let (Some(sender_id), Some(sender)) = (sender_id, secure_channel.sender) {
                self.check_connection(sender_id, sender, &permitted_sdes, true)?;
            }

            if let (Some(receiver_id), Some(receiver)) = (receiver_id, secure_channel.receiver)
            {
                self.check_connection(receiver_id, receiver, &permitted_sdes, false)?;
            }

            match (sender_id, receiver_id) {
                (Some(sender), Some(receiver)) => self.check_protection_paths(
                    sender,
                    false,
                    &pe_bpmn.meta,
                    &[receiver],
                    &PeBpmnProtection::SecureChannel,
                    |sde_id| permitted_sdes.is_empty() || permitted_sdes.contains(sde_id),
                    false,
                    enforce_reach_end,
                    "secure-channel",
                )?,
                (Some(sender), None) => self.check_protection_paths(
                    sender,
                    false,
                    &pe_bpmn.meta,
                    &[],
                    &PeBpmnProtection::SecureChannel,
                    |sde_id| permitted_sdes.is_empty() || permitted_sdes.contains(sde_id),
                    false,
                    false,
                    "secure-channel",
                )?,
                (None, Some(receiver)) => self.check_protection_paths(
                    receiver,
                    true,
                    &pe_bpmn.meta,
                    &[receiver],
                    &PeBpmnProtection::SecureChannel,
                    |sde_id| permitted_sdes.is_empty() || permitted_sdes.contains(sde_id),
                    false,
                    false,
                    "secure-channel",
                )?,
                (None, None) => {
                    if permitted_sdes.is_empty() {
                        return Err(vec![
                            ("You need to define IDs when you use pre-sent and post-received simultaneously".to_string(),
                            self.context.current_token_coordinate, Level::Error)
                            ]
                        );
                    }

                    // Add protection to data elements and their data flow edges
                    permitted_sdes.iter().for_each(|sde_id| {
                        let sde = &mut self.graph.data_elements[*sde_id];
                        sde.data_element.iter().for_each(|node_id| {
                            let node = &mut self.graph.nodes[*node_id];
                            node.fill_color = pe_bpmn.meta.fill_color.clone();
                            node.stroke_color = pe_bpmn.meta.stroke_color.clone();
                            node.set_pebpmn_protection(PeBpmnProtection::SecureChannel);
                            for edge_id in node.incoming.iter().chain(&node.outgoing) {
                                self.graph.edges[*edge_id].set_pebpmn_protection(
                                    sde_id,
                                    PeBpmnProtection::SecureChannel,
                                );
                                self.graph.edges[*edge_id].stroke_color =
                                    pe_bpmn.meta.stroke_color.clone();
                            }
                        });
                    });

                    // Add protection to message flows transporting data elements
                    for edge in self
                        .graph
                        .edges
                        .iter_mut()
                        .filter(|edge| edge.is_message_flow())
                    {
                        let sde_ids: Vec<_> = edge
                            .get_transported_data()
                            .iter()
                            .copied()
                            .filter(|sde_id| permitted_sdes.contains(sde_id))
                            .collect();

                        for sde_id in sde_ids {
                            edge.set_pebpmn_protection(
                                &sde_id,
                                PeBpmnProtection::SecureChannel,
                            );
                        }
                    }
                }
            }
        }
        lexer::PeBpmnType::Tee(lexer_tee) => {
            let tee = self.parse_tee_or_mpc(lexer_tee.common, "tee")?;

            match &tee.pebpmn_type {
                PeBpmnTypeParse::Pool(pool_id) => self.parse_pebpmn_participant(
                    *pool_id,
                    &tee,
                    &pe_bpmn.meta,
                    enforce_reach_end,
                    "tee",
                    None,
                ),
                PeBpmnTypeParse::Lane { pool_id, lane_id } => self.parse_pebpmn_participant(
                    *pool_id,
                    &tee,
                    &pe_bpmn.meta,
                    enforce_reach_end,
                    "tee",
                    Some(lane_id),
                ),
                PeBpmnTypeParse::Tasks(tasks) => {
                    for task in tasks {
                        if let NodeType::RealNode {
                            pe_bpmn_hides_protection_operations,
                            ..
                        } = &mut self.graph.nodes[*task].node_type
                        {
                            *pe_bpmn_hides_protection_operations = true;
                        }
                    }
                    self.parse_pebpmn_tasks(
                        tasks.clone(),
                        &tee,
                        &pe_bpmn.meta,
                        enforce_reach_end,
                        "tee",
                    )
                }
            }?
        }
        lexer::PeBpmnType::Mpc(lexer_mpc) => {
            let mpc = self.parse_tee_or_mpc(lexer_mpc.common, "mpc")?;

            match &mpc.pebpmn_type {
                PeBpmnTypeParse::Pool(pool_id) => self.parse_pebpmn_participant(
                    *pool_id,
                    &mpc,
                    &pe_bpmn.meta,
                    enforce_reach_end,
                    "mpc",
                    None,
                ),
                PeBpmnTypeParse::Lane { pool_id, lane_id } => self.parse_pebpmn_participant(
                    *pool_id,
                    &mpc,
                    &pe_bpmn.meta,
                    enforce_reach_end,
                    "mpc",
                    Some(lane_id),
                ),
                PeBpmnTypeParse::Tasks(tasks) => {
                    for task in tasks {
                        if let NodeType::RealNode {
                            pe_bpmn_hides_protection_operations,
                            ..
                        } = &mut self.graph.nodes[*task].node_type
                        {
                            *pe_bpmn_hides_protection_operations = true;
                        }
                    }
                    self.parse_pebpmn_tasks(
                        tasks.clone(),
                        &mpc,
                        &pe_bpmn.meta,
                        enforce_reach_end,
                        "mpc",
                    )
                }
            }?
        }
    }
}

fn parse_tee_or_mpc(
    &mut self,
    lexer: lexer::ComputationCommon,
    tee_or_mpc: &str,
) -> Result<ComputationCommonParse, ParseError> {
    let in_protect = self.parse_pebpmn_nodes(&lexer.in_protect, "incoming")?;
    let in_unprotect = self.parse_pebpmn_nodes(&lexer.in_unprotect, "incoming")?;
    let out_protect = self.parse_pebpmn_nodes(&lexer.out_protect, "outgoing")?;
    let out_unprotect = self.parse_pebpmn_nodes(&lexer.out_unprotect, "outgoing")?;

    let (pebpmn_type, admin) = match lexer.pebpmn_type {
        PeBpmnSubType::Pool(pool_str) => {
            let pool_id = self.find_pool_id_or_error(&pool_str)?;
            let admin_str = lexer
                .admin
                .as_ref()
                .ok_or_else(|| self.admin_missing_error(tee_or_mpc, ""))?;
            let admin = self.find_pool_id_or_error(admin_str)?;
            (PeBpmnTypeParse::Pool(pool_id), admin)
        }
        PeBpmnSubType::Lane(lane_str) => {
            let (pool_id, lane_id) = self.context.pool_id_matcher.find_pool_and_lane_id_by_lane_name_fuzzy(&self.graph, &lane_str).ok_or_else(|| vec![(
                format!("Lane with name {lane_str} was not found in any pool. Have you defined it?"),
                self.context.current_token_coordinate,
                Level::Error,
            )])?;
            if let Some(admin_str) = &lexer.admin {
                let node_id =
                    self.find_node_id_or_error(admin_str, &format!("{tee_or_mpc}-admin"))?;
                if pool_id != self.graph.nodes[node_id].pool {
                    return Err(vec![(
                        format!(
                            "The specified {tee_or_mpc}-admin with ID {admin_str} does not match the pool of the {tee_or_mpc}-lane. In {tee_or_mpc}-lane, the pool of the {tee_or_mpc}-lane is used as the {tee_or_mpc}-admin by default. If you specify an admin explicitly, it must be the same as the lane's pool."
                        ),
                        self.context.current_token_coordinate,
                        Level::Error,
                    )])?;
                }
            }
            (PeBpmnTypeParse::Lane { pool_id, lane_id }, pool_id)
        }
        PeBpmnSubType::Tasks(tasks) => {
            let task_ids = tasks
                .iter()
                .map(|task_str| {
                    self.find_node_id_or_error(task_str, &format!("{tee_or_mpc}-tasks"))
                })
                .collect::<Result<Vec<NodeId>, _>>()?;

            let first_node_id = task_ids.first().ok_or_else(|| self.admin_missing_error(tee_or_mpc, &format!("In {tee_or_mpc}-tasks, the pool of the {tee_or_mpc}-tasks is used as the {tee_or_mpc}-admin by default")))?;
            let admin = self.graph.nodes[*first_node_id].pool;

            if let Some(admin_str) = &lexer.admin {
                let node_id =
                    self.find_node_id_or_error(admin_str, &format!("{tee_or_mpc}-admin"))?;
                if admin != self.graph.nodes[node_id].pool {
                    return Err(vec![(
                        format!(
                            "The specified {tee_or_mpc}-admin with ID {admin_str} does not match the pool of the {tee_or_mpc}-tasks. In {tee_or_mpc}-tasks, the pool of the {tee_or_mpc}-tasks is used as the {tee_or_mpc}-admin by default. If you specify an admin explicitly, it must be the same as the task's pool."
                        ),
                        self.context.current_token_coordinate,
                        Level::Error,
                    )])?;
                }
            }
            (PeBpmnTypeParse::Tasks(task_ids), admin)
        }
    };

    // For all in-protect nodes that the admin either owns or are unassigned, give the admin visibility A to all the SDEs those nodes carry.
    self.graph.pools[admin]
        .tee_admin_has_pe_bpmn_visibility_A_for
        .extend(
            in_protect
                .iter()
                .filter(|data_flow_annotation| {
                    data_flow_annotation.rv_source.is_none()
                        || data_flow_annotation.rv_source == Some(admin)
                })
                .flat_map(|data_flow_annotation| {
                    self.graph.nodes[data_flow_annotation.node]
                        .get_node_transported_data()
                        .iter()
                        .copied()
                }),
        );

    // For all out-unprotect nodes that the admin either owns or are unassigned, give the admin visibility H to all the SDEs those nodes carry.
    self.graph.pools[admin]
        .tee_admin_has_pe_bpmn_visibility_H_for
        .extend(
            out_unprotect
                .iter()
                .filter(|data_flow_annotation| {
                    data_flow_annotation.rv_source.is_none()
                        || data_flow_annotation.rv_source == Some(admin)
                })
                .flat_map(|data_flow_annotation| {
                    self.graph.nodes[data_flow_annotation.node]
                        .get_node_transported_data()
                        .iter()
                        .copied()
                }),
        );

    let data_without_protection =
        self.parse_data_nodes(&lexer.data_without_protection, "data-without-protection")?;

    let data_already_protected =
        self.parse_data_nodes(&lexer.data_already_protected, "data-already-protected")?;

    let external_root_access = lexer.external_root_access.iter().map(|pool_str| {
                self.context.pool_id_matcher.find_pool_id(pool_str).ok_or_else(|| vec![(
                    format!("external_root_access pool with ID ({pool_str}) was not found. Have you defined it?"),
                    self.context.current_token_coordinate,
                    Level::Error,
                )])
            }).collect::<Result<Vec<PoolId>, _>>()?;

    let all_node_ids = std::iter::empty::<&Protection>()
        .chain(in_protect.iter())
        .chain(in_unprotect.iter())
        .chain(out_protect.iter())
        .chain(out_unprotect.iter())
        .map(|data_flow_annotation| data_flow_annotation.node)
        .collect_vec();

    if !all_node_ids.iter().all_unique() {
        return Err(vec![(
            "Each ID may only be used once within the entire [pe-bpmn] block.".to_string(),
            self.context.current_token_coordinate,
            Level::Error,
        )]);
    }

    let all_sdes = all_node_ids
        .iter()
        .flat_map(|&id| {
            self.graph.nodes[id]
                .get_node_transported_data()
                .iter()
                .copied()
        })
        .collect::<HashSet<SdeId>>();

    for pool in &external_root_access {
        self.graph.pools[*pool]
            .tee_external_root_access
            .extend(all_sdes.iter().copied());
    }

    Ok(ComputationCommonParse {
        pebpmn_type,
        in_protect,
        in_unprotect,
        out_protect,
        out_unprotect,
        data_without_protection,
        data_already_protected,
        admin,
        external_root_access,
    })
}

fn parse_pebpmn_nodes(
    &mut self,
    entries: &[lexer::Protection],
    label: &str,
) -> Result<Vec<Protection>, ParseError> {
    entries
        .iter()
        .map(|entry| {
            let node_id = self.find_node_id_or_error(&entry.node, label)?;

            let rv_source = if let Some(rv_str) = &entry.rv {
                Some(self.context.pool_id_matcher.find_pool_id(rv_str).ok_or_else(|| vec![(
                    format!(
                        "ID of the pool ({rv_str}) which created the reference value was not found. Have you defined it?"
                    ),
                    self.context.current_token_coordinate,
                    Level::Error,
                )])?)
            } else {
                None
            };

            Ok(Protection { node: node_id, rv_source })
        })
        .collect()
}

fn find_node_id_or_error(&mut self, node_str: &str, label: &str) -> Result<NodeId, ParseError> {
    self.find_node_id(node_str).ok_or_else(|| {
        vec![(
            format!("{label} node with ID {node_str} was not found. Have you defined it?"),
            self.context.current_token_coordinate,
            Level::Error,
        )]
    })
}

fn find_pool_id_or_error(&self, pool_str: &str) -> Result<PoolId, ParseError> {
    self.context
        .pool_id_matcher
        .find_pool_id(pool_str)
        .ok_or_else(|| {
            vec![(
                format!("Pool with ID ({pool_str}) was not found. Have you defined it?"),
                self.context.current_token_coordinate,
                Level::Error,
            )]
        })
}

fn admin_missing_error(&self, tee_or_mpc: &str, optional_text: &str) -> ParseError {
    vec![(
        format!("{tee_or_mpc}-admin is missing. Please define it.") + optional_text,
        self.context.current_token_coordinate,
        Level::Error,
    )]
}

fn parse_data_nodes(&self, node_ids: &[String], kind: &str) -> Result<Vec<SdeId>, ParseError> {
    node_ids
        .iter()
        .map(|data_str| {
            self.find_node_id(data_str)
                .ok_or_else(|| {
                    vec![(
                        format!(
                            "{kind} node with ID {data_str} was not found. Have you defined it?"
                        ),
                        self.context.current_token_coordinate,
                        Level::Error,
                    )]
                })
                .map(|node_id| {
                    self.graph.nodes[node_id]
                        .get_data_aux()
                        .map(|aux| aux.sde_id)
                        .expect("All data objects have SdeIds")
                })
        })
        .collect()
}

fn parse_pebpmn_participant(
    &mut self,
    pool_id: PoolId,
    common: &ComputationCommonParse,
    meta: &PeBpmnMeta,
    enforce_reach_end: bool,
    tee_or_mpc: &str,
    lane_id: Option<&LaneId>,
) -> Result<(), ParseError> {
    if let Some(lane_id) = lane_id {
        self.graph.pools[pool_id].lanes[*lane_id].fill_color = meta.fill_color.clone();
        self.graph.pools[pool_id].lanes[*lane_id].stroke_color = meta.stroke_color.clone();
    } else {
        if common.admin == pool_id {
            return Err(vec![(
                format!(
                    "The pool already represents the {tee_or_mpc}. It cannot represent the administrator at the same time. Change the pool id of {tee_or_mpc}-pool or {tee_or_mpc}-admin."
                ),
                self.context.current_token_coordinate,
                Level::Error,
            )]);
        }
        self.graph.pools[pool_id].fill_color = meta.fill_color.clone();
        self.graph.pools[pool_id].stroke_color = meta.stroke_color.clone();
    }

    if let Some(lane_id) = lane_id {
        if common.in_protect.iter().any(|node| {
            self.graph.nodes[node.node].pool == pool_id
                && self.graph.nodes[node.node].lane == *lane_id
        }) {
            return Err(vec![(
                format!("'{tee_or_mpc}-in-protect' is not allowed inside {tee_or_mpc} lane."),
                self.context.current_token_coordinate,
                Level::Error,
            )]);
        }

        if common.out_unprotect.iter().any(|node| {
            self.graph.nodes[node.node].pool == pool_id
                && self.graph.nodes[node.node].lane == *lane_id
        }) {
            return Err(vec![(
                format!(
                    "'{tee_or_mpc}-out-unprotect' is not allowed inside {tee_or_mpc} lane."
                ),
                self.context.current_token_coordinate,
                Level::Error,
            )]);
        }
    } else {
        if common
            .in_protect
            .iter()
            .any(|node| self.graph.nodes[node.node].pool == pool_id)
        {
            return Err(vec![(
                format!("'{tee_or_mpc}-in-protect' is not allowed inside {tee_or_mpc} pool."),
                self.context.current_token_coordinate,
                Level::Error,
            )]);
        }

        if common
            .out_unprotect
            .iter()
            .any(|node| self.graph.nodes[node.node].pool == pool_id)
        {
            return Err(vec![(
                format!(
                    "'{tee_or_mpc}-out-unprotect' is not allowed inside {tee_or_mpc} pool."
                ),
                self.context.current_token_coordinate,
                Level::Error,
            )]);
        }
    }

    let protection = match tee_or_mpc {
        "tee" => PeBpmnProtection::Tee,
        "mpc" => PeBpmnProtection::Mpc,
        _ => {
            unreachable!()
        }
    };

    let mut check_protected = |protect_nodes: &Vec<Protection>,
                               unprotect_nodes: &Vec<Protection>,
                               is_reverse: bool|
     -> Result<(), ParseError> {
        let unprotected_nodes = unprotect_nodes.iter().map(|n| n.node).collect_vec();
        for node in protect_nodes {
            let node_transported_data: HashSet<SdeId> = self.graph.nodes[node.node]
                .get_node_transported_data()
                .iter()
                .cloned()
                .collect();

            self.check_protection_paths(
                node.node,
                is_reverse,
                meta,
                &unprotected_nodes,
                &protection,
                |sde_id| node_transported_data.contains(sde_id),
                true,
                enforce_reach_end,
                tee_or_mpc,
            )?
        }
        Ok(())
    };

    check_protected(&common.in_protect, &common.in_unprotect, false)?;
    check_protected(&common.out_protect, &common.out_unprotect, true)?;

    Ok(())
}

fn parse_pebpmn_tasks(
    &mut self,
    tasks: Vec<NodeId>,
    tee: &ComputationCommonParse,
    meta: &PeBpmnMeta,
    enforce_reach_end: bool,
    tee_or_mpc: &str,
) -> Result<(), ParseError> {
    let end_out = tee
        .out_unprotect
        .iter()
        .map(|tee_node| tee_node.node)
        .collect_vec();
    if !tee.out_protect.is_empty() {
        return Err(vec![(
            format!(
                "{tee_or_mpc}-tasks does not allow the {tee_or_mpc}-out-protect attribute, as created data is implicitly protected. You may opt-out of encryption using the {tee_or_mpc}-already-protected attribute."
            ),
            self.context.current_token_coordinate,
            Level::Error,
        )]);
    }

    let end_in = tee
        .in_protect
        .iter()
        .map(|tee_node| tee_node.node)
        .collect_vec();
    if !tee.in_unprotect.is_empty() {
        return Err(vec![(
            format!(
                "{tee_or_mpc}-tasks does not allow the {tee_or_mpc}-in-unprotect attribute, as created data is implicitly protected. You may opt-out of encryption using the {tee_or_mpc}-already-protected attribute."
            ),
            self.context.current_token_coordinate,
            Level::Error,
        )]);
    }

    let protection = match tee_or_mpc {
        "tee" => {
            let unique_pools: Vec<_> = tasks
                .iter()
                .map(|id| self.graph.nodes[*id].pool)
                .unique()
                .collect();

            if unique_pools.len() != 1 {
                return Err(vec![(
                    "All tee-tasks must be in the same pool.".to_string(),
                    self.context.current_token_coordinate,
                    Level::Error,
                )]);
            }
            PeBpmnProtection::Tee
        }
        "mpc" => PeBpmnProtection::Mpc,
        _ => {
            unreachable!()
        }
    };
    for node_id in tasks {
        let node = &mut self.graph.nodes[node_id];
        node.fill_color = meta.fill_color.clone();
        node.stroke_color = meta.stroke_color.clone();
        node.set_pebpmn_protection(protection.clone());

        for (is_reverse, ends) in [(false, end_out.as_slice()), (true, end_in.as_slice())] {
            self.check_protection_paths(
                node_id,
                is_reverse,
                meta,
                ends,
                &protection,
                |sde_id| !tee.data_without_protection.contains(sde_id),
                false,
                enforce_reach_end,
                tee_or_mpc,
            )?
        }
    }

    Ok(())
}

fn protection_channel(
    &mut self,
    edge_id: EdgeId,
    meta: &PeBpmnMeta,
    cur_sde: &SdeId,
    end: &[NodeId],
    visited: &mut HashSet<NodeId>,
    is_reverse: bool,
    protection: &PeBpmnProtection,
) -> Result<bool, ParseError> {
    // TODO this function should do a more broad graph traversal:
    // On a data element, follow all (in and out) edges. On a node, (1) if coming in (through MF or
    // DF) then follow all DF and MF which are in or out of that node. (2) If it came backwards
    // through outgoing, then only traverse through other in/out of that node if there was some
    // incoming of that data into the node (transported_data contains the sde), as otherwise
    // this node produced the data, and it is totally possible that it produced a protected and
    // a non-protected version of the data.
    let edge = &mut self.graph.edges[edge_id];
    let node = if is_reverse { edge.from } else { edge.to };

    let is_data_or_msg = edge.is_message_flow() || edge.is_data_flow();
    let transports_sde = edge.get_transported_data().contains(cur_sde);

    if !is_data_or_msg {
        return Ok(false);
    }

    if !transports_sde {
        if end.contains(&node) {
            return self.missing_sde_id_error(node, is_reverse).map(|_| false);
        }
        return Ok(false);
    }

    if edge.is_message_flow() {
        edge.set_pebpmn_protection(cur_sde, protection.clone());
    }
    edge.stroke_color = meta.stroke_color.clone();

    // End node found
    if end.contains(&node) {
        let check_edges = if is_reverse {
            &self.graph.nodes[node].incoming
        } else {
            &self.graph.nodes[node].outgoing
        };
        if !check_edges.iter().any(|edge_id| {
            self.graph.edges[*edge_id]
                .get_transported_data()
                .contains(cur_sde)
        }) {
            return self.missing_sde_id_error(node, is_reverse).map(|_| false);
        }
        return Ok(true);
    }

    // Prevent infinite loop
    if !visited.insert(node) {
        return Ok(false);
    }

    let mut next_edges: Vec<EdgeId> = vec![];
    if let Some(node) = self.graph.nodes.get_mut(node.0) {
        if node.is_data() {
            node.fill_color = meta.fill_color.clone();
            node.stroke_color = meta.stroke_color.clone();
            node.set_pebpmn_protection(protection.clone());
        }

        next_edges = if is_reverse {
            node.incoming.clone()
        } else {
            node.outgoing.clone()
        };
    }

    let mut reach_end = false;
    for next_edge_id in next_edges {
        if self.protection_channel(
            next_edge_id,
            meta,
            cur_sde,
            end,
            visited,
            is_reverse,
            protection,
        )? {
            reach_end = true;
        };
    }
    Ok(reach_end)
}

#[track_caller]
fn check_protection_paths(
    &mut self,
    node_id: NodeId,
    is_reverse: bool,
    meta: &PeBpmnMeta,
    ends: &[NodeId],
    protection_type: &PeBpmnProtection,
    filter: impl Fn(&SdeId) -> bool,
    is_pool: bool,
    enforce_reach_end: bool,
    pe_bpmn_type: &str,
) -> Result<(), ParseError> {
    let edges = if is_reverse {
        &self.graph.nodes[node_id].incoming
    } else {
        &self.graph.nodes[node_id].outgoing
    };
    let node_transportated_data = dbg!(self.graph.nodes[node_id].get_node_transported_data());

    if edges
        .iter()
        .map(|edge_id| &self.graph.edges[*edge_id])
        .any(|edge| {
            (edge.is_message_flow() || edge.is_data_flow()) && {
                edge.get_transported_data()
                    .iter()
                    .any(|sde_id| !node_transportated_data.contains(dbg!(sde_id)))
            }
        })
    {
        // TODO understand and document this.
        return self.missing_sde_id_error(node_id, is_reverse);
    }

    let mut visited = HashSet::new();
    for edge_id in edges.clone() {
        dbg!(&self.graph, edge_id);
        let transported_data: HashSet<SdeId> = self.graph.edges[edge_id]
            .get_transported_data()
            .iter()
            .copied()
            .collect();
        dbg!(&transported_data);
        for sde_id in transported_data.iter().filter(|sde_id| filter(sde_id)) {
            // This is a new search, so reset the `visited` buffer.
            visited.clear();
            visited.insert(node_id);

            if !self.protection_channel(
                edge_id,
                meta,
                sde_id,
                ends,
                &mut visited,
                is_reverse,
                protection_type,
            )? && enforce_reach_end
            {
                self.create_protection_error_message(
                    node_id,
                    is_reverse,
                    is_pool,
                    protection_type,
                    pe_bpmn_type,
                )?
            }
        }
    }
    Ok(())
}

// A node that applies protection to some data object should have this data object also
// attached as an incoming data object. I.e. it should visually travel through the node and be
// transformed from an unprotected icon to a protected icon.
// And vice versa for unprotect nodes.
fn check_that_protection_is_visually_applied(
    &self,
    node_id: NodeId,
    // The specific `(tee-in-protect ..)` (or similar) line.
    pe_protection_tc: TokenCoordinate,
    is_reverse: bool,
    filter: impl Fn(&SdeId) -> bool,
) -> Result<(), ParseError> {
    assert!(
        matches!(&self.graph.nodes[node_id].node_type, NodeType::RealNode {
        pe_bpmn_hides_protection_operations,
        ..
    } if !pe_bpmn_hides_protection_operations),
        "got node: {:?}\n{:?}",
        &self.graph.nodes[node_id],
        &self.graph
    );

    let edges = if is_reverse {
        &self.graph.nodes[node_id].incoming
    } else {
        &self.graph.nodes[node_id].outgoing
    };

    let node_transported_data = self.graph.nodes[node_id].get_node_transported_data();

    if let Some(detached_sde_id) = edges
        .iter()
        .flat_map(|edge_id| self.graph.edges[*edge_id].get_transported_data())
        .cloned()
        .filter(filter)
        .find(|sde_id| !node_transported_data.contains(sde_id))
    {
        let full_data_declaration = self.graph.data_elements[detached_sde_id]
            .data_element
            .iter()
            .fold(
                TokenCoordinate {
                    start: usize::MAX,
                    end: usize::MIN,
                },
                |acc, el| {
                    let NodeType::RealNode { tc, .. } = &self.graph.nodes[*el].node_type else {
                        unreachable!()
                    };
                    TokenCoordinate {
                        start: acc.start.min(tc.start),
                        end: acc.end.max(tc.end),
                    }
                },
            );
        let NodeType::RealNode { tc, .. } = &self.graph.nodes[node_id].node_type else {
            unreachable!()
        };
        // Maybe this text needs to be modified one day to account for message flows, but I
        // think this is not relevant. Just will become more complicated to provide a hyper
        // specific error description (though it would help make the tool more usable ...).
        let error_string = if is_reverse {
            "This unprotect node has an incoming (protected) data element (`OD ... ->@node_id` or similar is present), but the same (unprotected) data object is missing on the outgoing side (`OD ... <-@node_id` or similar is missing). The unprotected object must be visually present to not confuse the reader."
        } else {
            "This protect node has an outgoing (protected) data element (`OD ... <-@node_id` or similar is present), but the same (unprotected) data object is missing on the incoming side (`OD ... ->@node_id` or similar is missing). The unprotected object must be visually present to not confuse the reader."
        };
        Err(vec![
            (error_string.to_string(), *tc, Level::Error),
            (
                "The PE-BPMN instruction is here".to_string(),
                pe_protection_tc,
                Level::Info,
            ),
            (
                "The data element is declared here".to_string(),
                full_data_declaration,
                Level::Info,
            ),
        ])
    } else {
        Ok(())
    }
}

pub fn create_transported_data(&mut self) {
    // Create transported data hashmap
    for edge_id in 0..self.graph.edges.len() {
        if !self.graph.edges[edge_id].is_message_flow() {
            continue;
        }
        let edge = &self.graph.edges[edge_id];

        let incoming_nodes: HashSet<NodeId> = self.graph.nodes[edge.from]
            .incoming
            .iter()
            .map(|e| self.graph.edges[*e].clone())
            .filter(|e| e.is_data_flow())
            .map(|e| e.from)
            .collect();

        let incoming_sde: HashSet<SdeId> = incoming_nodes
            .iter()
            .filter_map(|node_id| self.graph.nodes[*node_id].get_data_aux())
            .map(|aux| aux.sde_id)
            .collect();

        let outgoing_nodes: HashSet<NodeId> = self.graph.nodes[edge.to]
            .outgoing
            .iter()
            .map(|e| self.graph.edges[*e].clone())
            .filter(|e| e.is_data_flow())
            .map(|e| e.to)
            .collect();

        let outgoing_sde: HashSet<SdeId> = outgoing_nodes
            .iter()
            .filter_map(|node_id| self.graph.nodes[*node_id].get_data_aux())
            .map(|aux| aux.sde_id)
            .collect();

        let intersection: Vec<SdeId> =
            incoming_sde.intersection(&outgoing_sde).cloned().collect();

        if let FlowType::MessageFlow(message_flow_aux) =
            &mut self.graph.edges[edge_id].flow_type
        {
            message_flow_aux.transported_data.extend(intersection);
        }
    }

    for i in 0..self.graph.nodes.len() {
        let node = &self.graph.nodes[i];

        if node.is_gateway() {
            continue;
        }

        let incoming = self.get_edge_data_ids(&node.incoming);
        let outgoing = self.get_edge_data_ids(&node.outgoing);

        let intersect: Vec<SdeId> = incoming.intersection(&outgoing).copied().collect();
        self.graph.nodes[i].add_node_transported_data(&intersect);
    }
}

fn get_edge_data_ids(&self, edge_ids: &[EdgeId]) -> HashSet<SdeId> {
    let mut ids = HashSet::new();

    for edge_id in edge_ids {
        let edge = &self.graph.edges[*edge_id];
        ids.extend(edge.get_transported_data());
    }
    ids
}

fn check_connection(
    &self,
    node_id: NodeId,
    name: String,
    permitted_sdes: &HashSet<SdeId>,
    is_sender: bool,
) -> Result<(), ParseError> {
    for sde_id in permitted_sdes {
        let sde = &self.graph.data_elements[*sde_id];

        let is_connected = sde.data_element.iter().any(|&node_node_id| {
            let edge_ids = if is_sender {
                &self.graph.nodes[node_id].incoming
            } else {
                &self.graph.nodes[node_id].outgoing
            };

            edge_ids.iter().any(|&eid| {
                let edge = &self.graph.edges[eid];
                if is_sender {
                    edge.from == node_node_id
                } else {
                    edge.to == node_node_id
                }
            })
        });

        let data_element = if sde.name.is_empty() {
            "Data element".to_string()
        } else {
            format!("Data element '{}'", sde.name)
        };

        let display_name = self.graph.nodes[node_id]
            .display_text()
            .map(|s| s.to_owned())
            .unwrap_or("@".to_owned() + &name);

        if !is_connected {
            let direction = if is_sender { "sender" } else { "receiver" };
            return Err(vec![(
                format!(
                    "{data_element} is not connected to the {direction} node '{display_name}'",
                ),
                self.context.current_token_coordinate,
                Level::Error,
            )]);
        }
    }

    Ok(())
}

fn create_protection_error_message(
    &self,
    node_id: NodeId,
    is_reverse: bool,
    is_pool: bool,
    protection: &PeBpmnProtection,
    pe_bpmn_type: &str,
) -> Result<(), ParseError> {
    let node_str = self.graph.nodes[node_id]
        .display_text()
        .map_or_else(|| " ".to_string(), |text| format!(" '{text}' "));

    let error_message = if matches!(protection, PeBpmnProtection::SecureChannel) {
        format!(
            "The {} node{node_str}does not reach the {} node in the secure channel path. Make sure there is a full path.",
            if is_reverse { "receiver" } else { "sender" },
            if is_reverse { "sender" } else { "receiver" }
        )
    } else if is_pool {
        let direction = match is_reverse {
            true => "out",
            false => "in",
        };
        format!(
            "'{pe_bpmn_type}-{direction}-protect' node{node_str}does not reach any '{pe_bpmn_type}-{direction}-unprotect' nodes in the {pe_bpmn_type} path. Make sure there is a full path.",
        )
    } else {
        let (direction, prot) = match is_reverse {
            true => ("in", "protect"),
            false => ("out", "unprotect"),
        };
        format!(
            "'{pe_bpmn_type}-tasks' node{node_str}has a data element that does not reach '{pe_bpmn_type}-{direction}-{prot}' node in the {pe_bpmn_type} path. Make sure there is a full path or exclude it with '{pe_bpmn_type}-data-without-protection'.",
        )
    };

    Err(vec![(
        error_message,
        self.context.current_token_coordinate,
        Level::Error,
    )])
}

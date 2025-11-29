use itertools::Itertools;
use std::collections::HashMap;

use crate::common::graph::{LaneId, PoolId};
use crate::common::node::NodeType;
use crate::lexer::TokenCoordinate;
use crate::lexer::{self, PeBpmnProtection};
use crate::parser::Parser;
use crate::{
    common::graph::{NodeId, SdeId},
    lexer::PeBpmnMeta,
    parser::ParseError,
};

#[derive(Debug, Clone, PartialEq)]
pub struct PeBpmn {
    pub r#type: PeBpmnType,
    pub meta: PeBpmnMeta, // stroke color, etc
}

#[derive(Debug, Clone, PartialEq)]
pub enum PeBpmnType {
    SecureChannel(SecureChannel),
    //SecureChannelWithExplicitSecret(SecureChannelWithExplicitSecret),
    Tee(Tee),
    Mpc(Mpc),
}

impl PeBpmnType {
    pub fn protection(&self) -> PeBpmnProtection {
        match self {
            Self::SecureChannel(inner) => PeBpmnProtection::SecureChannel(inner.tc),
            Self::Tee(inner) => PeBpmnProtection::Tee(inner.common.tc),
            Self::Mpc(inner) => PeBpmnProtection::Mpc(inner.common.tc),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SecureChannel {
    pub sender: Option<(NodeId, TokenCoordinate)>,
    pub receiver: Option<(NodeId, TokenCoordinate)>,
    pub permitted_ids: Vec<(SdeId, TokenCoordinate)>,
    pub tc: TokenCoordinate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tee {
    pub common: ComputationCommon,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mpc {
    pub common: ComputationCommon,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputationCommon {
    pub pebpmn_type: PeBpmnSubType,

    pub in_protect: Vec<Protection>,
    pub in_unprotect: Vec<Protection>,
    pub out_protect: Vec<Protection>,
    pub out_unprotect: Vec<Protection>,

    pub data_without_protection: Vec<(SdeId, TokenCoordinate)>,
    /// TODO this is not used anywhere, yet.
    pub data_already_protected: Vec<(SdeId, TokenCoordinate)>,
    pub admin: PoolId,
    pub external_root_access: Vec<PoolId>,

    pub tc: TokenCoordinate,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PeBpmnSubType {
    // TODO Pool and Lane need to be Vecs because MPC is on multiple pools and lanes.
    Pool(PoolId),
    Lane { pool_id: PoolId, lane_id: LaneId },
    // They are all part of the same lane ... Or pool? TODO
    Tasks(Vec<(NodeId, TokenCoordinate)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Protection {
    pub node: NodeId,
    pub rv_source: Option<PoolId>,
    // The whole `(tee-in-protect ...)`
    pub tc: TokenCoordinate,
}

impl Parser {
    pub fn parse_pe_bpmn(&mut self, pe_bpmn: lexer::PeBpmn) -> Result<(), ParseError> {
        let r#type = match pe_bpmn.r#type {
            lexer::PeBpmnType::SecureChannel(secure_channel) => {
                let sender_id = secure_channel
                    .sender
                    .as_ref()
                    .map_or(Ok(None), |(sender_name, tc)| {
                        self.find_node_id(sender_name).map(|sender_id| Some((sender_id, *tc))).ok_or(vec![(
                            format!("Sender node with ID ({sender_name}) was not found. Have you defined it?"),
                            *tc,

                        )])
                    })?;

                let receiver_id = secure_channel
                    .receiver
                    .as_ref()
                    .map_or(Ok(None), |(receiver_name, tc)| {
                        self.find_node_id(receiver_name).map(|receiver_id| Some((receiver_id, *tc))).ok_or(vec![(
                            format!("Receiver node with ID ({receiver_name}) was not found. Have you defined it?"),
                            *tc,

                        )])
                    })?;

                let permitted_sdes: HashMap<SdeId, TokenCoordinate> = secure_channel
                    .argument_ids
                    .iter()
                    .map(|(string_id, tc)| {
                        let node_id = self.find_node_id(string_id).ok_or(vec![(
                            format!("Data element with ID ({string_id}) was not found. Have you defined it?"),
                            *tc,

                        )])?;
                        let target_node = &self.graph.nodes[node_id];
                        if !target_node.is_data() {
                            return Err(vec![(
                                format!("Only Data elements IDs are allowed! Node with ID ({string_id}) is not a data element"),
                                *tc,

                            ), (format!("This is the node which was selected by your `@{string_id}` selector"),target_node.tc(), ),]);
                        }
                        Ok(self.graph
                            .data_elements
                            .iter()
                            .position(|sde| sde.contains(node_id))
                            .map(SdeId)
                        .map(|sde_id| (sde_id,*tc)))
                    })
                    .filter_map(Result::transpose)
                    .collect::<Result<_, _>>()?;

                if sender_id.is_none() && receiver_id.is_none() && permitted_sdes.is_empty() {
                    return Err(vec![
                                ("You need to define IDs when you use pre-sent and post-received simultaneously, like:\n\t(secure-channel pre-sent post-received @data_to_send)".to_string(),
                                self.context.current_token_coordinate, )
                                ]
                            );
                }
                PeBpmnType::SecureChannel(SecureChannel {
                    sender: sender_id,
                    receiver: receiver_id,
                    permitted_ids: permitted_sdes.into_iter().collect(),
                    tc: secure_channel.tc,
                })
            }
            lexer::PeBpmnType::Tee(lexer::Tee { common }) => PeBpmnType::Tee(Tee {
                common: self.parse_tee_or_mpc(common, "tee")?,
            }),
            lexer::PeBpmnType::Mpc(lexer::Mpc { common }) => PeBpmnType::Mpc(Mpc {
                common: self.parse_tee_or_mpc(common, "mpc")?,
            }),
        };

        self.graph.pe_bpmn_definitions.push(PeBpmn {
            r#type,
            meta: pe_bpmn.meta,
        });

        Ok(())
    }

    fn parse_tee_or_mpc(
        &mut self,
        common: lexer::ComputationCommon,
        tee_or_mpc: &str,
    ) -> Result<ComputationCommon, ParseError> {
        let computation_common = self.parse_tee_or_mpc_inner(common, tee_or_mpc)?;

        match &computation_common.pebpmn_type {
            &PeBpmnSubType::Pool(..) => self.verify_pebpmn_pool(&computation_common, tee_or_mpc),
            &PeBpmnSubType::Lane { .. } => self.verify_pebpmn_lane(&computation_common, tee_or_mpc),
            PeBpmnSubType::Tasks(..) => self.verify_pebpmn_tasks(&computation_common, tee_or_mpc),
        };
        Ok(computation_common)
    }

    fn parse_tee_or_mpc_inner(
        &mut self,
        lexer: lexer::ComputationCommon,
        tee_or_mpc: &str,
    ) -> Result<ComputationCommon, ParseError> {
        let in_protect = self.parse_pebpmn_nodes(&lexer.in_protect, "incoming")?;
        let in_unprotect = self.parse_pebpmn_nodes(&lexer.in_unprotect, "incoming")?;
        let out_protect = self.parse_pebpmn_nodes(&lexer.out_protect, "outgoing")?;
        let out_unprotect = self.parse_pebpmn_nodes(&lexer.out_unprotect, "outgoing")?;

        let (pebpmn_type, admin) = match lexer.pebpmn_type {
            lexer::PeBpmnSubType::Pool(pool_str, _tc) => {
                let pool_id = self.find_pool_id_or_error(&pool_str)?;
                let (admin_str, _admin_tc) = lexer
                    .admin
                    .as_ref()
                    .ok_or_else(|| self.admin_missing_error(tee_or_mpc, ""))?;
                let admin = self.find_pool_id_or_error(admin_str)?;
                (PeBpmnSubType::Pool(pool_id), admin)
            }
            lexer::PeBpmnSubType::Lane(lane_str, lane_tc) => {
                let (pool_id, lane_id) = self.context.pool_id_matcher.find_pool_and_lane_id_by_lane_name_fuzzy(&self.graph, &lane_str).ok_or_else(|| vec![(
                    format!("Lane with name {lane_str} was not found in any pool. Have you defined it?"),
                    lane_tc,

                )])?;
                if let Some((admin_str, admin_tc)) = &lexer.admin {
                    let node_id = self.find_node_id_or_error(
                        admin_str,
                        &format!("{tee_or_mpc}-admin"),
                        *admin_tc,
                    )?;
                    let expected_pool_id = self.graph.nodes[node_id].pool;
                    if pool_id != expected_pool_id {
                        return Err(vec![
                            (
                                format!(
                                    "The specified {tee_or_mpc}-admin with ID {admin_str} does not match the pool of the {tee_or_mpc}-lane. In {tee_or_mpc}-lane, the pool of the {tee_or_mpc}-lane is used as the {tee_or_mpc}-admin by default. If you specify an admin explicitly, it must be the same as the lane's pool."
                                ),
                                *admin_tc,
                            ),
                            (
                                "This would be the correct pool".to_string(),
                                self.graph.pools[expected_pool_id].tc,
                            ),
                            (
                                "But you specified this one".to_string(),
                                self.graph.pools[pool_id].tc,
                            ),
                        ])?;
                    }
                }
                (PeBpmnSubType::Lane { pool_id, lane_id }, pool_id)
            }
            lexer::PeBpmnSubType::Tasks(tasks) => {
                let task_ids: Vec<(NodeId, TokenCoordinate)> = tasks
                    .iter()
                    .map(|(task_str, tc)| {
                        self.find_node_id_or_error(task_str, &format!("{tee_or_mpc}-tasks"), *tc)
                            .map(|node_id| (node_id, *tc))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let first_node_id = task_ids.first().ok_or_else(|| self.admin_missing_error(tee_or_mpc, &format!("In {tee_or_mpc}-tasks, the pool of the {tee_or_mpc}-tasks is used as the {tee_or_mpc}-admin by default")))?;
                let admin = self.graph.nodes[first_node_id.0].pool;

                if let Some((admin_str, tc)) = &lexer.admin {
                    let node_id =
                        self.find_node_id_or_error(admin_str, &format!("{tee_or_mpc}-admin"), *tc)?;
                    let expected_pool_id = self.graph.nodes[node_id].pool;
                    if admin != expected_pool_id {
                        return Err(vec![
                            (
                                format!(
                                    "The specified {tee_or_mpc}-admin with ID {admin_str} does not match the pool of the {tee_or_mpc}-tasks. In {tee_or_mpc}-tasks, the pool of the {tee_or_mpc}-tasks is used as the {tee_or_mpc}-admin by default. If you specify an admin explicitly, it must be the same as the task's pool."
                                ),
                                self.context.current_token_coordinate,
                            ),
                            (
                                "This would be the correct pool".to_string(),
                                self.graph.pools[expected_pool_id].tc,
                            ),
                            (
                                "But you specified this one".to_string(),
                                self.graph.pools[admin].tc,
                            ),
                        ])?;
                    }
                }
                (PeBpmnSubType::Tasks(task_ids), admin)
            }
        };

        let data_without_protection =
            self.parse_data_nodes(&lexer.data_without_protection, "data-without-protection")?;

        let data_already_protected =
            self.parse_data_nodes(&lexer.data_already_protected, "data-already-protected")?;

        let external_root_access = lexer.external_root_access.iter().map(|(pool_str, pool_tc)| {
                    self.context.pool_id_matcher.find_pool_id(pool_str).ok_or_else(|| vec![(
                        format!("external_root_access pool with ID ({pool_str}) was not found. Have you defined it?"),
                        *pool_tc,

                    )])
                }).collect::<Result<Vec<PoolId>, _>>()?;

        let all_node_ids = std::iter::empty::<&Protection>()
            .chain(in_protect.iter())
            .chain(in_unprotect.iter())
            .chain(out_protect.iter())
            .chain(out_unprotect.iter())
            .map(|data_flow_annotation| (data_flow_annotation.node, data_flow_annotation.tc))
            .collect::<Vec<_>>();

        {
            let mut set = HashMap::new();
            for (node_id, tc) in &all_node_ids {
                if let Some(old_duplicate_tc) = set.remove(node_id) {
                    return Err(vec![(
                        "Each ID may only be used once within the entire [pe-bpmn] block, but this one has been used twice".to_string(),
                        *tc,

                    ), (
                        "This was the first use of the same ID".to_string(),
                            old_duplicate_tc,
                    )]);
                }
                set.insert(node_id, *tc);
            }
        }

        Ok(ComputationCommon {
            pebpmn_type,
            in_protect,
            in_unprotect,
            out_protect,
            out_unprotect,
            data_without_protection,
            data_already_protected,
            admin,
            external_root_access,
            tc: lexer.tc,
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
                let node_id = self.find_node_id_or_error(&entry.node, label, entry.tc)?;

                let rv_source = if let Some(rv_str) = &entry.rv {
                    Some(self.context.pool_id_matcher.find_pool_id(rv_str).ok_or_else(|| vec![(
                        format!(
                            "ID of the pool ({rv_str}) which created the reference value was not found. Have you defined it?"
                        ),
                        self.context.current_token_coordinate,

                    )])?)
                } else {
                    None
                };

                Ok(Protection { node: node_id, rv_source, tc: entry.tc })
            })
            .collect()
    }

    fn find_node_id_or_error(
        &mut self,
        node_str: &str,
        label: &str,
        tc: TokenCoordinate,
    ) -> Result<NodeId, ParseError> {
        self.find_node_id(node_str).ok_or_else(|| {
            vec![(
                format!("{label} node with ID {node_str} was not found. Have you defined it?"),
                tc,
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
                )]
            })
    }

    fn admin_missing_error(&self, tee_or_mpc: &str, optional_text: &str) -> ParseError {
        vec![(
            format!("{tee_or_mpc}-admin is missing. Please define it.") + optional_text,
            self.context.current_token_coordinate,
        )]
    }

    fn parse_data_nodes(
        &self,
        node_ids: &[(String, TokenCoordinate)],
        kind: &str,
    ) -> Result<Vec<(SdeId, TokenCoordinate)>, ParseError> {
        let mut uniqueness_set = HashMap::<SdeId, TokenCoordinate>::new();
        node_ids
            .iter()
            .map(|(data_str, tc)| {
                self.find_node_id(data_str)
                    .ok_or_else(|| {
                        vec![(
                            format!(
                                "{kind} node with ID {data_str} was not found. Have you defined it?"
                            ),
                            *tc,

                        )]
                    })
                    .map(|node_id| {
                        let node = &self.graph.nodes[node_id];
                        node
                            .get_data_aux()
                            .map(|aux| (aux.sde_id, *tc, node.tc()))
                            .expect("All data objects have SdeIds")
                    }).and_then(|(sde_id, tc, sde_tc)| {

                if let Some(old_tc) = uniqueness_set.remove(&sde_id) {
                    return Err(vec![(
                        "This is referencing the same semantic data element as a previous ID, this is not allowed. It is sufficient to just specify it once, so you can simply omit this ID".to_string(),
                        tc,
                    ),
                    ("Previous reference to the same semantic data element is this one".to_string(),
                            old_tc, ),
                        ("This is the semantic data element which both reference".to_string(), sde_tc, ),
                    ]);
                }
            uniqueness_set.insert(sde_id, tc);
                        Ok((sde_id, tc))
                    })
            })
            .collect()
    }

    fn verify_pebpmn_pool(
        &mut self,
        common: &ComputationCommon,
        tee_or_mpc: &str,
    ) -> Result<(), ParseError> {
        let &PeBpmnSubType::Pool(pool_id) = &common.pebpmn_type else {
            // Guaranteed by the caller.
            unreachable!();
        };
        if common.admin == pool_id {
            return Err(vec![(
                format!(
                    "The pool already represents the {tee_or_mpc}. It cannot represent the administrator at the same time. Change the pool id of {tee_or_mpc}-pool or {tee_or_mpc}-admin."
                ),
                self.context.current_token_coordinate,
            )]);
        }

        if common
            .in_protect
            .iter()
            .any(|node| self.graph.nodes[node.node].pool == pool_id)
        {
            return Err(vec![(
                format!("'{tee_or_mpc}-in-protect' is not allowed inside {tee_or_mpc} pool."),
                self.context.current_token_coordinate,
            )]);
        }

        if common
            .out_unprotect
            .iter()
            .any(|node| self.graph.nodes[node.node].pool == pool_id)
        {
            return Err(vec![(
                format!("'{tee_or_mpc}-out-unprotect' is not allowed inside {tee_or_mpc} pool."),
                self.context.current_token_coordinate,
            )]);
        }

        Ok(())
    }

    fn verify_pebpmn_lane(
        &mut self,
        common: &ComputationCommon,
        tee_or_mpc: &str,
    ) -> Result<(), ParseError> {
        let &PeBpmnSubType::Lane { pool_id, lane_id } = &common.pebpmn_type else {
            // Guaranteed by the caller.
            unreachable!();
        };

        if common.in_protect.iter().any(|node| {
            self.graph.nodes[node.node].pool == pool_id
                && self.graph.nodes[node.node].lane == lane_id
        }) {
            return Err(vec![(
                format!("'{tee_or_mpc}-in-protect' is not allowed inside {tee_or_mpc} lane."),
                self.context.current_token_coordinate,
            )]);
        }

        if common.out_unprotect.iter().any(|node| {
            self.graph.nodes[node.node].pool == pool_id
                && self.graph.nodes[node.node].lane == lane_id
        }) {
            return Err(vec![(
                format!("'{tee_or_mpc}-out-unprotect' is not allowed inside {tee_or_mpc} lane."),
                self.context.current_token_coordinate,
            )]);
        }

        Ok(())
    }

    fn verify_pebpmn_tasks(
        &mut self,
        common: &ComputationCommon,
        tee_or_mpc: &str,
    ) -> Result<(), ParseError> {
        let PeBpmnSubType::Tasks(tasks) = &common.pebpmn_type else {
            // Guaranteed by the caller.
            unreachable!();
        };
        for (task, _) in tasks {
            if let NodeType::RealNode {
                pe_bpmn_hides_protection_operations,
                ..
            } = &mut self.graph.nodes[*task].node_type
            {
                *pe_bpmn_hides_protection_operations = true;
            }
        }

        if let [prot, ..] = &common.out_protect[..] {
            return Err(vec![(
                format!(
                    "{tee_or_mpc}-tasks does not allow the {tee_or_mpc}-out-protect attribute, as created data is implicitly protected. You may opt-out of encryption using the {tee_or_mpc}-already-protected attribute."
                ),
                prot.tc,
            )]);
        }

        if let [prot, ..] = &common.in_unprotect[..] {
            return Err(vec![(
                format!(
                    "{tee_or_mpc}-tasks does not allow the {tee_or_mpc}-in-unprotect attribute, as created data is implicitly protected. You may opt-out of encryption using the {tee_or_mpc}-already-protected attribute."
                ),
                prot.tc,
            )]);
        }

        if tee_or_mpc == "tee" {
            let unique_pools: Vec<_> = tasks
                .iter()
                .map(|(id, _)| self.graph.nodes[*id].pool)
                .unique()
                .collect();

            if unique_pools.len() != 1 {
                let mut error = vec![(
                    "All tee-tasks must be in the same pool.".to_string(),
                    self.context.current_token_coordinate,
                )];
                for (node_id, node_tc) in tasks.iter() {
                    let node = &self.graph.nodes[*node_id];
                    let pool = &self.graph.pools[node.pool];
                    error.push(("This node selector ..".to_string(), *node_tc));

                    error.push((".. matches this node ..".to_string(), node.tc()));
                    error.push((".. which is part of this pool.".to_string(), pool.tc));
                }
                return Err(error);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use crate::{
        common::{
            edge::{FlowType, MessageFlowAux},
            graph::SdeId,
        },
        lexer::{PeBpmnProtection, TokenCoordinate},
        parser::{ParseError, parse},
    };

    #[test]
    fn multiple_data_elements_secure_channel_protection() -> Result<(), ParseError> {
        let input = r#"
= Pool1
# Start1
- Task1
. Forward1

= Pool2
# Receive2
- Task2
. Forward2

= Pool3
# Receive3
- Task3
. End3

MF <-forward1 ->receive2
MF <-forward2 ->receive3

OD Data Element 1 <-task1 ->forward1
 & <-receive2 ->task2
 & <-task2 ->forward2
 & <-receive3 ->task3

OD Data Element 2 <-task1 ->forward1
 & <-receive2 ->task2
 & <-task2 ->forward2
 & <-receive3 ->task3

[pe-bpmn (secure-channel @forward1 @receive3)]
    "#;

        let graph = parse(input.to_string(), &mut Vec::new())?;

        for (i, sde) in graph.data_elements.iter().enumerate() {
            let node_ids: Vec<_> = sde.data_element.iter().collect();

            assert!(
                node_ids.len() == 4,
                "Expected at 4 nodes in semantic data element"
            );

            // Expected secure-channel protection on node 1 and 2 (index 1 and 2)
            for &secure_index in &[1, 2] {
                let node_id = *node_ids
                    .get(secure_index)
                    .expect("Missing node at expected secure index");
                let node = &graph.nodes[*node_id];
                let protection = &node
                    .get_data_aux()
                    .expect("Expected data_aux")
                    .pebpmn_protection;

                assert!(
                    protection.contains(&PeBpmnProtection::SecureChannel(TokenCoordinate {
                        /* TODO */ start: 0,
                        /* TODO */ end: 0,
                        source_file_idx: 0
                    })),
                    "Expected SecureChannel protection on node '{}' in SDE {}",
                    node.id,
                    i + 1
                );
            }

            // Sanity check: other nodes (optional)
            for (j, &node_id) in node_ids.iter().enumerate() {
                if ![1, 2].contains(&j) {
                    let node = &graph.nodes[*node_id];
                    let protection = &node
                        .get_data_aux()
                        .expect("Expected data_aux")
                        .pebpmn_protection;

                    assert!(
                        protection.is_empty(),
                        "Did not expect protection on node '{}' (index {}) in SDE {}",
                        node.id,
                        j,
                        i + 1
                    );
                }
            }

            let edges = &graph
                .edges
                .iter()
                .filter(|e| e.is_message_flow())
                .collect_vec();
            for edge in edges {
                if let FlowType::MessageFlow(MessageFlowAux {
                    pebpmn_protection, ..
                }) = &edge.flow_type
                {
                    assert!(
                        pebpmn_protection
                            .iter()
                            .find(|e| e.0 == SdeId(i))
                            .unwrap()
                            .1
                            .contains(&PeBpmnProtection::SecureChannel(TokenCoordinate {
                                start: 0,
                                end: 0,
                                source_file_idx: 0
                            }))
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn secure_channel_only_applied_to_middle_nodes_of_first_sde() -> Result<(), ParseError> {
        let input = r#"
= Pool1
# Start1
- Task1
. Forward1

= Pool2
# Receive2
- Task2
. Forward2

= Pool3
# Receive3
- Task3
. End3

MF <-forward1 ->receive2
MF <-forward2 ->receive3

OD Data Element 1 <-task1 ->forward1 @data-element1
& <-receive2 ->task2
& <-task2 ->forward2
& <-receive3 ->task3

OD Data Element 2 <-task1 ->forward1
& <-receive2 ->task2
& <-task2 ->forward2
& <-receive3 ->task3

[pe-bpmn (secure-channel @forward1 @receive3 @data-element1)]
    "#;

        let graph = parse(input.to_string(), &mut Vec::new())?;

        for (sde_index, sde) in graph.data_elements.iter().enumerate() {
            assert_eq!(
                sde.data_element.len(),
                4,
                "Each SDE should contain exactly 4 nodes"
            );

            for (node_position, &node_id) in sde.data_element.iter().enumerate() {
                let node = &graph.nodes[node_id];
                let protection = &node
                    .get_data_aux()
                    .expect("Expected data_aux")
                    .pebpmn_protection;

                let should_have_protection =
                    sde_index == 0 && (node_position == 1 || node_position == 2);

                if should_have_protection {
                    assert!(
                        protection.contains(&PeBpmnProtection::SecureChannel(TokenCoordinate {
                            start: 0,
                            end: 0,
                            source_file_idx: 0
                        })),
                        "Expected SecureChannel on node '{}' in SDE {} at index {}",
                        node.id,
                        sde_index,
                        node_position
                    );
                } else {
                    assert!(
                        protection.is_empty(),
                        "Did NOT expect protection on node '{}' in SDE {} at index {}",
                        node.id,
                        sde_index,
                        node_position
                    );
                }
            }
        }
        Ok(())
    }
}

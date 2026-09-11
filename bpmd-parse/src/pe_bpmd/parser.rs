use itertools::Itertools;
use std::collections::HashMap;

use crate::common::graph::{LaneId, PoolId};
use crate::common::node::NodeType;
use crate::lexer::{self, PeBpmdProtection};
use crate::lexer::{CONTAINING_POOL_ONLY_KEYWORD, TokenCoordinate};
use crate::parser::Parser;
use crate::{
    common::graph::{NodeId, SdeId},
    lexer::PeBpmdMeta,
    parser::ParseError,
};

#[derive(Debug, Clone, PartialEq)]
pub struct PeBpmd {
    pub r#type: PeBpmdType,
    pub meta: PeBpmdMeta, // stroke color, etc
}

#[derive(Debug, Clone, PartialEq)]
pub enum PeBpmdType {
    SecureChannel(SecureChannel),
    //SecureChannelWithExplicitSecret(SecureChannelWithExplicitSecret),
    Tee(Tee),
    Mpc(Mpc),
}

impl PeBpmdType {
    pub fn protection(&self) -> PeBpmdProtection {
        match self {
            Self::SecureChannel(inner) => PeBpmdProtection::SecureChannel(inner.tc),
            Self::Tee(inner) => PeBpmdProtection::Tee(inner.common.tc),
            Self::Mpc(inner) => PeBpmdProtection::Mpc(inner.common.tc),
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
    pub pebpmd_type: PeBpmdSubType,

    pub in_protect: Vec<Protection>,
    pub in_unprotect: Vec<Protection>,
    pub out_protect: Vec<Protection>,
    pub out_unprotect: Vec<Protection>,

    pub data_without_protection: Vec<(SdeId, TokenCoordinate)>,
    /// TODO this is not used anywhere, yet.
    pub data_already_protected: Vec<(SdeId, TokenCoordinate)>,
    pub software_operators: Vec<PoolId>,
    pub hardware_operators: Vec<PoolId>,
    pub external_root_access: Vec<PoolId>,

    pub tc: TokenCoordinate,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PeBpmdSubType {
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
    pub fn parse_pe_bpmd(&mut self, pe_bpmd: lexer::PeBpmd) -> Result<(), ParseError> {
        let r#type = match pe_bpmd.r#type {
            lexer::PeBpmdType::SecureChannel(secure_channel) => {
                let sender_id = secure_channel
                    .sender
                    .as_ref()
                    .map_or(Ok(None), |(sender_name, tc)| {
                        self.context.id_matcher.find_nondata_node_id(sender_name, None).map(|sender_id| Some((sender_id, *tc))).ok_or(vec![(
                            format!("Sender node with ID ({sender_name}) was not found. Have you defined it?"),
                            *tc,

                        )])
                    })?;

                let receiver_id = secure_channel
                    .receiver
                    .as_ref()
                    .map_or(Ok(None), |(receiver_name, tc)| {
                        self.context.id_matcher.find_nondata_node_id(receiver_name, None).map(|receiver_id| Some((receiver_id, *tc))).ok_or(vec![(
                            format!("Receiver node with ID ({receiver_name}) was not found. Have you defined it?"),
                            *tc,

                        )])
                    })?;

                let permitted_sdes: HashMap<SdeId, TokenCoordinate> = secure_channel
                    .argument_ids
                    .iter()
                    .map(|(string_id, tc)| {
                        let node_id = self.context.id_matcher.find_data_node_id(string_id, None).ok_or(vec![(
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
                PeBpmdType::SecureChannel(SecureChannel {
                    sender: sender_id,
                    receiver: receiver_id,
                    permitted_ids: permitted_sdes.into_iter().collect(),
                    tc: secure_channel.tc,
                })
            }
            lexer::PeBpmdType::Tee(lexer::Tee { common }) => PeBpmdType::Tee(Tee {
                common: self.parse_tee_or_mpc(common, "tee")?,
            }),
            lexer::PeBpmdType::Mpc(lexer::Mpc { common }) => PeBpmdType::Mpc(Mpc {
                common: self.parse_tee_or_mpc(common, "mpc")?,
            }),
        };

        self.graph.pe_bpmd_definitions.push(PeBpmd {
            r#type,
            meta: pe_bpmd.meta,
        });

        Ok(())
    }

    fn parse_tee_or_mpc(
        &mut self,
        common: lexer::ComputationCommon,
        tee_or_mpc: &str,
    ) -> Result<ComputationCommon, ParseError> {
        let computation_common = self.parse_tee_or_mpc_inner(common, tee_or_mpc)?;

        match &computation_common.pebpmd_type {
            &PeBpmdSubType::Pool(..) => self.verify_pebpmd_pool(&computation_common, tee_or_mpc)?,
            &PeBpmdSubType::Lane { .. } => {
                self.verify_pebpmd_lane(&computation_common, tee_or_mpc)?
            }
            PeBpmdSubType::Tasks(..) => {
                self.verify_pebpmd_tasks(&computation_common, tee_or_mpc)?
            }
        };
        Ok(computation_common)
    }

    fn parse_tee_or_mpc_inner(
        &mut self,
        lexer: lexer::ComputationCommon,
        tee_or_mpc: &str,
    ) -> Result<ComputationCommon, ParseError> {
        let in_protect = self.parse_pebpmd_nodes(&lexer.in_protect, "incoming")?;
        let in_unprotect = self.parse_pebpmd_nodes(&lexer.in_unprotect, "incoming")?;
        let out_protect = self.parse_pebpmd_nodes(&lexer.out_protect, "outgoing")?;
        let out_unprotect = self.parse_pebpmd_nodes(&lexer.out_unprotect, "outgoing")?;
        let software_operators_tc = TokenCoordinate {
            source_file_idx: lexer
                .software_operators
                .first()
                .map(|(_, tc)| tc.source_file_idx)
                .unwrap_or(0),
            start: lexer
                .software_operators
                .first()
                .map(|(_, tc)| tc.start)
                .unwrap_or(0),
            end: lexer
                .software_operators
                .last()
                .map(|(_, tc)| tc.end)
                .unwrap_or(0),
        };

        let (pebpmd_type, software_operators, hardware_operators) = match &lexer.pebpmd_type {
            lexer::PeBpmdSubType::Pool(pool_str, _tc) => (
                PeBpmdSubType::Pool(self.find_pool_id_or_error(pool_str)?),
                self.find_and_verify_hw_sw_operators(
                    tee_or_mpc,
                    &lexer.software_operators,
                    None,
                    "software",
                )?,
                self.find_and_verify_hw_sw_operators(
                    tee_or_mpc,
                    &lexer.hardware_operators,
                    None,
                    "hardware",
                )?,
            ),
            lexer::PeBpmdSubType::Lane(lane_str, lane_tc) => {
                let (pool_id, lane_id) = self.context.id_matcher.find_lane_id(lane_str).ok_or_else(|| vec![(
                    format!("Lane with name {lane_str} was not found in any pool. Have you defined it?"),
                    *lane_tc,

                )])?;

                (
                    PeBpmdSubType::Lane { pool_id, lane_id },
                    self.find_and_verify_hw_sw_operators(
                        tee_or_mpc,
                        &lexer.software_operators,
                        Some((pool_id, "lane")),
                        "software",
                    )?,
                    self.find_and_verify_hw_sw_operators(
                        tee_or_mpc,
                        &lexer.hardware_operators,
                        Some((pool_id, "lane")),
                        "hardware",
                    )?,
                )
            }
            lexer::PeBpmdSubType::Tasks(tasks) => {
                let task_ids: Vec<(NodeId, TokenCoordinate)> = tasks
                    .iter()
                    .map(|(task_str, tc)| {
                        self.find_nondata_node_id_or_error(
                            task_str,
                            &format!("{tee_or_mpc}-tasks"),
                            *tc,
                        )
                        .map(|node_id| (node_id, *tc))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let Some(first_node_id) = task_ids.first() else {
                    return Err(vec![(
                        format!("{tee_or_mpc}-tasks needs at least one activity ID as an argument"),
                        lexer.tc,
                    )]);
                };
                let automatically_derived_software_operator =
                    self.graph.nodes[first_node_id.0].pool;

                (
                    PeBpmdSubType::Tasks(task_ids),
                    self.find_and_verify_hw_sw_operators(
                        tee_or_mpc,
                        &lexer.software_operators,
                        Some((automatically_derived_software_operator, "lane")),
                        "software",
                    )?,
                    self.find_and_verify_hw_sw_operators(
                        tee_or_mpc,
                        &lexer.hardware_operators,
                        Some((automatically_derived_software_operator, "lane")),
                        "hardware",
                    )?,
                )
            }
        };

        let data_without_protection =
            self.parse_data_nodes(&lexer.data_without_protection, "data-without-protection")?;

        let data_already_protected =
            self.parse_data_nodes(&lexer.data_already_protected, "data-already-protected")?;

        let external_root_access = lexer.external_root_access.iter().map(|(pool_str, pool_tc)| {
                    self.context.id_matcher.find_pool_id(pool_str).ok_or_else(|| vec![(
                        format!("external_root_access pool with ID ({pool_str}) was not found. Have you defined it?"),
                        *pool_tc,

                    )]).and_then(|e| if PeBpmdSubType::Pool(e) == pebpmd_type {
                        let lexer::PeBpmdSubType::Pool(_, tee_pool_tc) = lexer.pebpmd_type else {
                            unreachable!();
                        };
                        Err(vec![
                            (
                                "external_root_access is not allowed to refer to itself.".to_string(),
                                *pool_tc,
                            ),
                            (
                                "This ID refers to the same pool.".to_string(),
                                tee_pool_tc
                            ),
                            (
                                "This is the referenced pool.".to_string(),
                                self.graph.pools[e].tc
                            )
                        ])
                    } else {
                        Ok(e)
                    })
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
                        "Each ID may only be used once within the entire [pe-bpmd] block, but this one has been used twice".to_string(),
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
            pebpmd_type,
            in_protect,
            in_unprotect,
            out_protect,
            out_unprotect,
            data_without_protection,
            data_already_protected,
            software_operators,
            hardware_operators,
            external_root_access,
            tc: lexer.tc,
        })
    }

    fn parse_pebpmd_nodes(
        &mut self,
        entries: &[lexer::Protection],
        label: &str,
    ) -> Result<Vec<Protection>, ParseError> {
        entries
            .iter()
            .map(|entry| {
                let node_id = self.find_nondata_node_id_or_error(&entry.node, label, entry.tc)?;

                let rv_source = if let Some(rv_str) = &entry.rv {
                    Some(self.context.id_matcher.find_pool_id(rv_str).ok_or_else(|| vec![(
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

    fn find_nondata_node_id_or_error(
        &mut self,
        node_str: &str,
        label: &str,
        tc: TokenCoordinate,
    ) -> Result<NodeId, ParseError> {
        self.context
            .id_matcher
            .find_nondata_node_id(node_str, None)
            .ok_or_else(|| {
                vec![(
                    format!("{label} node with ID {node_str} was not found. Have you defined it?"),
                    tc,
                )]
            })
    }

    fn find_pool_id_or_error(&self, pool_str: &str) -> Result<PoolId, ParseError> {
        self.context
            .id_matcher
            .find_pool_id(pool_str)
            .ok_or_else(|| {
                vec![(
                    format!("Pool with ID ({pool_str}) was not found. Have you defined it?"),
                    self.context.current_token_coordinate,
                )]
            })
    }

    fn find_pool_id_or_error_restricted(
        &self,
        pool_str: &str,
        forbidden_value: PoolId,
        forbidden_tc: TokenCoordinate,
        tee_or_mpc: &str,
        attribute: &str,
    ) -> Result<PoolId, ParseError> {
        let result = self.find_pool_id_or_error(pool_str);
        if let Ok(found_pool_id) = result
            && found_pool_id == forbidden_value
        {
            Err(vec![
                (
                    format!(
                        "The pool already represents the {tee_or_mpc}. It cannot represent the operator at the same time. Change the pool id in {tee_or_mpc}-{attribute}."
                    ),
                    forbidden_tc,
                ),
                (
                    "This is the matched pool".to_string(),
                    self.graph.pools[found_pool_id].tc,
                ),
            ])
        } else {
            result
        }
    }

    fn smthng_missing_error(
        &self,
        tee_or_mpc: &str,
        smthng: &str,
        optional_text: &str,
    ) -> ParseError {
        vec![(
            format!("{tee_or_mpc}-{smthng} is missing. Please define it.") + optional_text,
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
                self.context.id_matcher.find_data_node_id(data_str, None)
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

    fn verify_pebpmd_pool(
        &mut self,
        common: &ComputationCommon,
        tee_or_mpc: &str,
    ) -> Result<(), ParseError> {
        let &PeBpmdSubType::Pool(pool_id) = &common.pebpmd_type else {
            // Guaranteed by the caller.
            unreachable!();
        };

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

    fn verify_pebpmd_lane(
        &mut self,
        common: &ComputationCommon,
        tee_or_mpc: &str,
    ) -> Result<(), ParseError> {
        let &PeBpmdSubType::Lane { pool_id, lane_id } = &common.pebpmd_type else {
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

    fn verify_pebpmd_tasks(
        &mut self,
        common: &ComputationCommon,
        tee_or_mpc: &str,
    ) -> Result<(), ParseError> {
        let PeBpmdSubType::Tasks(tasks) = &common.pebpmd_type else {
            // Guaranteed by the caller.
            unreachable!();
        };
        for (task, _) in tasks {
            if let NodeType::RealNode {
                pe_bpmd_hides_protection_operations,
                ..
            } = &mut self.graph.nodes[*task].node_type
            {
                *pe_bpmd_hides_protection_operations = true;
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

    fn find_and_verify_hw_sw_operators(
        &self,
        tee_or_mpc: &str,
        operators: &[(String, TokenCoordinate)],
        // None for tee-pool/mpc-pool.
        this_pool_id_and_lane_or_tasks: Option<(PoolId, /* lane or tasks */ &str)>,
        software_or_hardware: &str,
    ) -> Result<Vec<PoolId>, ParseError> {
        let all_tcs = match operators {
            [(_, first_tc @ last_tc)] | [(_, first_tc), .., (_, last_tc)] => TokenCoordinate {
                end: last_tc.end,
                ..*first_tc
            },

            _ => {
                if let Some((this_pool_id, _)) = this_pool_id_and_lane_or_tasks {
                    if tee_or_mpc == "tee" {
                        return Ok(vec![this_pool_id]);
                    } else {
                        return Ok(Vec::new());
                    }
                } else {
                    return Err(vec![(
                        format!(
                            "You must have a list of pool references for `({tee_or_mpc}-{software_or_hardware}-operators @pool-1 @optional-pool-2)`."
                        ),
                        self.context.current_token_coordinate,
                    )]);
                }
            }
        };
        let mut result = Vec::new();
        let mut result_tc = Vec::new();
        let mut found_this_pool = this_pool_id_and_lane_or_tasks.is_none();
        for (operator, tc) in operators {
            let pool_id = self.find_pool_id_or_error(operator)?;
            if let Some(idx) = result.iter().position(|prev| *prev == pool_id) {
                return Err(vec![
                    (
                        format!(
                            "Each pool can only be specified once in `({tee_or_mpc}-{software_or_hardware}-operators ...)`, but this one was specified twice"
                        ),
                        *tc,
                    ),
                    (
                        "Previous reference to the same pool was here".to_string(),
                        result_tc[idx],
                    ),
                    (
                        "This is the pool which both target".to_string(),
                        self.graph.pools[pool_id].tc,
                    ),
                ]);
            }
            if let Some((this_pool_id, _)) = this_pool_id_and_lane_or_tasks
                && this_pool_id == pool_id
            {
                found_this_pool = true;
            }
            result.push(pool_id);
            result_tc.push(*tc);
        }

        if !found_this_pool {
            let (_, lane_or_tasks) = this_pool_id_and_lane_or_tasks.unwrap();
            let mut errors = vec![(
                format!(
                    "The containing pool must always be part of the `({tee_or_mpc}-{software_or_hardware}-operators ..)` list of {tee_or_mpc}-{lane_or_tasks}, but it is currently missing. Note: If the containing pool would be the only operator, then you can use the magic keyword `{CONTAINING_POOL_ONLY_KEYWORD}`"
                ),
                all_tcs,
            )];
            for matched_pool_id in result.iter().cloned() {
                errors.push((
                    "This pool was referenced (but is not the containing pool)".to_string(),
                    self.graph.pools[matched_pool_id].tc,
                ));
            }
            return Err(errors);
        }

        Ok(result)
    }
}

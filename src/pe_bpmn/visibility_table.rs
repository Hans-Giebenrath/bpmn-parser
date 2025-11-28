use crate::common::node::Node;
use crate::pe_bpmn::parser::{
    ComputationCommon, Mpc, PeBpmnSubType, PeBpmnType, SecureChannel, Tee,
};
use itertools::chain;
use proc_macros::{from, n, to};
use std::collections::HashSet;

use crate::{
    common::{
        edge::{Edge, FlowType},
        graph::{Graph, NodeId, PoolId, SdeId},
    },
    lexer::PeBpmnProtection,
    pe_bpmn::parser::PeBpmn,
};
use std::collections::HashMap;

enum PoolProtection {
    None,
    Visible,
    Protected(Vec<Vec<PeBpmnProtection>>),
}

pub fn generate_visibility_table(graph: &Graph) -> Result<String, Box<dyn std::error::Error>> {
    let mut csv = csv::Writer::from_writer(Vec::new());

    // Write header
    csv.write_record(chain(
        std::iter::once("Pool"),
        graph.data_elements.iter().map(|sde| sde.name.as_str()),
    ))?;

    // Write rows
    for (pool_idx, pool) in graph.pools.iter().enumerate() {
        let pool_id = PoolId(pool_idx);
        csv.write_record(
            std::iter::once(
                pool.name
                    .clone()
                    .unwrap_or_else(|| "Anonymous Pool".to_string()),
            )
            .chain(
                graph
                    .data_elements
                    .iter()
                    .enumerate()
                    .map(|(sde_idx, sde)| {
                        let sde_id = SdeId(sde_idx);
                        let protection =
                            analyze_pool_protection_nested(&sde.data_element, graph, pool_id);

                        match protection {
                            PoolProtection::None => {
                                let root_access = contains(
                                    &graph.pools[pool_id].tee_external_root_access,
                                    &sde_id,
                                );

                                let admin_accessible = contains(
                                    &graph.pools[pool_id].tee_admin_has_pe_bpmn_visibility_A_for,
                                    &sde_id,
                                );

                                let admin_hidden = contains(
                                    &graph.pools[pool_id].tee_admin_has_pe_bpmn_visibility_H_for,
                                    &sde_id,
                                );

                                // TODO this is inaccurate. It should check what is the
                                // minimum protection level of the data in the TEE/MPC and then
                                // copy that + add an `A`. So just `A` is only correct when the
                                // TEE actually has Visible access to the given data. But it
                                // might just forward some further protected data.
                                if admin_accessible || root_access {
                                    "A".to_string()
                                } else if admin_hidden {
                                    "H".to_string()
                                } else {
                                    String::new()
                                }
                            }
                            PoolProtection::Visible => "V".to_string(),
                            PoolProtection::Protected(protections) => {
                                let mut results = Vec::<String>::new();
                                for mut protections in protections {
                                    let previous_len = protections.len();
                                    protections.retain(|protection| {
                                        !graph.pools[pool_id]
                                            .tee_external_root_access
                                            .contains(&(sde_id, *protection))
                                            && !graph.pools[pool_id]
                                                .tee_admin_has_pe_bpmn_visibility_A_for
                                                .contains(&(sde_id, *protection))
                                    });
                                    let mut result = "H".repeat(protections.len());
                                    if previous_len != protections.len() {
                                        result.push('A');
                                    }
                                    results.push(result);
                                }
                                results.sort();
                                results.dedup();
                                results.join("/")
                            }
                        }
                    }),
            ),
        )?;
    }

    // Network row
    let mut network_map: HashMap<SdeId, bool> = HashMap::new();
    for edge in graph.edges.iter().filter(|e| Edge::is_message_flow(e)) {
        if let FlowType::MessageFlow(aux) = &edge.flow_type {
            for sde_id in &aux.transported_data {
                let is_protected = aux
                    .pebpmn_protection
                    .iter()
                    .find(|(id, _)| id == sde_id)
                    .map(|(_, protections)| !protections.is_empty())
                    .unwrap_or(false);

                network_map
                    .entry(*sde_id)
                    .and_modify(|existing| *existing &= is_protected)
                    .or_insert(is_protected);
            }
        }
    }

    // Write network row
    let mut row = vec!["Network".to_string()];
    for sde_id in 0..graph.data_elements.len() {
        match network_map.get(&SdeId(sde_id)) {
            Some(true) => row.push("H".to_string()),
            Some(false) => row.push("V".to_string()),
            None => row.push(" ".to_string()),
        }
    }
    csv.write_record(row)?;

    let bytes = csv.into_inner()?;
    Ok(String::from_utf8(bytes).unwrap_or("".to_string()))
}

fn is_pebpmn_node(graph: &Graph, node: &Node) -> bool {
    // match on all the pebpmn elements and check if it falls in there.
    for pebpmn in &graph.pe_bpmn_definitions {
        let subtype = match &pebpmn.r#type {
            PeBpmnType::Tee(Tee {
                common: ComputationCommon { pebpmn_type, .. },
            })
            | PeBpmnType::Mpc(Mpc {
                common: ComputationCommon { pebpmn_type, .. },
            }) => pebpmn_type,
            _ => continue,
        };
        match subtype {
            PeBpmnSubType::Pool(pool_id) if node.pool == *pool_id => return true,
            PeBpmnSubType::Lane { pool_id, lane_id }
                if node.pool == *pool_id && node.lane == *lane_id =>
            {
                return true;
            }
            PeBpmnSubType::Tasks(tasks) if tasks.iter().any(|&(node_id, _)| node_id == node.id) => {
                return true;
            }
            _ => continue,
        }
    }
    false
}

/// Imagine a data-element is in a pool twice, (1) once in a TEE+Secure Channel protection (meaning
/// `HH`), and (2) a second time as a thrice pseudonymised form (meaning `PPP`[*]). It is up to the
/// reader to interpret whether `HH` or `PPP` is better.
/// [*]: Allowing custom letters per pe-bpmn protection is not yet implemented, but the idea is that
/// one can say that a secure channel uses `P` to misuse it for pseudonymization (or anonymization).
fn analyze_pool_protection_nested(sde_id: SdeId, graph: &Graph, pool_id: PoolId) -> PoolProtection {
    let mut found = false;
    let mut protection_kinds = Vec::new();

    for node in &graph.nodes {
        if is_pebpmn_node(graph, node) || node.pool != pool_id {
            continue;
        }
        // Data nodes can be placed funkily. If a TEE lane shares data with the non-TEE lane then I
        // don't know where the respective data node is drawn. So ignore it and instead look at the
        // real nodes' incoming and outgoing data edges. (Not node_transported_data because it might
        // be created in a node and then directly shared with the TEE lane, so need to really
        // inspect the edges.)
        if node.is_data() {
            continue;
        }

        for edge_id in &node.incoming {
            let from = &from!(*edge_id);
            if let Some(aux) = from.get_data_aux()
                && aux.sde_id == sde_id
            {
                found = true;
                if aux.pebpmn_protection.is_empty() {
                    return PoolProtection::Visible;
                }

                incorporate(&mut protection_kinds, &aux.pebpmn_protection);
            }
        }

        for edge_id in &node.outgoing {
            let to = &to!(*edge_id);
            if let Some(aux) = to.get_data_aux()
                && aux.sde_id == sde_id
            {
                found = true;
                if aux.pebpmn_protection.is_empty() {
                    return PoolProtection::Visible;
                }

                incorporate(&mut protection_kinds, &aux.pebpmn_protection);
            }
        }
    }

    if !found {
        PoolProtection::None
    } else {
        PoolProtection::Protected(protection_kinds)
    }
}

fn incorporate(existing: &mut Vec<Vec<PeBpmnProtection>>, new: &[PeBpmnProtection]) {
    let mut was_replaced = false;
    for existing in existing.iter_mut() {
        if existing.iter().all(|p| new.contains(p)) {
            // `existing` is a subset of `new`, so adding `new` would be redundant.
            return;
        }
        if new.iter().all(|p| existing.contains(p)) {
            existing.clear();
            existing.reserve(new.len());
            existing.extend_from_slice(new);
            was_replaced = true;
        }
    }

    if was_replaced {
        existing.sort();
        existing.dedup();
    } else {
        existing.push(new.to_vec());
        existing.sort();
    }
}

fn contains(haystack: &HashSet<(SdeId, PeBpmnProtection)>, needle: &SdeId) -> bool {
    haystack.iter().any(|(straw, _)| *straw == *needle)
}

fn find(haystack: &HashSet<(SdeId, PeBpmnProtection)>, needle: SdeId) -> Option<PeBpmnProtection> {
    for &(key, val) in haystack {
        if key == needle {
            return Some(val);
        }
    }

    None
}

fn is_pool_pebpmn(pebpmn: &PeBpmn, pool_id: PoolId) -> bool {
    match &pebpmn.r#type {
        PeBpmnType::Mpc(Mpc {
            common:
                ComputationCommon {
                    pebpmn_type: PeBpmnSubType::Pool(pool),
                    ..
                },
        })
        | PeBpmnType::Tee(Tee {
            common:
                ComputationCommon {
                    pebpmn_type: PeBpmnSubType::Pool(pool),
                    ..
                },
        }) => *pool == pool_id,
        _ => false,
    }
}

struct OnDemandVisibilityTableCell {
    cache_pool: HashMap<(PoolId, SdeId), Vec<String>>,
    cache_pebpmn: HashMap<(PeBpmnProtection, SdeId), Vec<String>>,
    endless_recursion_detection_pool: Vec<(PoolId, SdeId)>,
    endless_recursion_detection_pebpmn: Vec<(PeBpmnProtection, SdeId)>,
}

impl OnDemandVisibilityTableCell {
    fn get_pool(
        &mut self,
        graph: &Graph,
        pool: PoolId,
        sde_id: SdeId,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        if let Some(protection_string) = self.cache_pool.get(&(pool, sde_id)) {
            Ok(protection_string.clone())
        } else if self
            .endless_recursion_detection_pool
            .contains(&(pool, sde_id))
        {
            return Err(format!(
                "endless recursion: {:?}, {:?}",
                self.endless_recursion_detection_pool, self.endless_recursion_detection_pebpmn
            )
            .into());
        } else if let Some(pebpmn) = graph
            .pe_bpmn_definitions
            .iter()
            .find(|pebpmn| is_pool_pebpmn(pebpmn, pool))
        {
            return self.get_pebpmn(graph, pebpmn.r#type.protection(), sde_id);
        } else {
            self.endless_recursion_detection_pool.push((pool, sde_id));
            let protection_string = self.calculate_for_pool(graph, pool, sde_id)?;
            self.cache_pool
                .insert((pool, sde_id), protection_string.clone());
            Ok(protection_string)
        }
    }

    fn get_pebpmn(
        &mut self,
        graph: &Graph,
        protection: PeBpmnProtection,
        sde_id: SdeId,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        if let Some(protection_string) = self.cache_pebpmn.get(&(protection, sde_id)) {
            Ok(protection_string.clone())
        } else if self
            .endless_recursion_detection_pebpmn
            .contains(&(protection, sde_id))
        {
            return Err(format!(
                "endless recursion: {:?}, {:?}",
                self.endless_recursion_detection_pool, self.endless_recursion_detection_pebpmn
            )
            .into());
        } else {
            self.endless_recursion_detection_pebpmn
                .push((protection, sde_id));
            let protection_string = self.calculate_for_pebpmn(graph, protection, sde_id)?;
            self.cache_pebpmn
                .insert((protection, sde_id), protection_string.clone());
            Ok(protection_string)
        }
    }

    fn calculate_for_pool(
        &mut self,
        graph: &Graph,
        pool_id: PoolId,
        sde_id: SdeId,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let protection = analyze_pool_protection_nested(sde_id, graph, pool_id);

        let mut protection_strings = match protection {
            PoolProtection::Visible => return Ok(vec!["V".to_string()]),
            PoolProtection::None => vec![ /* empty, but not `String::new()` !!! */ ],
            PoolProtection::Protected(protections) => {
                let mut results = Vec::<String>::new();
                for mut protections in protections {
                    let previous_len = protections.len();
                    protections.retain(|protection| {
                        !graph.pools[pool_id]
                            .tee_external_root_access
                            .contains(&(sde_id, *protection))
                            && !graph.pools[pool_id]
                                .tee_admin_has_pe_bpmn_visibility_A_for
                                .contains(&(sde_id, *protection))
                    });
                    let mut result = "H".repeat(protections.len());
                    if previous_len != protections.len() {
                        result.push('A');
                    }
                    results.push(result);
                }
                results
            }
        };
        let root_access = find(&graph.pools[pool_id].tee_external_root_access, sde_id);

        let admin_accessible = find(
            &graph.pools[pool_id].tee_admin_has_pe_bpmn_visibility_A_for,
            sde_id,
        );

        let admin_hidden = find(
            &graph.pools[pool_id].tee_admin_has_pe_bpmn_visibility_H_for,
            sde_id,
        );

        // TODO this is inaccurate. It should check what is the
        // minimum protection level of the data in the TEE/MPC and then
        // copy that + add an `A`. So just `A` is only correct when the
        // TEE actually has Visible access to the given data. But it
        // might just forward some further protected data.
        if let Some(protection) = admin_accessible.or(root_access) {
            // Whetever we can reach through our admin or root access, add an 'A'.
            // This could lead to multiple 'A's in a row, but that is fine - it becomes a
            // bit harder. Can be collapsed manually by users in a follow-up step.
            protection_strings.extend(self.get_pebpmn(graph, protection, sde_id)?.into_iter().map(
                |mut s| {
                    if s == "V" {
                        s.clear();
                    }
                    s.push('A');
                    s
                },
            ));
        } else if let Some(protection) = admin_hidden {
            protection_strings.extend(self.get_pebpmn(graph, protection, sde_id)?.into_iter().map(
                |mut s| {
                    if s == "V" {
                        s.clear();
                    }
                    s.insert(0, 'H');
                    s
                },
            ));
        };
        protection_strings.sort();
        protection_strings.dedup();
        Ok(protection_strings)
    }

    fn calculate_for_pebpmn(
        &mut self,
        graph: &Graph,
        protection: PeBpmnProtection,
        sde_id: SdeId,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let pebpmn = graph
            .pe_bpmn_definitions
            .iter()
            .find(|pebpmn| pebpmn.r#type.protection() == protection)
            .expect("should exist");
        todo!()
    }
}

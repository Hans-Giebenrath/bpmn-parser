use itertools::chain;
use proc_macros::n;
use std::collections::HashSet;

use crate::{
    common::{
        edge::{Edge, FlowType},
        graph::{Graph, NodeId, PoolId, SdeId},
    },
    lexer::PeBpmnProtection,
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

/// Imagine a data-element is in a pool twice, (1) once in a TEE+Secure Channel protection (meaning
/// `HH`), and (2) a second time as a thrice pseudonymised form (meaning `PPP`). It is up to the
/// reader to interpret whether `HH` or `PPP` is better.
fn analyze_pool_protection_nested(
    data_elements: &[NodeId],
    graph: &Graph,
    pool_id: PoolId,
) -> PoolProtection {
    let mut found = false;
    let mut visible = false;
    let mut protection_kinds = Vec::new();

    for node_id in data_elements.iter().cloned() {
        let data_node = &n!(node_id);
        if data_node.pool != pool_id {
            continue;
        }

        found = true;
        let Some(aux) = data_node.get_data_aux() else {
            unreachable!("Must be a data node.");
        };

        if aux.pebpmn_protection.is_empty() {
            visible = true;
            break;
        }

        incorporate(&mut protection_kinds, &aux.pebpmn_protection);
    }

    if !found {
        PoolProtection::None
    } else if visible {
        PoolProtection::Visible
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

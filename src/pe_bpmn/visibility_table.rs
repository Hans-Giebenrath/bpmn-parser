use itertools::chain;

use crate::common::{
    edge::{Edge, FlowType},
    graph::{Graph, NodeId, PoolId, SdeId},
};
use std::collections::HashMap;

enum PoolProtection {
    None,
    Visible,
    Protected { min_depth: usize },
}

pub fn generate_visibility_table(graph: &Graph) -> Result<String, Box<dyn std::error::Error>> {
    let mut csv = csv::Writer::from_writer(Vec::new());

    // Write header
    csv.write_record(chain(
        std::iter::once("Pool"),
        graph.data_elements.iter().map(|sde| sde.name.as_str()),
    ))?;

    // Write rows
    for (pool_index, pool) in graph.pools.iter().enumerate() {
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
                    .map(|(sde_index, sde)| {
                        let protection =
                            analyze_pool_protection_nested(&sde.data_element, graph, pool_index);

                        let root_access = graph.pools[PoolId(pool_index)]
                            .tee_external_root_access
                            .contains(&SdeId(sde_index));

                        let admin_accessible = graph.pools[pool_index]
                            .tee_admin_has_pe_bpmn_visibility_A_for
                            .contains(&SdeId(sde_index));

                        let admin_hidden = graph.pools[pool_index]
                            .tee_admin_has_pe_bpmn_visibility_H_for
                            .contains(&SdeId(sde_index));

                        match protection {
                            PoolProtection::None => {
                                if admin_accessible || root_access {
                                    "A".to_string()
                                } else if admin_hidden {
                                    "H".to_string()
                                } else {
                                    " ".to_string()
                                }
                            }
                            PoolProtection::Visible => "V".to_string(),
                            PoolProtection::Protected { min_depth } => {
                                let mut result = "H".repeat(min_depth);
                                if admin_accessible || root_access {
                                    result.pop();
                                    result.push('A');
                                }
                                result
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

fn analyze_pool_protection_nested(
    data_elements: &[NodeId],
    graph: &Graph,
    pool_index: usize,
) -> PoolProtection {
    let mut found = false;
    let mut visible = false;
    let mut min_depth = usize::MAX;

    for id in data_elements {
        if graph.nodes[*id].pool != PoolId(pool_index) {
            continue;
        }

        found = true;
        let aux = match graph.nodes[*id].get_data_aux() {
            Some(aux) => aux,
            None => continue,
        };

        if aux.pebpmn_protection.is_empty() {
            visible = true;
            break;
        }

        min_depth = min_depth.min(aux.pebpmn_protection.len());
    }

    if !found {
        PoolProtection::None
    } else if visible {
        PoolProtection::Visible
    } else {
        PoolProtection::Protected { min_depth }
    }
}

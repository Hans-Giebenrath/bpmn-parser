use crate::common::node::Node;
use crate::pe_bpmn::parser::{ComputationCommon, Mpc, PeBpmnSubType, PeBpmnType, Tee};
use itertools::chain;
use proc_macros::{from, to};
use std::collections::HashSet;

use crate::{
    common::{
        edge::{Edge, FlowType},
        graph::{Graph, PoolId, SdeId},
    },
    lexer::PeBpmnProtection,
    pe_bpmn::parser::PeBpmn,
};
use std::collections::HashMap;

// Technically this whole enum could be represented as just `Vec<Vec<PeBpmnProtection>>` with
// `None` being `[]` and visible being `[[]]`. But this is a bit hard to read, so add a bit more
// verbosity to it.
enum PoolProtection {
    None,
    Visible,
    Protected(Vec<Vec<PeBpmnProtection>>),
}

pub fn generate_visibility_table(graph: &Graph) -> Result<String, Box<dyn std::error::Error>> {
    let mut csv = csv::Writer::from_writer(Vec::new());
    let mut on_demand_call = OnDemandVisibilityTableCell::default();

    // Write header
    csv.write_record(chain(
        std::iter::once("Pool"),
        graph.data_elements.iter().map(|sde| sde.name.as_str()),
    ))?;

    // Write rows
    for (pool_idx, pool) in graph.pools.iter().enumerate() {
        let pool_id = PoolId(pool_idx);
        // The `Result` is there primarily because there can be a cycle of TEE admins being each
        // others TEE admins. TODO In general this is not a problem at all, but just needs to be
        // implemented, but I don't do that now. But then the `collect` can be removed, avoid a
        // bunch of allocations, yay! (who cares! I care!)
        let row = graph
            .data_elements
            .iter()
            .enumerate()
            .map(|(sde_idx, _)| {
                on_demand_call
                    .get_pool(graph, pool_id, SdeId(sde_idx))
                    .map(|strings| strings.join("/"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        csv.write_record(
            std::iter::once(
                pool.name
                    .clone()
                    .unwrap_or_else(|| "Anonymous Pool".to_string()),
            )
            .chain(row.into_iter()),
        )?;
    }

    // Network row
    // At the moment the network cannot have an A since we don't model encryption keys explicitly.
    // But with the secure channel with explicit encryption key this can change.
    let mut network_map: HashMap<SdeId, usize> = HashMap::new();
    for edge in graph.edges.iter().filter(|e| Edge::is_message_flow(e)) {
        if let FlowType::MessageFlow(aux) = &edge.flow_type {
            for sde_id in &aux.transported_data {
                let protection_count = dbg!(aux)
                    .pebpmn_protection
                    .iter()
                    .find(|(id, _)| id == sde_id)
                    .map(|(_, protections)| protections.len())
                    .unwrap_or(0);

                network_map
                    .entry(*sde_id)
                    .and_modify(|existing| *existing = (*existing).min(protection_count))
                    .or_insert(protection_count);
            }
        }
    }

    // Write network row
    let mut row = vec!["Network".to_string()];
    for sde_id in 0..graph.data_elements.len() {
        match network_map.get(&SdeId(sde_id)) {
            Some(0) => row.push("V".to_string()),
            Some(n) => row.push(std::iter::repeat_n('H', *n).collect()),
            None => row.push(String::new()),
        }
    }
    csv.write_record(row)?;

    let bytes = csv.into_inner()?;
    Ok(String::from_utf8(bytes).unwrap_or("".to_string()))
}

fn is_pebpmn_node_of(node: &Node, subtype: &PeBpmnSubType) -> bool {
    match subtype {
        &PeBpmnSubType::Pool(pool_id) if node.pool == pool_id => true,
        &PeBpmnSubType::Lane { pool_id, lane_id }
            if node.pool == pool_id && node.lane == lane_id =>
        {
            true
        }
        PeBpmnSubType::Tasks(tasks) if tasks.iter().any(|&(node_id, _)| node_id == node.id) => true,
        _ => false,
    }
}

fn is_pebpmn_node(graph: &Graph, node: &Node) -> bool {
    // match on all the pebpmn elements and check if it falls in there.
    for pebpmn in &graph.pe_bpmn_definitions {
        match &pebpmn.r#type {
            PeBpmnType::Tee(Tee {
                common: ComputationCommon { pebpmn_type, .. },
            })
            | PeBpmnType::Mpc(Mpc {
                common: ComputationCommon { pebpmn_type, .. },
            }) if is_pebpmn_node_of(node, pebpmn_type) => return true,
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
fn analyze_pool_protection(sde_id: SdeId, graph: &Graph, pool_id: PoolId) -> PoolProtection {
    let mut protection_kinds = Vec::new();

    for node in &graph.nodes {
        if node.pool != pool_id || is_pebpmn_node(graph, node) {
            continue;
        }

        let is_visible = analyze_pool_protection_nested(graph, sde_id, node, &mut protection_kinds);
        if is_visible {
            return PoolProtection::Visible;
        }
    }

    if protection_kinds.is_empty() {
        PoolProtection::None
    } else {
        PoolProtection::Protected(protection_kinds)
    }
}

fn analyze_pebpmn_protection(
    sde_id: SdeId,
    graph: &Graph,
    protection: PeBpmnProtection,
) -> PoolProtection {
    let pebpmn = graph
        .pe_bpmn_definitions
        .iter()
        .find(|pebpmn| pebpmn.r#type.protection() == protection)
        .expect("should exist");
    let common = match &pebpmn.r#type {
        PeBpmnType::SecureChannel(..) => {
            unreachable!("secure channels should not be analyzed for protection")
        }
        PeBpmnType::Tee(Tee { common }) | PeBpmnType::Mpc(Mpc { common }) => common,
    };

    let mut protection_kinds = Vec::new();
    // Yes, this iteration is less than optimal, since we do an O(SdeId * N) iteration. But
    // both are small and we have superb hardware, don't we? 2025+? The real reason is that
    // I don't really managed to directly iterate the correct nodes without duplicating
    // code. So this is, again, more optimized for readability. If profiling shows this is a
    // problem, we can change it. Or if you are monetizing this product by providing a SaaS
    // and want to optimize it to reduce hosting costs by 1%, you can manage to improve it
    // :)
    for node in &graph.nodes {
        if is_pebpmn_node_of(node, &common.pebpmn_type) {
            let is_visible =
                analyze_pool_protection_nested(graph, sde_id, node, &mut protection_kinds);
            if is_visible {
                return PoolProtection::Visible;

                //
            }
        }
    }

    if protection_kinds.is_empty() {
        PoolProtection::None
    } else {
        PoolProtection::Protected(protection_kinds)
    }
}

fn analyze_pool_protection_nested(
    graph: &Graph,
    sde_id: SdeId,
    node: &Node,
    protection_kinds: &mut Vec<Vec<PeBpmnProtection>>,
) -> bool {
    // Data nodes can be placed funkily. If a TEE lane shares data with the non-TEE lane then I
    // don't know where the respective data node is drawn. So ignore it and instead look at the
    // real nodes' incoming and outgoing data edges. (Not node_transported_data because it might
    // be created in a node and then directly shared with the TEE lane, so need to really
    // inspect the edges.)
    if node.is_data() {
        return false;
    }

    for edge_id in &node.incoming {
        let from = &from!(*edge_id);
        if let Some(aux) = from.get_data_aux()
            && aux.sde_id == sde_id
        {
            if aux.pebpmn_protection.is_empty() {
                return true;
            }

            incorporate(protection_kinds, &aux.pebpmn_protection);
        }
    }

    for edge_id in &node.outgoing {
        let to = &to!(*edge_id);
        if let Some(aux) = to.get_data_aux()
            && aux.sde_id == sde_id
        {
            if aux.pebpmn_protection.is_empty() {
                return true;
            }

            incorporate(protection_kinds, &aux.pebpmn_protection);
        }
    }

    false
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

/// Due to the external root access and admin for TEEs, one needs to query the visibility cell of
/// other pools / pebpmns. It could be that the other cell is not computed yet, because the admin
/// pool comes before the TEE pool. Also, for TEE tasks/lanes one needs to compute it as well and
/// they are not pools. And further, if TEE `A` is admin of TEE `B`, then admin of TEE `A` becomes
/// transitively the admin of TEE `B`. So this is an on-demand data structure where cells are
/// calculated lazily and then cached for further usage.
#[derive(Default)]
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
            Err(format!(
                "endless recursion: {:?}, {:?}",
                self.endless_recursion_detection_pool, self.endless_recursion_detection_pebpmn
            )
            .into())
        } else if let Some(pebpmn) = graph
            .pe_bpmn_definitions
            .iter()
            .find(|pebpmn| is_pool_pebpmn(pebpmn, pool))
        {
            self.get_pebpmn(graph, pebpmn.r#type.protection(), sde_id)
        } else {
            self.endless_recursion_detection_pool.push((pool, sde_id));
            let protection_string = self.calculate_for_pool(graph, pool, sde_id)?;
            self.cache_pool
                .insert((pool, sde_id), protection_string.clone());
            self.endless_recursion_detection_pool.pop();
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
            Err(format!(
                "endless recursion: {:?}, {:?}",
                self.endless_recursion_detection_pool, self.endless_recursion_detection_pebpmn
            )
            .into())
        } else {
            self.endless_recursion_detection_pebpmn
                .push((protection, sde_id));
            let protection_string = self.calculate_for_pebpmn(graph, protection, sde_id)?;
            self.cache_pebpmn
                .insert((protection, sde_id), protection_string.clone());
            self.endless_recursion_detection_pebpmn.pop();
            Ok(protection_string)
        }
    }

    fn calculate_for_pool(
        &mut self,
        graph: &Graph,
        pool_id: PoolId,
        sde_id: SdeId,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        self.protection_to_strings(
            graph,
            sde_id,
            analyze_pool_protection(sde_id, graph, pool_id),
            StringifyModification::PoolId(pool_id),
        )
    }

    fn calculate_for_pebpmn(
        &mut self,
        graph: &Graph,
        pebpmn_protection: PeBpmnProtection,
        sde_id: SdeId,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let pebpmn = graph
            .pe_bpmn_definitions
            .iter()
            .find(|pebpmn| pebpmn.r#type.protection() == pebpmn_protection)
            .expect("should exist");
        let unprotect_operations_are_hidden = 'block: {
            let subtype = match &pebpmn.r#type {
                PeBpmnType::Tee(Tee { common }) | PeBpmnType::Mpc(Mpc { common }) => {
                    &common.pebpmn_type
                }
                _ => break 'block false,
            };
            matches!(subtype, PeBpmnSubType::Tasks(..))
        };
        self.protection_to_strings(
            graph,
            sde_id,
            analyze_pebpmn_protection(sde_id, graph, pebpmn_protection),
            StringifyModification::PeBpmn {
                pebpmn_protection,
                unprotect_operations_are_hidden,
            },
        )
    }

    fn protection_to_strings(
        &mut self,
        graph: &Graph,
        sde_id: SdeId,
        protection: PoolProtection,
        stringify_modification: StringifyModification,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut protection_strings = match protection {
            PoolProtection::Visible => return Ok(vec!["V".to_string()]),
            PoolProtection::None => vec![ /* empty, but not `String::new()` !!! */ ],
            PoolProtection::Protected(protections) => {
                stringify_modification.stringify(graph, sde_id, protections)
            }
        };
        let pool_id = stringify_modification.pool_id();
        let root_access = pool_id
            .and_then(|pool_id| find(&graph.pools[pool_id].tee_external_root_access, sde_id));

        let admin_accessible = pool_id.and_then(|pool_id| {
            find(
                &graph.pools[pool_id].tee_admin_has_pe_bpmn_visibility_A_for,
                sde_id,
            )
        });

        let admin_hidden = pool_id.and_then(|pool_id| {
            find(
                &graph.pools[pool_id].tee_admin_has_pe_bpmn_visibility_H_for,
                sde_id,
            )
        });

        if let Some(protection) = admin_accessible.or(root_access) {
            // Whatever we can reach through our admin or root access, add an `A`. This may result
            // in an `AA` which is fine. I think `AA` is a tidbit more complicated for an attacker
            // than `A` I would say. Can be collapsed manually by users in a follow-up step if they
            // think this is useless.
            protection_strings.extend(self.get_pebpmn(graph, protection, sde_id)?.into_iter().map(
                |mut s| {
                    if s == "V" {
                        // If this was plaintext data to the TEE, then for us it is just `A`.
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
                        // If this was plaintext data to the TEE, then for us it is just `H`.
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
}

enum StringifyModification {
    PoolId(PoolId),
    PeBpmn {
        pebpmn_protection: PeBpmnProtection,
        unprotect_operations_are_hidden: bool,
    },
}

impl StringifyModification {
    fn stringify(
        &self,
        graph: &Graph,
        sde_id: SdeId,
        protections: Vec<Vec<PeBpmnProtection>>,
    ) -> Vec<String> {
        let mut results = Vec::<String>::new();
        for mut protections in protections {
            let previous_len = protections.len();
            match *self {
                StringifyModification::PoolId(pool_id) => {
                    protections.retain(|protection| {
                        // Only check visibility A here. For root access the data needs to actually
                        // reach the TEE (so we get access to it in the code below when used
                        // `get_pebpmn`. But visibility A has consequences for the data that did not
                        // yet reach the TEE, the admin could still decrypt it.
                        !graph.pools[pool_id]
                            .tee_admin_has_pe_bpmn_visibility_A_for
                            .contains(&(sde_id, *protection))
                    });
                    let mut result = "H".repeat(protections.len());
                    if previous_len != protections.len() {
                        result.push('A');
                    }
                    results.push(result);
                }
                StringifyModification::PeBpmn {
                    pebpmn_protection,
                    unprotect_operations_are_hidden,
                } => {
                    protections.retain(|protection| *protection != pebpmn_protection);
                    let mut result = "H".repeat(protections.len());
                    // For tee-tasks we just assume that it unprotects its own protection. But
                    // for a full fledged lane/pool we have the actions present - if it simply does
                    // not decrypt it, then we at least have an `A`. Otherwise, the protection would
                    // already have been stripped (as a node without that protection would have been
                    // found).
                    if previous_len != protections.len() && !unprotect_operations_are_hidden {
                        result.push('A');
                    }
                    results.push(result);
                }
            }
        }
        results
    }
    fn pool_id(&self) -> Option<PoolId> {
        match self {
            &StringifyModification::PoolId(pool_id) => Some(pool_id),
            _ => None,
        }
    }
}

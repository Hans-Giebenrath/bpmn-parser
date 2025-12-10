use crate::pe_bpmn::PoolOrProtection;
use crate::pe_bpmn::VisibilityTableInput;
use crate::pe_bpmn::parser::{ComputationCommon, Mpc, PeBpmnSubType, PeBpmnType, Tee};
use itertools::chain;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fmt::Display;

use crate::{
    common::graph::{Graph, PoolId, SdeId},
    lexer::PeBpmnProtection,
    pe_bpmn::parser::PeBpmn,
};
use std::collections::HashMap;

struct Args<'a> {
    graph: &'a Graph,
    input: &'a VisibilityTableInput,
}

#[derive(PartialOrd, Ord, PartialEq, Eq, Copy, Clone)]
enum ProtectionString {
    Visible,
    Protected { h_count: u32, a_count: u32 },
    Inaccessible,
}

impl ProtectionString {
    fn become_min(&mut self, other: ProtectionString) {
        if other < *self {
            *self = other;
        }
    }

    #[must_use]
    fn mark_with_h(&self) -> Self {
        match self {
            ProtectionString::Protected { h_count, a_count } => ProtectionString::Protected {
                h_count: h_count + 1,
                a_count: *a_count,
            },
            ProtectionString::Visible => ProtectionString::Protected {
                h_count: 1,
                a_count: 0,
            },
            ProtectionString::Inaccessible => ProtectionString::Inaccessible,
        }
    }

    #[must_use]
    fn mark_with_a(&self) -> Self {
        match self {
            ProtectionString::Protected { h_count, a_count } => ProtectionString::Protected {
                h_count: *h_count,
                a_count: a_count + 1,
            },
            ProtectionString::Visible => ProtectionString::Protected {
                h_count: 0,
                a_count: 1,
            },
            ProtectionString::Inaccessible => ProtectionString::Inaccessible,
        }
    }
}

impl Display for ProtectionString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtectionString::Visible => write!(f, "V"),
            ProtectionString::Protected { h_count, a_count } => {
                for _ in 0..*h_count {
                    write!(f, "H")?;
                }

                if *a_count > 0 {
                    // It actually doesn't matter much if there is one `A` or a thousand `A`. The data can
                    // be accessed.
                    write!(f, "A")?;
                }

                Ok(())
            }
            ProtectionString::Inaccessible => Ok(()),
        }
    }
}

pub fn generate_visibility_table(
    graph: &Graph,
    input: &VisibilityTableInput,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut csv = csv::Writer::from_writer(Vec::new());
    let mut on_demand_call = OnDemandVisibilityTableCell {
        cache: Default::default(),
        endless_recursion_detection: Default::default(),
    };

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
                    .get(
                        &Args { graph, input },
                        PoolOrProtection::Pool(pool_id),
                        SdeId(sde_idx),
                    )
                    .map(|strings| strings.to_string())
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

    let min_protections_len = |protection_groups: &HashSet<BTreeSet<PeBpmnProtection>>| -> usize {
        protection_groups
            .iter()
            .map(BTreeSet::len)
            .min()
            .expect("should not just be empty (it should not exist in the first place, then)")
    };

    // Write network row
    // At the moment the network cannot have an A since we don't model encryption keys explicitly.
    // But with the secure channel with explicit encryption key this can change.
    let mut row = vec!["Network".to_string()];
    for sde_id in 0..graph.data_elements.len() {
        match input
            .network_message_protections
            .get(&SdeId(sde_id))
            .map(min_protections_len)
        {
            Some(0) => row.push("V".to_string()),
            Some(n) => row.push(std::iter::repeat_n('H', n).collect()),
            None => row.push(String::new()),
        }
    }
    csv.write_record(row)?;

    let bytes = csv.into_inner()?;
    Ok(String::from_utf8(bytes).unwrap_or("".to_string()))
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
struct OnDemandVisibilityTableCell {
    cache: HashMap<(PoolOrProtection, SdeId), ProtectionString>,
    endless_recursion_detection: Vec<(PoolOrProtection, SdeId)>,
}

impl OnDemandVisibilityTableCell {
    fn get(
        &mut self,
        args: &Args<'_>,
        pool_or_protection: PoolOrProtection,
        sde_id: SdeId,
    ) -> Result<ProtectionString, Box<dyn std::error::Error>> {
        let pool_or_protection = match pool_or_protection {
            PoolOrProtection::Pool(pool_id) => args
                .graph
                .pe_bpmn_definitions
                .iter()
                .find(|pebpmn| is_pool_pebpmn(pebpmn, pool_id))
                .map(|pebpmn| PoolOrProtection::Protection(pebpmn.r#type.protection()))
                .unwrap_or(pool_or_protection),
            _ => pool_or_protection,
        };
        if self
            .endless_recursion_detection
            .contains(&(pool_or_protection, sde_id))
        {
            Err(format!("endless recursion: {:?}", self.endless_recursion_detection).into())
        } else if let Some(protection_string) = self.cache.get(&(pool_or_protection, sde_id)) {
            Ok(*protection_string)
        } else {
            self.endless_recursion_detection
                .push((pool_or_protection, sde_id));
            let protection_string = self.calculate(args, pool_or_protection, sde_id)?;
            self.cache
                .insert((pool_or_protection, sde_id), protection_string);
            self.endless_recursion_detection.pop();
            Ok(protection_string)
        }
    }

    fn calculate(
        &mut self,
        args: &Args<'_>,
        pool_or_protection: PoolOrProtection,
        sde_id: SdeId,
    ) -> Result<ProtectionString, Box<dyn std::error::Error>> {
        let mut protections_result = if let Some(inner) =
            args.input.directly_accessible_data.get(&pool_or_protection)
            && let Some(inner) = inner.get(&sde_id)
        {
            if let Some(min) = inner.iter().map(BTreeSet::len).min() {
                if min == 0 {
                    return Ok(ProtectionString::Visible);
                } else {
                    ProtectionString::Protected {
                        h_count: min as u32,
                        a_count: 0,
                    }
                }
            } else {
                dbg!("should not happen");
                // Not accessible at all.
                return Ok(ProtectionString::Inaccessible);
            }
        } else {
            // Not accessible at all.
            return Ok(ProtectionString::Inaccessible);
        };

        if let PoolOrProtection::Pool(pool_id) = pool_or_protection {
            for external_protection in args
                .input
                .tee_external_root_access
                .get(&pool_id)
                .iter()
                .flat_map(|pebpmn| pebpmn.iter())
                .map(|pebpmn| self.get(args, PoolOrProtection::Protection(*pebpmn), sde_id))
            {
                protections_result.become_min(external_protection?);
            }

            for pebpmn in args
                .graph
                .pe_bpmn_definitions
                .iter()
                .map(|pebpmn| pebpmn.r#type.protection())
            {
                if args
                    .input
                    .tee_hardware_operator
                    .contains(&(pool_id, pebpmn))
                {
                    let external_protection =
                        self.get(args, PoolOrProtection::Protection(pebpmn), sde_id)?;
                    protections_result.become_min(external_protection.mark_with_a());
                }
            }
        }

        for pebpmn in args
            .graph
            .pe_bpmn_definitions
            .iter()
            .map(|pebpmn| pebpmn.r#type.protection())
        {
            if args
                .input
                .software_operator
                .contains(&(pool_or_protection, pebpmn))
            {
                let external_protection =
                    self.get(args, PoolOrProtection::Protection(pebpmn), sde_id)?;
                protections_result.become_min(external_protection.mark_with_h());
            }
        }

        Ok(protections_result)
    }
}

use crate::common::graph::EdgeId;
use crate::common::graph::PoolId;
use crate::common::graph::SdeId;
use crate::lexer::PeBpmnProtection;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;

pub mod analysis;
pub mod parser;
pub mod visibility_table;

/// This is just a slightly different form of the PeBpmn type, but more digestible for the creation
/// of the visibility table. All necessary information is assembled in one place.
#[derive(Default)]
pub struct VisibilityTableInput {
    /// This happens if a sender uses tee-protect `no-rv` or with the `@software-operator-of-tee`,
    /// as then the software operator could replace the TEE with something they control (the remote
    /// user doesn't do correct RA, so they won't notice) and hence decrypt what they should not
    /// have seen.
    pub tee_vulnerable_rv: HashSet<(PoolId, SdeId, PeBpmnProtection)>,
    /// That PoolId gets all the data, which is part of that TEE or MPC, with an additional H.
    pub tee_software_operator: HashSet<(PoolId, PeBpmnProtection)>,
    /// That PoolId gets all the data, which is part of that TEE or MPC, with an additional A.
    pub tee_hardware_operator: HashSet<(PoolId, PeBpmnProtection)>,
    /// A pool could have root access to multiple TEEs, hence a `Vec`.
    pub tee_external_root_access: HashMap<PoolId, HashSet<PeBpmnProtection>>,
    // Contains only the `data` nodes. Does not contain data which moves via message flows, as this
    // is already captured via transitively being that TEE's software/hardware operator.
    // And the `tee_vulnerable_rv` stuff is applied directly when something is protected at the
    // `tee-in-protect` statement, as this cannot be deferred to the moment when the respective data
    // actually moves into the software operator pool. Because this breaks for subdivided programs
    // where we don't know if an icon is currently moving towards the TEE (should replace an `H`
    // with an `A`), or out of the TEE (does not replace anything). Maybe in the future one could
    // extend `bpmd` to ingest a suite of diagrams at once, then such cross-file analysis might be
    // possible (but I am not sure if it is possible at all to derive the `in` or `out` direction
    // from the multiple smaller `.bpmd` files, really not sure). So err on the safe side here. This
    // basically means that one cannot pretend to encrypt something for the TEE and then not
    // actually send it, but I am not sure whether someone would actually want to do that.
    pub regular_pool_directly_accessible_data:
        HashMap<PoolId, HashMap<SdeId, Vec<Vec<PeBpmnProtection>>>>,
    pub tee_or_mpc_directly_accessible_data:
        HashMap<PeBpmnProtection, HashMap<SdeId, Vec<Vec<PeBpmnProtection>>>>,
}

#[derive(Default)]
pub struct ProtectionPathsGraph {
    /// Can't nest HashSet in a HashSet due to HashSet not implementing Hash.
    subgraphs: HashSet<BTreeSet<EdgeId>>,
}

enum ProtectionGraphCmp {
    Sub,
    Super,
    Disjoint,
}

impl ProtectionPathsGraph {
    fn compare(&self, other: &Self) -> Result<ProtectionGraphCmp, String> {
        let mut some_smaller = false;
        let mut some_larger = false;
        let mut some_equal = false;
        let error_message = "This pe-bpmn block has both subset and superset subgraphs of another pe-bpmn block. But they must be either nested properly or not intersecting at all.";
        for ours in &self.subgraphs {
            for theirs in &other.subgraphs {
                if ours.eq(theirs) {
                    some_equal = true;
                } else if ours.is_subset(theirs) {
                    some_smaller = true;
                } else if ours.is_superset(theirs) {
                    some_larger = true;
                } else if !ours.is_disjoint(theirs) {
                    return Err(error_message.to_string());
                }
            }
        }
        if some_smaller && some_larger {
            Err(error_message.to_string())
        } else if some_larger {
            Ok(ProtectionGraphCmp::Super)
        } else if some_smaller {
            Ok(ProtectionGraphCmp::Sub)
        } else if some_equal {
            Err("This pe-bpmn block has some equal subgraphs, but it is not clear which of them is overlapping. This ambiguous situation would result in an incorrect analysis and is thus forbidden.".to_string())
        } else {
            Ok(ProtectionGraphCmp::Disjoint)
        }
    }
}

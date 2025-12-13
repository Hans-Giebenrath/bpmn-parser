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

#[derive(Debug, Eq, Hash, PartialEq, Clone, Copy)]
pub enum PoolOrProtection {
    Pool(PoolId),
    Protection(PeBpmnProtection),
}

/// This is just a slightly different form of the PeBpmn type, but more digestible for the creation
/// of the visibility table. All necessary information is assembled in one place.
#[derive(Default)]
pub struct VisibilityTableInput {
    /// This happens if a sender uses tee-protect `no-rv` or with the `@software-operator-of-tee`,
    /// as then the software operator could replace the TEE with something they control (the remote
    /// user doesn't do correct RA, so they won't notice) and hence decrypt what they should not
    /// have seen.
    /// The `BTreeSet<PeBpmnProtection>` is the set of protections which already have been present
    /// on the object. An additional `A` must be appiled hereto.
    /// Note: This does *not* look at the directly accessible data of the attacker, but only looks
    /// at the tee-in-protect node which does the encryption. This cannot be deferred to the moment
    /// when the respective data actually moves into the software operator pool, because this would
    /// break for subdivided programs where we don't know if an icon is currently moving towards the
    /// TEE (should replace an `H` with an `A`), or out of the TEE (does not replace anything).
    /// Maybe in the future one could extend `bpmd` to ingest a suite of diagrams at once, then such
    /// cross-file analysis might be possible (but I am not sure if it is possible at all to derive
    /// the `in` or `out` direction from the multiple smaller `.bpmd` files, really not sure). So
    /// err on the safe side here. This basically means that one cannot pretend to encrypt something
    /// for the TEE and then not actually send it (or add another secure channel and give that to
    /// the TEE), but I am not sure whether someone would actually want to do that. So until then we
    /// *only* look at the protections of SdeId at the tee-in-protect node.
    pub tee_vulnerable_rv: HashMap<(/*attacker*/ PoolId, SdeId), BTreeSet<PeBpmnProtection>>,
    /// That PoolId gets all the data, which is part of that TEE or MPC, with an additional H.
    /// Since protections can be nested, they also happen to get an `H`.
    /// (Conceptually a HashSet<PoolOrProtection, HashSet<PeBpmnProtection>> but just one
    /// allocation)
    pub software_operator: HashSet<(PoolOrProtection, PeBpmnProtection)>,
    /// That PoolId gets all the data, which is part of that TEE or MPC, with an additional A.
    /// Why an `A`? TEE technologies usually exclude the hardware operator from the threat model, or
    /// at least only protect against a small handful of easyish hardware attacks (cold boot). But
    /// anything beyond that is out of scope and has been shown and shown again by researchers that
    /// it can be broken. That is to say, the hardware operator must be trusted or monitored and
    /// then this can be totally reasonable for a given context. No security measure is perfect, it
    /// just makes an attack more expensive and/or time consuming. The only silver bullet is to not
    /// gather any data in the first place.
    /// TODO verify that a hardware operator is not a `(tee|mpc)-pool`.
    pub tee_hardware_operator: HashSet<(PoolId, PeBpmnProtection)>,
    /// A pool could have root access to multiple TEEs, hence a `Vec`.
    pub tee_external_root_access: HashMap<PoolId, HashSet<PeBpmnProtection>>,
    /// For generating the network operator visibility row.
    pub network_message_protections: HashMap<SdeId, HashSet<BTreeSet<PeBpmnProtection>>>,
    // Contains both the `data` nodes and data which moves via message flows.
    //
    // TODO this comment is not totally adequate and should move to `tee_vulnerable_rv`.
    pub directly_accessible_data:
        HashMap<PoolOrProtection, HashMap<SdeId, HashSet<BTreeSet<PeBpmnProtection>>>>,
}

#[derive(Default)]
pub struct ProtectionPaths {
    /// Can't nest HashSet in a HashSet due to HashSet not implementing Hash.
    subgraphs: HashSet<BTreeSet<EdgeId>>,
}

enum ProtectionGraphCmp {
    Sub,
    Super,
    Disjoint,
}

impl ProtectionPaths {
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

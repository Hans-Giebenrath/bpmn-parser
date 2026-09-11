use itertools::Itertools;

use crate::common::graph::SdeId;
use crate::common::graph::{EdgeId, PoolId};
use crate::lexer::PeBpmdProtection;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;

pub mod analysis;
pub mod parser;
pub mod visibility_table;

#[derive(Eq, Hash, PartialEq, Clone, Copy, PartialOrd, Ord)]
pub enum PoolOrProtection {
    Pool(PoolId),
    Protection(PeBpmdProtection),
}

impl Debug for PoolOrProtection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pool(PoolId(pool_idx)) => write!(f, "p({pool_idx})"),
            Self::Protection(prot) => write!(f, "prot({prot})"),
        }
    }
}

/// This is just a slightly different form of the PeBpmd type, but more digestible for the creation
/// of the visibility table. All necessary information is assembled in one place.
#[derive(Default)]
pub struct VisibilityTableInput {
    /// This happens if a sender uses tee-protect `no-rv` or with the `@software-operator-of-tee`,
    /// as then the software operator could replace the TEE with something they control (the remote
    /// user doesn't do correct RA, so they won't notice) and hence decrypt what they should not
    /// have seen.
    /// The `BTreeSet<PeBpmdProtection>` is the set of protections which already have been present
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
    pub tee_vulnerable_rv:
        HashMap<(/*attacker*/ PoolId, SdeId), HashSet<BTreeSet<PeBpmdProtection>>>,
    /// That PoolId gets all the data, which is part of that TEE or MPC, with an additional H.
    /// Since protections can be nested, they also happen to get an `H`.
    /// (Conceptually a HashSet<PoolOrProtection, HashSet<PeBpmdProtection>> but just one
    /// allocation)
    pub software_operator: HashSet<(PoolOrProtection, PeBpmdProtection)>,
    /// That PoolId gets all the data, which is part of that TEE or MPC, with an additional A.
    /// Why an `A`? TEE technologies usually exclude the hardware operator from the threat model, or
    /// at least only protect against a small handful of easyish hardware attacks (cold boot). But
    /// anything beyond that is out of scope and has been shown and shown again by researchers that
    /// it can be broken. That is to say, the hardware operator must be trusted or monitored and
    /// then this can be totally reasonable for a given context. No security measure is perfect, it
    /// just makes an attack more expensive and/or time consuming. The only silver bullet is to not
    /// gather any data in the first place.
    /// TODO verify that a hardware operator is not a `(tee|mpc)-pool`.
    pub tee_hardware_operator: HashSet<(PoolId, PeBpmdProtection)>,
    /// A pool could have root access to multiple TEEs, hence a `Vec`.
    pub tee_external_root_access: HashMap<PoolId, HashSet<PeBpmdProtection>>,
    /// For generating the network operator visibility row.
    pub network_message_protections: HashMap<SdeId, HashSet<BTreeSet<PeBpmdProtection>>>,
    // Contains both the `data` nodes and data which moves via message flows.
    //
    // TODO this comment is not totally adequate and should move to `tee_vulnerable_rv`.
    pub directly_accessible_data:
        HashMap<PoolOrProtection, HashMap<SdeId, HashSet<BTreeSet<PeBpmdProtection>>>>,
}

impl Debug for VisibilityTableInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "visibility Table Input")?;
        writeln!(f, "  tee_vulnerable_rv")?;
        for ((pool_id, sde_id), hs) in self
            .tee_vulnerable_rv
            .iter()
            .sorted_unstable_by_key(|e| e.0)
        {
            write!(f, "    p/sde({}/{}): ", pool_id.0, sde_id.0)?;
            for protections in hs.iter().sorted_unstable() {
                let protections = protections.iter().map(|e| format!("{e}")).join(", ");
                write!(f, "({protections}), ")?;
            }
            writeln!(f)?;
        }
        writeln!(f, "  software_operator")?;
        for (pool_or_protection, pe_prot) in self
            .software_operator
            .iter()
            .sorted_unstable_by_key(|e| e.0)
        {
            writeln!(f, "    {pool_or_protection:?} {pe_prot}")?;
        }
        writeln!(f, "  tee_hardware_operator")?;
        for (PoolId(pool_idx), pe_prot) in self
            .tee_hardware_operator
            .iter()
            .sorted_unstable_by_key(|e| e.0)
        {
            writeln!(f, "    p({pool_idx}) {pe_prot}")?;
        }
        writeln!(f, "  tee_external_root_access")?;
        for (pool_id, hs) in self
            .tee_external_root_access
            .iter()
            .sorted_unstable_by_key(|e| e.0)
        {
            let protections = hs
                .iter()
                .sorted_unstable()
                .map(|e| format!("{e}"))
                .join(", ");
            writeln!(f, "    p({}): {protections}", pool_id.0)?;
        }

        writeln!(f, "  network_message_protections")?;
        for (sde_id, hs) in self
            .network_message_protections
            .iter()
            .sorted_unstable_by_key(|e| e.0)
        {
            write!(f, "    sde({}): ", sde_id.0)?;
            for protections in hs.iter().sorted_unstable() {
                let protections = protections.iter().map(|e| format!("{e}")).join(", ");
                write!(f, "({protections}), ")?;
            }
            writeln!(f)?;
        }

        writeln!(f, "  directly_accessible_data")?;
        for (pool_or_protection, hs) in self
            .directly_accessible_data
            .iter()
            .sorted_unstable_by_key(|e| e.0)
        {
            writeln!(f, "    {pool_or_protection:?}:")?;
            for (sde_id, protections) in hs.iter().sorted_unstable_by_key(|e| e.0) {
                writeln!(f, "      sde({}):", sde_id.0)?;
                for protections in protections.iter().sorted_unstable() {
                    let protections = protections.iter().map(|e| format!("{e}")).join(", ");
                    writeln!(f, "        ({protections})")?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Default)]
pub struct ProtectionPaths {
    /// Can't nest HashSet in a HashSet due to HashSet not implementing Hash.
    subgraphs: HashSet<BTreeSet<EdgeId>>,
}

#[derive(Debug)]
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
        let error_message = "This pe-bpmd block has both subset and superset subgraphs of another pe-bpmd block. But they must be either nested properly or not intersecting at all.";
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
            Err("This pe-bpmd block has some equal subgraphs, but it is not clear which of them is overlapping. This ambiguous situation would result in an incorrect analysis and is thus forbidden.".to_string())
        } else {
            Ok(ProtectionGraphCmp::Disjoint)
        }
    }
}

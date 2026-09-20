use crate::TokenCoordinate;
use crate::graph::LaneId;
use crate::graph::NodeId;
use crate::graph::PoolId;
use crate::graph::SdeId;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Display;

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

// Copy: 32 Bytes are actually large, but whatever, I don't want to deal with the references all the
// time. If need arises, this can be changed.
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub enum PeBpmdProtection {
    SecureChannel(TokenCoordinate),
    Tee(TokenCoordinate),
    Mpc(TokenCoordinate),
}

impl PeBpmdProtection {
    pub fn is_secure_channel(&self) -> bool {
        matches!(self, PeBpmdProtection::SecureChannel(..))
    }

    pub fn tc(&self) -> TokenCoordinate {
        match self {
            &PeBpmdProtection::SecureChannel(tc)
            | &PeBpmdProtection::Tee(tc)
            | &PeBpmdProtection::Mpc(tc) => tc,
        }
    }
}

impl Display for PeBpmdProtection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PeBpmdProtection::Mpc(..) => write!(f, "mpc"),
            PeBpmdProtection::Tee(..) => write!(f, "tee"),
            PeBpmdProtection::SecureChannel(..) => write!(f, "secure-channel"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PeBpmdMeta {
    pub stroke_color: Option<String>,
    pub fill_color: Option<String>,
}

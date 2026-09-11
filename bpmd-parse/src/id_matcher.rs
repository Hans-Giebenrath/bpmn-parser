use crate::common::graph::Graph;
use crate::common::graph::LaneId;
use crate::common::graph::NodeId;
use crate::common::graph::PoolId;
use crate::lexer::is_allowed_symbol_in_label_or_id;
use fuzzy_matcher::FuzzyMatcher;

const DUMMY_SEPARATOR: char = '-';

#[derive(Default)]
pub struct IdMatcher {
    data_nodes: Vec<NodeIdMeta>,
    nondata_nodes: Vec<NodeIdMeta>,
    pools: Vec<PoolIdMeta>,
    lanes: Vec<LaneIdMeta>,
}

struct PoolIdMeta {
    /// `@identifier`
    /// If the needle matches exactly one of the node's `id`s, then no further fuzzing is done.
    ids: Vec<String>,
    /// (Pool DT x Pool IDs).
    fuzzy_haystack: Vec<String>,
    pool_id: PoolId,
}

struct LaneIdMeta {
    /// `@identifier`
    /// If the needle matches exactly one of the node's `id`s, then no further fuzzing is done.
    ids: Vec<String>,
    /// (Pool DT x Pool IDs) x (Lane DT x Lane IDs), DT=Display Text.
    fuzzy_haystack: Vec<String>,
    pool_id: PoolId,
    #[allow(dead_code)] // Not sure whether this one is useful, filtering within the same pool?
    lane_id: LaneId,
}

#[derive(Debug)]
struct NodeIdMeta {
    /// `@identifier`
    /// If the needle matches exactly one of the node's `id`s, then no further fuzzing is done.
    ids: Vec<String>,
    /// (Pool DT) x (Lane DT) x (Node DT|IDs), DT=Display Text. Pools and Lanes don't have IDs.
    fuzzy_haystack: Vec<String>,
    node_id: NodeId,
    pool_id: PoolId,
    #[allow(dead_code)] // Not sure whether this one is useful, filtering within the same pool?
    lane_id: LaneId,
}

#[derive(Debug)]
pub enum SomeId {
    NodeId(NodeId),
    PoolId(PoolId),
    LaneId(PoolId, LaneId),
}

#[derive(Default, Debug)]
enum BestMatch {
    #[default]
    None,
    Some(i64, SomeId),
    ExactMatch(SomeId),
}

impl From<Option<(i64, PoolId)>> for BestMatch {
    fn from(value: Option<(i64, PoolId)>) -> Self {
        match value {
            None => BestMatch::None,
            Some((score, pool_id)) => BestMatch::Some(score, SomeId::PoolId(pool_id)),
        }
    }
}

impl From<Option<(i64, (PoolId, LaneId))>> for BestMatch {
    fn from(value: Option<(i64, (PoolId, LaneId))>) -> Self {
        match value {
            None => BestMatch::None,
            Some((score, (pool_id, lane_id))) => {
                BestMatch::Some(score, SomeId::LaneId(pool_id, lane_id))
            }
        }
    }
}

impl From<Option<(i64, NodeId)>> for BestMatch {
    fn from(value: Option<(i64, NodeId)>) -> Self {
        match value {
            None => BestMatch::None,
            Some((score, node_id)) => BestMatch::Some(score, SomeId::NodeId(node_id)),
        }
    }
}

impl From<BestMatch> for Option<PoolId> {
    fn from(value: BestMatch) -> Self {
        match value {
            BestMatch::None => None,
            BestMatch::Some(_, SomeId::PoolId(id)) | BestMatch::ExactMatch(SomeId::PoolId(id)) => {
                Some(id)
            }
            _ => unreachable!(),
        }
    }
}

impl From<BestMatch> for Option<(PoolId, LaneId)> {
    fn from(value: BestMatch) -> Self {
        match value {
            BestMatch::None => None,
            BestMatch::Some(_, SomeId::LaneId(pool_id, lane_id))
            | BestMatch::ExactMatch(SomeId::LaneId(pool_id, lane_id)) => Some((pool_id, lane_id)),
            _ => unreachable!(),
        }
    }
}

impl From<BestMatch> for Option<NodeId> {
    fn from(value: BestMatch) -> Self {
        match value {
            BestMatch::None => None,
            BestMatch::Some(_, SomeId::NodeId(id)) | BestMatch::ExactMatch(SomeId::NodeId(id)) => {
                Some(id)
            }
            _ => unreachable!(),
        }
    }
}

impl From<BestMatch> for Option<SomeId> {
    fn from(value: BestMatch) -> Self {
        match value {
            BestMatch::None => None,
            BestMatch::Some(_, id) | BestMatch::ExactMatch(id) => Some(id),
        }
    }
}

impl BestMatch {
    fn maybe_update<F>(self, other: F) -> Self
    where
        F: FnOnce() -> BestMatch,
    {
        use BestMatch::*;

        // Don't evaluate `other` if we already have an exact match.
        if matches!(self, ExactMatch(..)) {
            return self;
        }

        let other = other();

        match (&self, &other) {
            (None, _) => other,
            (_, ExactMatch(..)) => other,
            (Some(old, _), Some(new, _)) if old < new => other,
            _ => self,
        }
    }
}

impl IdMatcher {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn register_pool(&mut self, pool_id: PoolId, ids: Vec<String>, name: Option<String>) {
        let mut fuzzy_haystack = vec![];
        for pool_fuzzy in [name.as_deref().unwrap_or_default()]
            .iter()
            .cloned()
            .chain(ids.iter().map(|id| id.as_str()))
        {
            if pool_fuzzy.is_empty() {
                continue;
            }
            let matchable = normalize(pool_fuzzy);
            fuzzy_haystack.push(matchable.clone());
        }
        self.pools.push(PoolIdMeta {
            ids,
            pool_id,
            fuzzy_haystack,
        });
    }

    pub fn register_lane(
        &mut self,
        pool_id: PoolId,
        lane_id: LaneId,
        ids: Vec<String>,
        name: Option<String>,
    ) {
        let mut fuzzy_haystack = vec![];
        // Maybe in the future pools and lanes can also have IDs (to streamline the user
        // experience), in that case the for loops should be easily extendable?
        let mut matchable = String::new();
        for pool_fuzzy in self.pool_names_and_empty(pool_id) {
            if !pool_fuzzy.is_empty() {
                matchable += &normalize(pool_fuzzy);
                matchable.push(DUMMY_SEPARATOR);
            }
            for lane_fuzzy in [name.as_deref().unwrap_or_default()]
                .iter()
                .cloned()
                .chain(ids.iter().map(|id| id.as_str()))
            {
                if lane_fuzzy.is_empty() {
                    continue;
                }
                let matchable_len = matchable.len();
                matchable += &normalize(lane_fuzzy);
                matchable.push(DUMMY_SEPARATOR);
                fuzzy_haystack.push(matchable.clone());
                matchable.truncate(matchable_len);
            }
            matchable.clear();
        }

        self.lanes.push(LaneIdMeta {
            ids,
            fuzzy_haystack,
            pool_id,
            lane_id,
        });
    }

    pub fn register_node(&mut self, node_id: NodeId, ids: Vec<String>, graph: &Graph) {
        let node = &graph.nodes[node_id];
        let mut fuzzy_haystack = vec![];
        // Maybe in the future pools and lanes can also have IDs (to streamline the user
        // experience), in that case the for loops should be easily extendable?
        let mut matchable = String::new();
        for pool_fuzzy in self.pool_names_and_empty(node.pool) {
            if !pool_fuzzy.is_empty() {
                matchable += &normalize(pool_fuzzy);
                matchable.push(DUMMY_SEPARATOR);
            }
            for lane_fuzzy in self.lane_names_and_empty(node.pool, node.lane) {
                let matchable_len = matchable.len();
                let mut matchable = matchable.clone();
                if !lane_fuzzy.is_empty() {
                    matchable += &normalize(lane_fuzzy);
                    matchable.push(DUMMY_SEPARATOR);
                }
                for node_fuzzy in [node.display_text().unwrap_or("")]
                    .iter()
                    .cloned()
                    .chain(ids.iter().map(|id| id.as_str()))
                {
                    let matchable_len = matchable.len();
                    if node_fuzzy.is_empty() {
                        // No information here which would uniquely identify the
                        continue;
                    }
                    let mut matchable = matchable.clone();
                    matchable += &normalize(node_fuzzy);
                    fuzzy_haystack.push(matchable.clone());
                    matchable.truncate(matchable_len);
                }
                matchable.truncate(matchable_len);
            }
            matchable.clear();
        }
        let meta = NodeIdMeta {
            ids,
            fuzzy_haystack,
            node_id,
            pool_id: node.pool,
            lane_id: node.lane,
        };
        if node.is_data() {
            self.data_nodes.push(meta);
        } else {
            self.nondata_nodes.push(meta);
        }
    }

    pub fn find_pool_or_nondata_id(&self, needle: &str) -> Option<SomeId> {
        self.find_pool_id_inner(needle)
            .maybe_update(|| Self::find_node_id_inner(needle, None, &self.nondata_nodes))
            .into()
    }

    pub fn find_pool_id(&self, needle: &str) -> Option<PoolId> {
        self.find_pool_id_inner(needle).into()
    }

    fn find_pool_id_inner(&self, needle: &str) -> BestMatch {
        let exact_needle = needle;
        let needle = normalize(needle);
        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
        let mut best_match = BestMatch::default();
        for meta in &self.pools {
            for id in &meta.ids {
                if exact_needle == id {
                    // Exact match is found.
                    return BestMatch::ExactMatch(SomeId::PoolId(meta.pool_id));
                }
            }
            for choice in meta.fuzzy_haystack.iter() {
                let m = matcher
                    .fuzzy_match(choice, &needle)
                    .map(|score| (score, meta.pool_id));
                best_match = best_match.maybe_update(|| m.into());
            }
        }
        best_match
    }

    fn find_lane_id_inner(&self, needle: &str) -> BestMatch {
        let exact_needle = needle;
        let needle = normalize(needle);
        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
        let mut best_match = BestMatch::default();
        for meta in &self.lanes {
            for id in &meta.ids {
                if exact_needle == id {
                    // Exact match is found.
                    return BestMatch::ExactMatch(SomeId::LaneId(meta.pool_id, meta.lane_id));
                }
            }
            for choice in meta.fuzzy_haystack.iter() {
                let m = matcher
                    .fuzzy_match(choice, &needle)
                    .map(|score| (score, (meta.pool_id, meta.lane_id)));
                best_match = best_match.maybe_update(|| m.into());
            }
        }
        best_match
    }

    /// Fuzzy-matches a lane name (optionally with pool name) across all pools,
    pub fn find_lane_id(&self, needle: &str) -> Option<(PoolId, LaneId)> {
        self.find_lane_id_inner(needle).into()
    }

    pub fn find_any_node_id(&self, needle: &str, pool_id: Option<PoolId>) -> Option<NodeId> {
        Self::find_node_id_inner(needle, pool_id, &self.nondata_nodes)
            .maybe_update(|| Self::find_node_id_inner(needle, pool_id, &self.data_nodes))
            .into()
    }

    pub fn find_data_node_id(&self, needle: &str, pool_id: Option<PoolId>) -> Option<NodeId> {
        Self::find_node_id_inner(needle, pool_id, &self.data_nodes).into()
    }

    pub fn find_nondata_node_id(&self, needle: &str, pool_id: Option<PoolId>) -> Option<NodeId> {
        Self::find_node_id_inner(needle, pool_id, &self.nondata_nodes).into()
    }

    fn find_node_id_inner(needle: &str, pool_id: Option<PoolId>, meta: &[NodeIdMeta]) -> BestMatch {
        let exact_needle = needle;
        let needle = normalize(needle);
        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
        let mut best_match = BestMatch::default();
        for meta in meta {
            if let Some(pool_id) = pool_id
                && meta.pool_id != pool_id
            {
                // Filter for a special pool.
                continue;
            }
            for id in &meta.ids {
                if exact_needle == id {
                    // Exact match is found.
                    return BestMatch::ExactMatch(SomeId::NodeId(meta.node_id));
                }
            }
            for choice in meta.fuzzy_haystack.iter() {
                let m = matcher
                    .fuzzy_match(choice, &needle)
                    .map(|score| (score, meta.node_id));
                best_match = best_match.maybe_update(|| m.into());
            }
        }
        best_match
    }

    fn pool_names_and_empty(&self, pool_id: PoolId) -> impl Iterator<Item = &str> {
        self.pools
            .iter()
            .find(|p| p.pool_id == pool_id)
            .map(|p| p.fuzzy_haystack.iter().map(|e| e.as_ref()))
            .into_iter()
            .flatten()
            .chain(std::iter::once(""))
    }

    fn lane_names_and_empty(&self, pool_id: PoolId, lane_id: LaneId) -> impl Iterator<Item = &str> {
        self.lanes
            .iter()
            .find(|l| l.pool_id == pool_id && l.lane_id == lane_id)
            .map(|l| l.fuzzy_haystack.iter().map(|e| e.as_ref()))
            .into_iter()
            .flatten()
            .chain(std::iter::once(""))
    }
}

fn normalize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if is_allowed_symbol_in_label_or_id(c) {
                c
            } else {
                DUMMY_SEPARATOR
            }
        })
        .collect()
}

//#[cfg(test)]
//mod tests {
//    use super::*;
//
//    #[test]
//    fn t() {
//        let m = IdMatcher {
//            nodes: vec![
//                IdMeta {
//                    ids: vec!["send1".to_owned()],
//                    fuzzy_haystack: vec![
//                        normalize("Service Provider Dept1 Send Message"),
//                        normalize("Service Provider Dept1 send1"),
//                    ],
//                    node_id: NodeId(1),
//                    pool_id: PoolId(0),
//                    lane_id: LaneId(0),
//                },
//                IdMeta {
//                    ids: vec!["send2".to_owned()],
//                    fuzzy_haystack: vec![
//                        normalize("End User Send Message"),
//                        normalize("End User send1"),
//                    ],
//                    node_id: NodeId(2),
//                    pool_id: PoolId(0),
//                    lane_id: LaneId(0),
//                },
//                IdMeta {
//                    // This ID is created from the initials of the first node:
//                    // "Service Provider Dept1 Send1"
//                    ids: vec!["spds".to_owned()],
//                    fuzzy_haystack: vec![normalize("End User Speed S"), normalize("End User spds")],
//                    node_id: NodeId(3),
//                    pool_id: PoolId(0),
//                    lane_id: LaneId(0),
//                },
//            ],
//        };
//
//        assert_eq!(m.find_any_node_id("send2", None), Some(NodeId(2)));
//        assert_eq!(m.find_any_node_id("spds", None), Some(NodeId(3)));
//        assert_eq!(m.find_any_node_id("eu-send", None), Some(NodeId(2)));
//        assert_eq!(m.find_any_node_id("sp-send", None), Some(NodeId(1)));
//    }
//}

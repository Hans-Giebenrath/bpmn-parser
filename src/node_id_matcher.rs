use fuzzy_matcher::FuzzyMatcher;

use crate::common::graph::Graph;
use crate::common::graph::LaneId;
use crate::common::graph::NodeId;
use crate::common::graph::PoolId;
use crate::lexer::is_allowed_symbol_in_label_or_id;

const DUMMY_SEPARATOR: char = '-';

pub struct NodeIdMatcher {
    nodes: Vec<NodeIdMeta>,
}

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

impl NodeIdMatcher {
    pub fn new() -> Self {
        NodeIdMatcher { nodes: Vec::new() }
    }

    pub fn register(&mut self, node_id: NodeId, ids: Vec<String>, graph: &Graph) {
        let node = &graph.nodes[node_id.0];
        let pool = &graph.pools[node.pool.0];
        let lane = &pool.lanes[node.lane.0];
        let mut fuzzy_haystack = vec![];
        // Maybe in the future pools and lanes can also have IDs (to streamline the user
        // experience), in that case the for loops should be easily extendable?
        let mut matchable = String::new();
        for pool_fuzzy in [pool.name.as_ref().map(|s| s.as_str()).unwrap_or_default()] {
            if !pool_fuzzy.is_empty() {
                matchable += &normalize(&pool_fuzzy);
                matchable.push(DUMMY_SEPARATOR);
            }
            for lane_fuzzy in [lane.name.as_ref().map(|s| s.as_str()).unwrap_or_default()] {
                let matchable_len = matchable.len();
                let mut matchable = matchable.clone();
                if !lane_fuzzy.is_empty() {
                    matchable += &normalize(&lane_fuzzy);
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
                    matchable += &normalize(&node_fuzzy);
                    fuzzy_haystack.push(matchable.clone());
                    matchable.truncate(matchable_len);
                }
                matchable.truncate(matchable_len);
            }
            matchable.truncate(0);
        }
        self.nodes.push(NodeIdMeta {
            ids,
            fuzzy_haystack,
            node_id,
            pool_id: node.pool,
            lane_id: node.lane,
        });
    }

    pub fn find_node_id(&self, needle: &str, pool_id: Option<PoolId>) -> Option<NodeId> {
        let exact_needle = needle;
        let normalized_needle = normalize(needle);
        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
        let mut best_match = None;
        for meta in &self.nodes {
            if let Some(pool_id) = pool_id
                && meta.pool_id != pool_id
            {
                // Filter for a special pool.
                continue;
            }
            for id in &meta.ids {
                if exact_needle == id {
                    // Exact match is found.
                    return Some(meta.node_id);
                }
            }
            for choice in meta.fuzzy_haystack.iter() {
                let m = matcher
                    .fuzzy_match(&choice, &normalized_needle)
                    .map(|score| (score, meta.node_id));
                best_match = [best_match, m]
                    .into_iter()
                    .flatten()
                    .max_by_key(|(score, _)| *score);
            }
        }
        best_match.map(|(_, node_id)| node_id)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let m = NodeIdMatcher {
            nodes: vec![
                NodeIdMeta {
                    ids: vec!["send1".to_owned()],
                    fuzzy_haystack: vec![
                        normalize("Service Provider Dept1 Send Message"),
                        normalize("Service Provider Dept1 send1"),
                    ],
                    node_id: NodeId(1),
                    pool_id: PoolId(0),
                    lane_id: LaneId(0),
                },
                NodeIdMeta {
                    ids: vec!["send2".to_owned()],
                    fuzzy_haystack: vec![
                        normalize("End User Send Message"),
                        normalize("End User send1"),
                    ],
                    node_id: NodeId(2),
                    pool_id: PoolId(0),
                    lane_id: LaneId(0),
                },
                NodeIdMeta {
                    // This ID is created from the initials of the first node:
                    // "Service Provider Dept1 Send1"
                    ids: vec!["spds".to_owned()],
                    fuzzy_haystack: vec![normalize("End User Speed S"), normalize("End User spds")],
                    node_id: NodeId(3),
                    pool_id: PoolId(0),
                    lane_id: LaneId(0),
                },
            ],
        };

        assert_eq!(m.find_node_id("send2", None), Some(NodeId(2)));
        assert_eq!(m.find_node_id("spds", None), Some(NodeId(3)));
        assert_eq!(m.find_node_id("eu-send", None), Some(NodeId(2)));
        assert_eq!(m.find_node_id("sp-send", None), Some(NodeId(1)));
    }
}

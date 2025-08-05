use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

use crate::common::graph::{Graph, LaneId, PoolId};
use crate::lexer::is_allowed_symbol_in_label_or_id;

const DUMMY_SEPARATOR: char = '-';

pub struct PoolIdMatcher {
    entries: Vec<PoolIdEntry>,
}

struct PoolIdEntry {
    name: Option<String>,
    normalized_name: Option<String>,
    pool_id: PoolId,
}

impl PoolIdMatcher {
    pub fn new() -> Self {
        PoolIdMatcher {
            entries: Vec::new(),
        }
    }

    pub fn register(&mut self, pool_id: PoolId, name: Option<String>) {
        let normalized_name = name.as_ref().map(|n| normalize(n));
        let entry = PoolIdEntry {
            name,
            normalized_name,
            pool_id,
        };
        self.entries.push(entry);
    }

    /// First attempts exact match. Falls back to fuzzy match.
    pub fn find_pool_id(&self, needle: &str) -> Option<PoolId> {
        // Try exact match first
        for entry in &self.entries {
            if let Some(name) = &entry.name {
                if name == needle {
                    return Some(entry.pool_id);
                }
            }
        }

        let normalized_needle = normalize(needle);
        let matcher = SkimMatcherV2::default();

        self.entries
            .iter()
            .filter_map(|entry| {
                entry
                    .normalized_name
                    .as_ref()
                    .and_then(|name| matcher.fuzzy_match(name, &normalized_needle))
                    .map(|score| (score, entry.pool_id))
            })
            .max_by_key(|(score, _)| *score)
            .map(|(_, pool_id)| pool_id)
    }

    /// Fuzzy-matches a lane name (optionally with pool name) across all pools,
    pub fn find_pool_and_lane_id_by_lane_name_fuzzy(
        &self,
        graph: &Graph,
        lane_needle: &str,
    ) -> Option<(PoolId, LaneId)> {
        let matcher = SkimMatcherV2::default();
        let normalized_needle = normalize(lane_needle);
        let mut best: Option<(i64, PoolId, LaneId)> = None;

        for (pool_idx, pool) in graph.pools.iter().enumerate() {
            let pool_name = pool.name.as_ref().map(|s| s.as_str()).unwrap_or("");
            let normalized_pool = normalize(pool_name);

            for (lane_idx, lane) in pool.lanes.iter().enumerate() {
                if let Some(lane_name) = &lane.name {
                    let normalized_lane = normalize(lane_name);

                    // Try matching just the lane name
                    if let Some(score) = matcher.fuzzy_match(&normalized_lane, &normalized_needle) {
                        match &best {
                            Some((best_score, _, _)) if score <= *best_score => {}
                            _ => best = Some((score, PoolId(pool_idx), LaneId(lane_idx))),
                        }
                    }

                    // Try matching "poolname-lanename"
                    let combined = format!("{normalized_pool}{DUMMY_SEPARATOR}{normalized_lane}");
                    if let Some(score) = matcher.fuzzy_match(&combined, &normalized_needle) {
                        match &best {
                            Some((best_score, _, _)) if score <= *best_score => {}
                            _ => best = Some((score, PoolId(pool_idx), LaneId(lane_idx))),
                        }
                    }
                }
            }
        }
        best.map(|(_, pool_id, lane_id)| (pool_id, lane_id))
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

// Have a macro to avoid duplicating field names in the struct def and custom default def.
// Also, later we want to parse each of these values from the DSL, so autogenerating that code as
// well can be done easily with this macro.
use std::str::FromStr;

use crate::common::{graph::MAX_NODE_WIDTH, node::LayerId};

macro_rules! define_config {
    (
        $(
            $(#[$doc:meta])*
            $field:ident : $type:ty = $default:expr
        ),* $(,)?
    ) => {
        #[derive(Debug, Clone)]
        pub struct Config {
            $(
                $(#[$doc])*
                pub $field: $type
            ),*
        }

        impl Default for Config {
            fn default() -> Self {
                Self {
                    $($field: $default),*
                }
            }
        }

        impl Config {
            pub fn apply_kv_line(&mut self, key: &str, val: &str) -> Result<(), String> {
                match key.replace("-", "_").as_str() {
                    $(
                        stringify!($field) => {
                            self.$field = <$type>::from_str(val)
                                .map_err(|e| format!("Invalid value for {}: {}. Expected a value in the form of {} (<- this is the default value)", key, e, $default))?;
                            if !stringify!($field).ends_with("_weight") && self.$field == 0.try_into().unwrap() {
                                return Err("A value of 0 is not allowed for this config option (only weights can be set to 0)".to_string());
                            }
                        }
                    )*
                    other => {
                        let mut valid_keys = vec![$(stringify!($field).replace("_", "-")),*];
                        valid_keys.sort();
                        return Err(format!("Unknown key: {}. Valid keys are:\n{}", other, valid_keys.join("\n")));
                    }
                }

                Ok(())
            }
        }
    };
}

define_config!(
    /// Space between the lane border and nodes/edges inside of the lane.
    lane_y_padding: usize = 45,
    lane_header_width: usize = 30,
    lane_x_padding: usize = 60,
    pool_header_width: usize = 30,
    vertical_space_between_pools: usize = 40,
    // Space between two pools which are on the same horizontal line.
    pool_x_margin: usize = 40,

    /// `min` because nodes have varying width. So this is for two wide blocks next to each other.
    min_horizontal_space_between_nodes: usize = 60,
    max_space_between_vertical_edge_segments: usize = 6,
    /// Cannot be less than 13, otherwise the arrowheads go strange.
    min_space_between_nodes_and_vertical_edge_segments: usize = 13,

    /// For empty lanes so that they don't collapse to a vertical line.
    height_of_empty_lane: usize = 40,
    height_of_empty_pool: usize = 100,
    dummy_node_y_padding: usize = 20,
    regular_node_y_padding: usize = 30,
    /// Spanning zero layers.
    same_layer_sequence_flow_weight: f64 = 0.125,
    /// Spanning one layer.
    short_sequence_flow_weight: f64 = 8.0,
    /// Spanning only zero or one layer.
    short_data_flow_weight: f64 = 1.0,
    /// At least spanning two layers.
    /// 3 x short_sequence_flow_weight
    long_sequence_flow_weight: f64 = 32.0,
    /// At least spanning two layers.
    long_data_flow_weight: f64 = 0.5,
    message_edge_weight: f64 = 0.015625,
    min_vertical_space_between_gateway_bendpoints: usize = 110,

    /// ```
    ///      ABC     ABC    ABC    ABC     ABC         ^
    ///  ---------------------------------------->     v `margin` distance between edge and display text
    ///  |<-->| <- margin to ends
    ///       X       X       X       X       X   <- if just applying steps
    ///                      C <- The actual center of the edge. The ABC was collapsed here because
    ///                           it would otherwise be too close to the center, resulting in weird
    ///                           visuals.
    ///                             ^ on this side we try mirror the modified step size around the center,
    ///                               so that we reach the farthest end of the edge (otherwise would
    ///                               overrun since step size is probably not a perfect multiple of the
    ///                               `edge length - 2*margin_to_ends`)
    /// ```
    display_text_margin: u32 = 8,
    display_text_edge_x_step_size: u32 = 30,
    display_text_edge_y_step_size: u32 = 30,
    display_text_edge_x_margin_to_ends: u32 = 30,
    display_text_edge_y_margin_to_ends: u32 = 20,
    display_text_edge_x_min_distance_to_center_or_else_collapse_into_center: u32 = 25,
    display_text_edge_y_min_distance_to_center_or_else_collapse_into_center: u32 = 20,

    /// When checking whether a straight data edge crosses a node boundary, this margin is applied
    /// around nodes. The goal is that edges don't go very close by a node.
    data_edge_node_collision_margin: u32 = 4,

    /// TODO The heuristic moves data nodes just at a good place according to the average distance of
    /// the recipients. Edge case: This could result in a large amount of data objects in the same
    /// (half)layer. In the future a postprocessing step should evenly distribute them to neighboring
    /// layers. Since this is in practice rather uncommon, it is not implemented, yet.
    max_nodes_per_layer: usize = 3,
);

pub(crate) struct EdgeSegmentSpace {
    pub(crate) start_x: usize,
    pub(crate) end_x: usize,
    pub(crate) center_x: usize,
}

pub(crate) enum EdgeSegmentSpaceLocation {
    // There should only be right-loops.
    LeftBorder,
    // Regular room between two node layers.
    After(LayerId),
    // There should only be left-loops.
    AfterLast(LayerId),
}

impl Config {
    pub(crate) fn space_between_layers_for_segments(&self) -> usize {
        self.min_horizontal_space_between_nodes - 2 * self.max_space_between_vertical_edge_segments
    }

    pub(crate) fn layer_width(&self) -> usize {
        self.min_horizontal_space_between_nodes + MAX_NODE_WIDTH
    }

    pub(crate) fn layer_center(&self, LayerId(layer_idx): LayerId) -> usize {
        self.pool_header_width
            + self.lane_header_width
            + self.lane_x_padding
            + MAX_NODE_WIDTH / 2
            + layer_idx * self.layer_width()
    }

    pub(crate) fn pool_width(&self, num_layers: usize) -> usize {
        // The start:
        self.pool_header_width
            + self.lane_header_width
            + self.lane_x_padding
            // For central layers:
            + self.layer_width() * num_layers.saturating_sub(1)
            // For the last layer:
            + MAX_NODE_WIDTH + self.lane_x_padding
    }

    pub(crate) fn edge_segment_space(
        &self,
        location: EdgeSegmentSpaceLocation,
    ) -> EdgeSegmentSpace {
        match location {
            EdgeSegmentSpaceLocation::LeftBorder => {
                let start_x = self.pool_header_width
                    + self.lane_header_width
                    + self.min_space_between_nodes_and_vertical_edge_segments;
                let len = self
                    .lane_x_padding
                    .strict_sub(2 * self.min_space_between_nodes_and_vertical_edge_segments);
                EdgeSegmentSpace {
                    start_x,
                    end_x: start_x + len,
                    center_x: start_x + len / 2,
                }
            }
            EdgeSegmentSpaceLocation::After(LayerId(layer_idx)) => {
                let start_x = self.pool_header_width
                    + self.lane_header_width
                    + self.lane_x_padding
                    + MAX_NODE_WIDTH
                    + self.layer_width() * layer_idx
                    + self.min_space_between_nodes_and_vertical_edge_segments;
                let len = self
                    .min_horizontal_space_between_nodes
                    .strict_sub(2 * self.min_space_between_nodes_and_vertical_edge_segments);
                EdgeSegmentSpace {
                    start_x,
                    end_x: start_x + len,
                    center_x: start_x + len / 2,
                }
            }
            EdgeSegmentSpaceLocation::AfterLast(LayerId(layer_idx)) => {
                let start_x = self.pool_header_width
                    + self.lane_header_width
                    + self.lane_x_padding
                    + MAX_NODE_WIDTH
                    + self.layer_width() * layer_idx
                    + self.min_space_between_nodes_and_vertical_edge_segments;
                let len = self
                    .lane_x_padding
                    .strict_sub(2 * self.min_space_between_nodes_and_vertical_edge_segments);
                EdgeSegmentSpace {
                    start_x,
                    end_x: start_x + len,
                    center_x: start_x + len / 2,
                }
            }
        }
    }
}

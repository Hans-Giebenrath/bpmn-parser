// Have a macro to avoid duplicating field names in the struct def and custom default def.
// Also, later we want to parse each of these values from the DSL, so autogenerating that code as
// well can be done easy with this macro.
use std::str::FromStr;

use crate::common::graph::MAX_NODE_WIDTH;

macro_rules! define_config {
    (
        $(
            $(#[$doc:meta])*
            $field:ident : $type:ty = $default:expr
        ),* $(,)?
    ) => {
        #[derive(Debug)]
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
    lane_y_padding: usize = 40,
    lane_x_padding: usize = 30,
    pool_header_width: usize = 30,
    pool_y_margin: usize = 40,
    pool_x_margin: usize = 40,
    /// `min` because nodes have varying width. So this is for two wide blocks next to each other.
    min_horizontal_space_between_nodes: usize = 60,
    max_space_between_vertical_edge_segments: usize = 6,

    /// For empty lanes so that they don't collapse to a vertical line.
    height_of_empty_lane: usize = 40,
    height_of_empty_pool: usize = 40,
    dummy_node_y_padding: usize = 20,
    regular_node_y_padding: usize = 30,
    /// Spanning only zero or one layer.
    short_sequence_flow_weight: f64 = 10.0,
    /// Spanning only zero or one layer.
    short_data_flow_weight: f64 = 0.01,
    /// At least spanning two layers.
    /// 3 x short_sequence_flow_weight
    long_sequence_flow_weight: f64 = 30.0,
    /// At least spanning two layers.
    /// 30 x short_data_flow_weight
    long_data_edge_weight: f64 = 0.3,
    message_edge_weight: f64 = 0.01,

    /// TODO The heuristic moves data nodes just at a good place according to the average distance of
    /// the recipients. Edge case: This could result in a large amount of data objects in the same
    /// (half)layer. In the future a post-processing step should evenly distribute them to neighboring
    /// layers. Since this is in practice rather uncommon, it is not implemented, yet.
    max_nodes_per_layer: usize = 3,
);

impl Config {
    pub fn space_between_layers_for_segments(&self) -> usize {
        self.min_horizontal_space_between_nodes - 2 * self.max_space_between_vertical_edge_segments
    }

    pub fn layer_width(&self) -> usize {
        self.min_horizontal_space_between_nodes + MAX_NODE_WIDTH
    }
}

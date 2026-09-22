#![no_std]
extern crate alloc;
mod all_crossing_minimization_common;
pub mod all_crossing_minimization_sweep;
pub mod back_edge_removal;
pub mod dummy_node_generation;
pub mod dummy_node_removal;
pub mod edge_routing;
pub mod fix_boundary_event_connections;
pub mod introduce_snake_edge_bisect_dummies;
mod macros;
pub mod port_assignment;
pub mod postprocess_ports_and_vertical_edges;
pub mod set_display_text_location_candidates;
pub mod solve_layer_assignment;
pub mod sort_incoming_and_outgoing;
pub mod straight_edge_math;
pub mod straight_edge_routing;
pub mod try_move_nodes_into_half_layer;
mod util;
pub mod xy_ilp;

pub use all_crossing_minimization_sweep::*;
pub use back_edge_removal::*;
use bpmd_graph::*;
use bpmd_util::timer::Timer;
pub use dummy_node_generation::*;
pub use dummy_node_removal::*;
pub use edge_routing::*;
pub use fix_boundary_event_connections::*;
pub use introduce_snake_edge_bisect_dummies::*;
pub use port_assignment::*;
pub use postprocess_ports_and_vertical_edges::*;
pub use set_display_text_location_candidates::*;
pub use solve_layer_assignment::*;
pub use sort_incoming_and_outgoing::*;
pub use straight_edge_math::*;
pub use straight_edge_routing::*;
pub use try_move_nodes_into_half_layer::*;
pub use xy_ilp::*;

pub fn layout_graph(
    graph: &mut Graph,
    timer: &mut Timer,
    font_cache: &mut FontCache,
) -> Result<(), ParseError> {
    // Phase 1
    timer.time_it("back_edge_removal", || back_edge_removal(graph))?;

    // Phase 2
    timer.time_it("solve_layer_assignment", || solve_layer_assignment(graph));
    timer.time_it("generate_dummy_nodes", || dummy_node_generation(graph));
    timer.time_it("sort_lanes_by_layer", || sort_lanes_by_layer(graph));

    // Phase 3
    timer.time_it("reduce_all_crossings_sweep", || {
        reduce_all_crossings_sweep(graph)
    })?;
    timer.time_it("sort_incoming_and_outgoing", || {
        sort_incoming_and_outgoing(graph);
    });
    timer.time_it("port_assignment", || port_assignment(graph));

    // Phase 4
    timer.time_it("assign_xy_ilp", || assign_xy_ilp(graph));

    // Phase 5
    timer.time_it("postprocess_ports_and_vertical_edges", || {
        postprocess_ports_and_vertical_edges(graph)
    });
    timer.time_it("try_move_nodes_into_half_layer", || {
        try_move_nodes_into_half_layer(graph)
    });
    timer.time_it("find_straight_edges", || find_straight_edges(graph));
    timer.time_it("edge_routing", || edge_routing(graph));
    timer.time_it("dummy_node_removal", || dummy_node_removal(graph));
    timer.time_it("fix_boundary_event_connections", || {
        fix_boundary_event_connections(graph)
    });
    timer.time_it("set_display_text_locations", || {
        set_display_text_locations(graph, font_cache)
    });

    Ok(())
}

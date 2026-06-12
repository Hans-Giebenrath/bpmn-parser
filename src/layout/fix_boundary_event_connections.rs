use proc_macros::n;

use crate::common::{
    edge::{EdgeType, RegularEdgeBendPoints},
    graph::{EVENT_NODE_HEIGHT, EVENT_NODE_WIDTH, Graph},
};

pub fn fix_boundary_event_connections(graph: &mut Graph) {
    for edge in &mut graph.edges {
        let Some(boundary_event) = &mut edge.attached_to_boundary_event else {
            continue;
        };
        let EdgeType::Regular {
            bend_points: RegularEdgeBendPoints::FullyRouted(bend_points),
            ..
        } = &mut edge.edge_type
        else {
            dbg!("This should never be the case?");
            continue;
        };
        let &(start_x, start_y) = bend_points.first().unwrap();
        boundary_event.x = start_x - EVENT_NODE_WIDTH / 2;
        boundary_event.y = start_y - EVENT_NODE_HEIGHT / 2;

        if n!(edge.from).port_is_left_or_right(start_y) {
            // For boundary events our edge starts at the boundary of the boundary event,
            // so we need to shift it. But it depends on which side it leaves. Here it is to the
            // right side (at the moment left side boundary events are unsupported).
            bend_points[0].0 += EVENT_NODE_WIDTH / 2;
        } else if start_y <= n!(edge.from).y {
            // Leaves at the top.
            bend_points[0].1 -= EVENT_NODE_HEIGHT / 2;
        } else {
            // Leaves at the bottom.
            bend_points[0].1 += EVENT_NODE_HEIGHT / 2;
        };
    }
}

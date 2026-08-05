use crate::{
    common::{
        bpmn_node::BpmnNode,
        edge::{Edge, EdgeType, FlowType, RegularEdgeBendPoints},
        graph::Graph,
        node::{Node, NodeType},
    },
    layout::collision_grid::{Grid, Line},
};

pub mod defs;
pub mod primitives;

use primitives::ElementSvgStyle;

pub fn to_svg(graph: &Graph, embed_font: bool) -> String {
    let (total_width, total_height) = graph.total_width_height();
    let mut svg = primitives::Svg::new(
        embed_font,
        total_width,
        total_height,
        prepare_collision_grid(graph),
        &graph.config,
    );
    if graph.pools[0].name.is_some() || graph.pools[0].lanes[0].name.is_some() {
        // Only render the pools if they are not anonymous.
        for pool in &graph.pools {
            let pool_style = ElementSvgStyle {
                stroke: pool.stroke_color.as_ref().map(Into::into),
                fill: pool.fill_color.as_ref().map(Into::into),
                ..Default::default()
            };
            let lane_info = pool
                .lanes
                .iter()
                .map(|lane| {
                    (
                        lane.name.as_deref().unwrap_or_default(),
                        lane.height,
                        ElementSvgStyle {
                            stroke: lane.stroke_color.as_ref().map(Into::into),
                            fill: lane.fill_color.as_ref().map(Into::into),
                            ..Default::default()
                        },
                    )
                })
                .collect::<Vec<_>>();
            svg.draw_pool(
                graph.config.pool_header_width,
                graph.config.lane_header_width,
                pool.height,
                pool.width - graph.config.pool_header_width,
                (pool.x, pool.y),
                pool.name.as_deref().unwrap_or_default(),
                pool.multiple,
                &lane_info[..],
                &pool_style,
            );
        }
    }

    for node in &graph.nodes {
        let NodeType::RealNode {
            display_text,
            event,
            ..
        } = &node.node_type
        else {
            unreachable!();
        };
        let style = node_style(node);
        match event {
            BpmnNode::Event(event_type, event_visual) => svg.draw_event(
                (node.x, node.y),
                display_text,
                *event_type,
                *event_visual,
                &style,
                node.side_of_first_incoming_flow(graph, Edge::is_sequence_flow),
            ),
            BpmnNode::Gateway(gateway_type) => svg.draw_gateway(
                (node.x, node.y),
                display_text,
                &style,
                *gateway_type,
                node.side_of_first_incoming_flow(graph, Edge::is_sequence_flow),
            ),
            BpmnNode::Activity(activity_type, activity_marker) => svg.draw_task(
                (node.x, node.y),
                display_text,
                &style,
                *activity_type,
                *activity_marker,
            ),
            BpmnNode::Data(data_type, ..) => svg.draw_data(
                (node.x, node.y),
                display_text,
                *data_type,
                &style,
                node.side_of_first_incoming_flow(graph, Edge::is_data_flow),
            ),
        }
    }

    for edge in &graph.edges {
        let EdgeType::Regular {
            bend_points: RegularEdgeBendPoints::FullyRouted(bend_points),
            text,
        } = &edge.edge_type
        else {
            dbg!("This should never be the case?");
            continue;
        };

        let style = edge_style(edge);
        if let Some(boundary_event) = &edge.attached_to_boundary_event {
            svg.draw_boundary_event(
                (boundary_event.x, boundary_event.y),
                boundary_event.event_type,
                boundary_event.interrupt_kind,
                &style,
            );
        }

        svg.draw_flow(bend_points, text, &edge.flow_type, &style);
    }

    svg.finish()
}

fn edge_style(edge: &Edge) -> ElementSvgStyle {
    ElementSvgStyle {
        stroke: edge.stroke_color.as_ref().map(Into::into),
        font_color: edge.stroke_color.as_ref().map(Into::into),
        ..Default::default()
    }
}

fn node_style(node: &Node) -> ElementSvgStyle {
    ElementSvgStyle {
        stroke: node.stroke_color.as_ref().map(Into::into),
        font_color: node.stroke_color.as_ref().map(Into::into),
        ..Default::default()
    }
}

fn prepare_collision_grid(graph: &Graph) -> Grid {
    let mut quad_tree = Grid::new(graph.total_width_height());

    for edge in &graph.edges {
        let EdgeType::Regular {
            bend_points: RegularEdgeBendPoints::FullyRouted(bend_points),
            ..
        } = &edge.edge_type
        else {
            unreachable!("Only regular edges at this point");
        };
        let weight = match edge.flow_type {
            FlowType::MessageFlow(..) => 8,
            FlowType::DataFlow(..) => 2,
            FlowType::SequenceFlow => 10,
        };
        for [start, end] in bend_points.array_windows() {
            quad_tree.insert(&Line::new(start, end), weight);
        }
    }

    for node in &graph.nodes {
        let node_weight = 10;
        if node.is_gateway() {
            #[rustfmt::skip]
            let (top, right, bottom, left)   = (
                (node.x + node.width / 2, node.y                  ),
                (node.x + node.width    , node.y + node.height / 2),
                (node.x + node.width / 2, node.y + node.height    ),
                (node.x                 , node.y + node.height / 2)
             );

            quad_tree.insert_quadrangle(top, right, bottom, left, node_weight);
        } else {
            let tl = (node.x, node.y);
            let tr = (node.x + node.width, node.y);
            let br = (node.x + node.width, node.y + node.height);
            let bl = (node.x, node.y + node.height);

            quad_tree.insert_quadrangle(tl, tr, br, bl, node_weight);
        }
    }

    for pool in &graph.pools {
        for lane in &pool.lanes {
            let lane_weight = 9;
            let tl = (lane.x, lane.y);
            let tr = (lane.x + lane.width, lane.y);
            let br = (lane.x + lane.width, lane.y + lane.height);
            let bl = (lane.x, lane.y + lane.height);

            quad_tree.insert_quadrangle(tl, tr, br, bl, lane_weight);
        }
    }

    quad_tree
}

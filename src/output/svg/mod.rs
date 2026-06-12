use crate::common::{
    bpmn_node::BpmnNode,
    edge::{Edge, EdgeType, RegularEdgeBendPoints},
    graph::{EVENT_NODE_HEIGHT, EVENT_NODE_WIDTH, Graph},
    node::{Node, NodeType},
};

pub mod defs;
pub mod primitives;

use primitives::ElementSvgStyle;
use proc_macros::n;

pub fn to_svg(graph: &Graph) -> String {
    let mut svg = primitives::Svg::new();
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
            pool.width,
            (pool.x, pool.y),
            pool.name.as_deref().unwrap_or_default(),
            &lane_info[..],
            &pool_style,
        );
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
            ),
            BpmnNode::Gateway(gateway_type) => {
                svg.draw_gateway((node.x, node.y), display_text, &style, *gateway_type)
            }
            BpmnNode::Activity(activity_type) => {
                svg.draw_task((node.x, node.y), display_text, &style, activity_type)
            }
            BpmnNode::Data(data_type, ..) => {
                svg.draw_data((node.x, node.y), display_text, *data_type, &style)
            }
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
        svg.draw_flow(bend_points, text, &edge.flow_type, &edge_style(edge));
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

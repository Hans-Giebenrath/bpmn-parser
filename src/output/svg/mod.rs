use crate::common::graph::Graph;

pub mod defs;
pub mod primitives;

use primitives::ElementSvgStyle;

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

    svg.finish()
}

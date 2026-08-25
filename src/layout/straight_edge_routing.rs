use crate::common::bpmn_node::BpmnNode;
use crate::common::edge::EdgeType;
use crate::common::edge::RegularEdgeBendPoints;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::index_iter::IterIndices;
use crate::common::node::Node;
use crate::common::node::NodeType;
use crate::layout::collision_grid::Grid;
use crate::layout::straight_edge_math;
use crate::lexer::DataType;
use proc_macros::e;
use proc_macros::n;

pub fn find_straight_edges(graph: &mut Graph) {
    let mut grid = Grid::new(graph.total_width_height());

    let margin = graph.config.data_edge_node_collision_margin;
    for node in &graph.nodes {
        if node.is_any_dummy() {
            continue;
        }
        let node_weight = 10;
        let margin = margin as usize;
        if node.is_gateway() {
            #[rustfmt::skip]
            let (top, right, bottom, left)   = (
        ( node.x + node.width / 2                       , node.y                  .saturating_sub(margin)),
        ((node.x + node.width)   .saturating_add(margin), node.y + node.height / 2         ),
        ( node.x + node.width / 2                       ,(node.y + node.height)   .saturating_add( margin)),
        ( node.x                 .saturating_sub(margin), node.y + node.height / 2         )
             );

            grid.insert_quadrangle(top, right, bottom, left, node_weight);
        } else {
            let tl = (node.x.saturating_sub(margin), node.y.saturating_sub(margin));
            let tr = (
                (node.x + node.width).saturating_add(margin),
                node.y.saturating_sub(margin),
            );
            let br = (
                (node.x + node.width).saturating_add(margin),
                (node.y + node.height).saturating_add(margin),
            );
            let bl = (
                node.x.saturating_sub(margin),
                (node.y + node.height).saturating_add(margin),
            );

            grid.insert_quadrangle(tl, tr, br, bl, node_weight);
        }
    }

    data_edge_routing(graph, &grid, margin);
    sequence_edge_routing(graph);
}

fn sequence_edge_routing(graph: &mut Graph) {
    for edge_id in (0..graph.edges.len()).map(EdgeId) {
        let edge = &e!(edge_id);
        if !edge.is_sequence_flow() || !edge.is_regular() {
            continue;
        }

        let [start @ (_, start_y), end @ (_, end_y)] = graph.start_and_end_ports(edge_id);
        let is_reversed = edge.is_reversed;
        let is_vertical = edge.is_vertical;
        let from = &n!(edge.from);
        let EdgeType::Regular {
            bend_points: out_bend_points,
            ..
        } = &mut e!(edge_id).edge_type
        else {
            // Straight dummy edges will be stitched together in the `dummy_node_removal`
            // phase, hence they are skipped here.
            continue;
        };

        if from.is_blackbox_node() {
            // Route blackbox edges here, so they don't need to be handled later in
            // the edge routing phase anymore. There should not be any dummy edges
            // in the blackbox, so it is just about regular edges.
            *out_bend_points = RegularEdgeBendPoints::FullyRouted(vec![]);
            continue;
        }

        if start_y != end_y && !is_vertical {
            // Not a straight edge.
            continue;
        }
        let bend_points = if is_reversed {
            vec![end, start]
        } else {
            vec![start, end]
        };
        *out_bend_points = RegularEdgeBendPoints::FullyRouted(bend_points);
    }
}

fn data_edge_routing(graph: &mut Graph, grid: &Grid, margin: u32) {
    'next_edge: for edge_id in graph.edges.len().iter_indices(true).map(EdgeId) {
        let edge = &graph.edges[edge_id];
        if !edge.is_data_flow() {
            continue;
        }

        let text = match &edge.edge_type {
            EdgeType::Regular { text, .. } => text,
            EdgeType::ReplacedByDummies { text, .. } => text,
            EdgeType::DummyEdge { .. } => continue,
        };

        if n!(edge.from).is_blackbox_node() {
            // It no longer is replaced by dummies.
            graph.edges[edge_id].edge_type = EdgeType::Regular {
                text: text.clone(),
                bend_points: RegularEdgeBendPoints::FullyRouted(vec![]),
            };

            continue 'next_edge;
        }

        // Note: This looks at the original, ReplacedByDummies edges as well!
        let (
            from_collision_boundary,
            from_visual_boundary,
            (from_x, from_y),
            (from_center_x, from_center_y),
        ) = prepare_data(&n!(edge.from));
        let (to_collision_boundary, to_visual_boundary, (to_x, to_y), (to_center_x, to_center_y)) =
            prepare_data(&n!(edge.to));
        assert_ne!((from_center_x, from_center_y), (to_center_x, to_center_y));
        let degree = ((to_center_y as f64 - from_center_y as f64)
            .atan2(to_center_x as f64 - from_center_x as f64))
        .to_degrees()
        .round() as isize;
        for offset_degrees in [0_isize, 12, -12, 18, -18] {
            let start_idx = (degree + offset_degrees).rem_euclid(360) as usize;
            let end_idx = (degree - offset_degrees + 180).rem_euclid(360) as usize;
            let (start_collision, start_endpoint) = endpoints(
                from_collision_boundary,
                from_visual_boundary,
                (from_x, from_y),
                start_idx,
                margin,
            );
            let (end_collision, end_endpoint) = endpoints(
                to_collision_boundary,
                to_visual_boundary,
                (to_x, to_y),
                end_idx,
                margin,
            );

            if grid.line_intersection_weight(start_collision, end_collision) > 0 {
                continue;
            }
            let mut bend_points = vec![start_endpoint, end_endpoint];
            if edge.is_reversed {
                bend_points.reverse();
            }

            // It no longer is replaced by dummies.
            graph.edges[edge_id].edge_type = EdgeType::Regular {
                text: text.clone(),
                bend_points: RegularEdgeBendPoints::FullyRouted(bend_points),
            };

            // The dummy edges are not explicitly iterated in the upcoming edge routing phase,
            // so we can just leave them in the state which they are. Also no need to
            // touch the outgoing/incoming fields as they are no longer looked at.
            continue 'next_edge;
        }
    }
}

fn prepare_data(node: &Node) -> (&[(u8, u8); 360], &[(u8, u8); 360], (u32, u32), (u32, u32)) {
    let (collision_boundary_table, visual_boundary_table) = match &node.node_type {
        NodeType::RealNode {
            event: BpmnNode::Gateway(..),
            ..
        } => (
            &straight_edge_math::boundary_lookuptable::GATEWAY,
            // TODO make a custom gateway visual boundary.
            &straight_edge_math::boundary_lookuptable::GATEWAY,
        ),
        NodeType::RealNode {
            event: BpmnNode::Event(..),
            ..
        } => (
            &straight_edge_math::boundary_lookuptable::EVENT,
            &straight_edge_math::boundary_lookuptable::EVENT_TRUE_BOUNDARY,
        ),
        NodeType::RealNode {
            event: BpmnNode::Activity(..),
            ..
        } => (
            &straight_edge_math::boundary_lookuptable::ACTIVITY,
            &straight_edge_math::boundary_lookuptable::ACTIVITY,
        ),
        NodeType::RealNode {
            event: BpmnNode::Data(DataType::Store, ..),
            ..
        } => (
            &straight_edge_math::boundary_lookuptable::DATASTORE,
            // TODO make a custom data store visual boundary.
            &straight_edge_math::boundary_lookuptable::DATASTORE,
        ),
        NodeType::RealNode {
            event: BpmnNode::Data(DataType::Object, ..),
            ..
        } => (
            &straight_edge_math::boundary_lookuptable::DATAOBJECT,
            // TODO make a custom data object visual boundary.
            &straight_edge_math::boundary_lookuptable::DATAOBJECT,
        ),
        _ => unreachable!("An original data edge should only be connected to real nodes."),
    };
    (
        collision_boundary_table,
        visual_boundary_table,
        (node.x as u32, node.y as u32),
        (
            (node.x + node.width / 2) as u32,
            (node.y + node.height / 2) as u32,
        ),
    )
}

fn endpoints(
    collision_boundary: &[(u8, u8); 360],
    visual_boundary: &[(u8, u8); 360],
    (x, y): (u32, u32),
    degrees: usize,
    margin: u32,
) -> (
    /* collision endpoint */ (u32, u32),
    /* result / display end point */ (usize, usize),
) {
    let (offset_x, offset_y) = collision_boundary[degrees];
    let (collision_offset_x, collision_offset_y) =
        straight_edge_math::boundary_lookuptable::OFFSET[degrees];
    (
        (
            (x + offset_x as u32)
                .saturating_add_signed((collision_offset_x * (margin + 1) as f32) as i32),
            (y + offset_y as u32)
                .saturating_add_signed((collision_offset_y * (margin + 1) as f32) as i32),
        ),
        (
            (x + visual_boundary[degrees].0 as u32) as usize,
            (y + visual_boundary[degrees].1 as u32) as usize,
        ),
    )
}

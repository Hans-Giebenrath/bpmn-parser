use crate::common::edge::Edge;
use crate::common::edge::EdgeType;
use crate::common::edge::RegularEdgeBendPoints;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::graph::NodeId;
use crate::common::node::Node;
use proc_macros::e;
use std::collections::HashMap;

const NODE_MARGIN: usize = 5;

pub fn find_straight_edges(graph: &mut Graph) {
    // HashMap to store coordinates of obstacles with node id and is_datanode as key
    // HashMap stores tuples of top left and bottom right coordinates of obstacles
    // TODO filter this more by pools and possibly layers, as a sort of quad tree (not by lanes
    // since data edges can span lanes). Currently this function is very slow.
    let mut matrix: HashMap<usize, (usize, usize, usize, usize)> = HashMap::new();

    for node in &graph.nodes {
        if node.is_any_dummy() {
            continue;
        }
        add_to_matrix(
            &mut matrix,
            &node.id.0,
            node.width,
            node.height,
            node.x,
            node.y,
        );
    }

    data_edge_routing(&matrix, graph);
    sequence_edge_routing(graph);
}

fn add_to_matrix(
    matrix: &mut HashMap<usize, (usize, usize, usize, usize)>,
    node_id: &usize,
    width: usize,
    height: usize,
    x: usize,
    y: usize,
) {
    matrix.insert(*node_id, (x, y, x + width, y + height));
}

fn is_in_obstacle_ignore_self(
    x: usize,
    y: usize,
    from_id: usize,
    to_id: usize,
    matrix: &HashMap<usize, (usize, usize, usize, usize)>,
) -> bool {
    for (id, (x1, y1, x2, y2)) in matrix.iter() {
        if (*id == from_id) || (*id == to_id) {
            if x > *x1 && x < *x2 && y > *y1 && y < *y2 {
                return true;
            }
        } else if x >= *x1 - NODE_MARGIN
            && x <= *x2 + NODE_MARGIN
            && y >= *y1 - NODE_MARGIN
            && y <= *y2 + NODE_MARGIN
        {
            return true;
        }
    }
    false
}

fn sequence_edge_routing(graph: &mut Graph) {
    for edge_idx in 0..graph.edges.len() {
        let edge_id = EdgeId(edge_idx);
        let edge = &e!(edge_id);
        if !edge.is_sequence_flow() || !edge.is_regular() {
            // Straight dummy edges will be stitched together in the `replace_dummy_nodes`
            // phase, hence they are skipped here.
            continue;
        }
        assert!(
            !edge.is_vertical,
            "Check whether this should be handled via new HorizontalSegmentDummy nodes."
        );
        let edge_id = EdgeId(edge_idx);
        let [start @ (_, start_y), end @ (_, end_y)] = graph.start_and_end_ports(edge_id);
        if start_y != end_y {
            continue;
        }
        let bend_points = if edge.is_reversed {
            vec![end, start]
        } else {
            vec![start, end]
        };
        let EdgeType::Regular {
            bend_points: out_bend_points,
            ..
        } = &mut e!(edge_id).edge_type
        else {
            unreachable!("Verified `edge.is_regular()` above.");
        };
        *out_bend_points = RegularEdgeBendPoints::FullyRouted(bend_points);
    }
}

fn data_edge_routing(matrix: &HashMap<usize, (usize, usize, usize, usize)>, graph: &mut Graph) {
    let mut start_point_buffer = vec![];
    let mut end_point_buffer = vec![];
    for data_edge_idx in 0..graph.edges.len() {
        let data_edge = &mut graph.edges[data_edge_idx];
        if !Edge::is_data_flow(data_edge) {
            continue;
        }

        let text = match &data_edge.edge_type {
            EdgeType::Regular { text, .. } => text,
            EdgeType::ReplacedByDummies { text, .. } => text,
            EdgeType::DummyEdge { .. } => continue,
        };
        // Note: This looks at the original, ReplacedByDummies edges as well!
        start_point_buffer.clear();
        end_point_buffer.clear();
        find_start_and_end_points(
            &graph.nodes[data_edge.from],
            &graph.nodes[data_edge.to],
            &mut start_point_buffer,
            &mut end_point_buffer,
        );

        let mut bend_points = Vec::new();
        for start_points in start_point_buffer.iter() {
            for end_points in end_point_buffer.iter() {
                if possible_direct(
                    matrix,
                    start_points,
                    end_points,
                    data_edge.from,
                    data_edge.to,
                ) {
                    bend_points.push((*start_points, *end_points));
                }
            }
        }
        if !bend_points.is_empty() {
            let edge = find_shortest_path(&bend_points);
            let mut bend_points = vec![edge.0, edge.1];
            if data_edge.is_reversed {
                bend_points.reverse();
            }

            // It no longer is replaced by dummies.
            graph.edges[data_edge_idx].edge_type = EdgeType::Regular {
                text: text.clone(),
                bend_points: RegularEdgeBendPoints::FullyRouted(bend_points),
            };

            // The dummy edges are not explicitly iterated in the upcoming edge routing phase,
            // so we can just leave them in the state which they are. Also no need to
            // touch the outgoing/incoming fields as they are no longer looked at.
        }
    }
}

fn possible_direct(
    matrix: &HashMap<usize, (usize, usize, usize, usize)>,
    start_xy: &(usize, usize),
    end_xy: &(usize, usize),
    from_id: NodeId,
    to_id: NodeId,
) -> bool {
    let dx = end_xy.0 as f64 - start_xy.0 as f64;
    let dy = end_xy.1 as f64 - start_xy.1 as f64;

    let steps = dx.abs().max(dy.abs()) as usize;
    let step_x = dx / steps as f64;
    let step_y = dy / steps as f64;

    let mut x = start_xy.0 as f64;
    let mut y = start_xy.1 as f64;

    for _ in 0..=steps {
        if is_in_obstacle_ignore_self(
            x.round() as usize,
            y.round() as usize,
            from_id.0,
            to_id.0,
            matrix,
        ) {
            return false;
        }
        x += step_x;
        y += step_y;
    }

    true
}

fn find_start_and_end_points(
    from_node: &Node,
    to_node: &Node,
    start_point_buffer: &mut Vec<(usize, usize)>,
    end_point_buffer: &mut Vec<(usize, usize)>,
) {
    let (dn_x, dn_y, dn_width, dn_height) =
        (from_node.x, from_node.y, from_node.width, from_node.height);
    let (node_x, node_y, node_width, node_height) =
        (to_node.x, to_node.y, to_node.width, to_node.height);

    // left
    start_point_buffer.push((dn_x, dn_y + dn_height / 2));
    // left_up
    start_point_buffer.push((dn_x, dn_y + dn_height / 4));
    // left_down
    start_point_buffer.push((dn_x, dn_y + dn_height * 3 / 4));
    // right
    start_point_buffer.push((dn_x + dn_width, dn_y + dn_height / 2));
    // right_up
    start_point_buffer.push((dn_x + dn_width, dn_y + dn_height / 4));
    // right_down
    start_point_buffer.push((dn_x + dn_width, dn_y + dn_height * 3 / 4));
    // top
    start_point_buffer.push((dn_x + dn_width / 2, dn_y));
    // top_left
    start_point_buffer.push((dn_x + dn_width / 4, dn_y));
    // top_right
    start_point_buffer.push((dn_x + dn_width * 3 / 4, dn_y));
    // bottom
    start_point_buffer.push((dn_x + dn_width / 2, dn_y + dn_height));
    // bottom_left
    start_point_buffer.push((dn_x + dn_width / 4, dn_y + dn_height));
    // bottom_right
    start_point_buffer.push((dn_x + dn_width * 3 / 4, dn_y + dn_height));

    // left_up
    end_point_buffer.push((node_x, node_y + node_height / 4));
    // left_down
    end_point_buffer.push((node_x, node_y + node_height * 3 / 4));
    // right_up
    end_point_buffer.push((node_x + node_width, node_y + node_height / 4));
    // right_down
    end_point_buffer.push((node_x + node_width, node_y + node_height * 3 / 4));
    // top
    end_point_buffer.push((node_x + node_width / 2, node_y));
    // top_left
    end_point_buffer.push((node_x + node_width / 4, node_y));
    // top_right
    end_point_buffer.push((node_x + node_width * 3 / 4, node_y));
    // bottom
    end_point_buffer.push((node_x + node_width / 2, node_y + node_height));
    // bottom_left
    end_point_buffer.push((node_x + node_width / 4, node_y + node_height));
    // bottom_right
    end_point_buffer.push((node_x + node_width * 3 / 4, node_y + node_height));
}

fn find_shortest_path(
    bend_points: &[((usize, usize), (usize, usize))],
) -> ((usize, usize), (usize, usize)) {
    let mut min_distance = usize::MAX;
    let mut start_xy: (usize, usize) = (0, 0);
    let mut end_xy: (usize, usize) = (0, 0);
    for (start, end) in bend_points.iter() {
        let distance = ((start.0 as isize - end.0 as isize).abs()
            + (start.1 as isize - end.1 as isize).abs()) as usize;
        if distance < min_distance {
            min_distance = distance;
            start_xy = *start;
            end_xy = *end;
        }
    }

    (start_xy, end_xy)
}

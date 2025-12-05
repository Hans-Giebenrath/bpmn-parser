use crate::common::edge::FlowType;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::graph::NodeId;
use crate::common::graph::SdeId;
use std::collections::HashSet;

pub fn create_transported_data(graph: &mut Graph) {
    for edge_id in 0..graph.edges.len() {
        if !graph.edges[edge_id].is_message_flow() {
            continue;
        }
        let edge = &graph.edges[edge_id];

        let incoming_nodes: HashSet<NodeId> = graph.nodes[edge.from]
            .incoming
            .iter()
            .map(|e| graph.edges[*e].clone())
            .filter(|e| e.is_data_flow())
            .map(|e| e.from)
            .collect();

        let incoming_sde: HashSet<SdeId> = incoming_nodes
            .iter()
            .filter_map(|node_id| graph.nodes[*node_id].get_data_aux())
            .map(|aux| aux.sde_id)
            .collect();

        let outgoing_nodes: HashSet<NodeId> = graph.nodes[edge.to]
            .outgoing
            .iter()
            .map(|e| graph.edges[*e].clone())
            .filter(|e| e.is_data_flow())
            .map(|e| e.to)
            .collect();

        let outgoing_sde: HashSet<SdeId> = outgoing_nodes
            .iter()
            .filter_map(|node_id| graph.nodes[*node_id].get_data_aux())
            .map(|aux| aux.sde_id)
            .collect();

        let intersection: Vec<SdeId> = incoming_sde.intersection(&outgoing_sde).cloned().collect();

        if let FlowType::MessageFlow(message_flow_aux) = &mut graph.edges[edge_id].flow_type {
            message_flow_aux.transported_data.extend(intersection);
            message_flow_aux.transported_data.sort();
            message_flow_aux.transported_data.dedup();
        }
    }

    for i in 0..graph.nodes.len() {
        let node = &graph.nodes[i];

        if node.is_gateway() {
            continue;
        }

        let incoming = get_edge_data_ids(graph, &node.incoming);
        let outgoing = get_edge_data_ids(graph, &node.outgoing);

        let intersect: Vec<SdeId> = incoming.intersection(&outgoing).copied().collect();
        graph.nodes[i].add_node_transported_data(&intersect);
    }
}

fn get_edge_data_ids(graph: &Graph, edge_ids: &[EdgeId]) -> HashSet<SdeId> {
    let mut ids = HashSet::new();

    for edge_id in edge_ids {
        let edge = &graph.edges[*edge_id];
        ids.extend(edge.get_transported_data());
    }
    ids
}

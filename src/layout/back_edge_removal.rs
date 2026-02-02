use crate::common::graph::{EdgeId, Graph, NodeId};
use crate::common::node::Node;
use proc_macros::e;
use proc_macros::n;
use proc_macros::to;

// Things to do:
//
// * Find the actual back edges. The idea is to traverse through the graph in a depth first search.
// Whenever we re-visit a node, then the last edge is marked as a back edge. The idea is that we
// want the longest paths possible going from left to right. I suspect that this is what users want
// from a back edge discovery in BPMN. If this does not work, then at some point in the future I
// might allow flipping nodes, but I think this is an unintuitive way to structure processes (the
// process is going back in time??!)
// * For a back edge that starts in a higher layer k and ends in a lower layer l, in all layers
// from l to k there are new dummy nodes added. All edges point to the right. Especially, the new
// dummy node in layer l has two outgoing edges (to the original edge target in the same layer, and
// to the new dummy node in layer l+1), and the new dummy node in layer k has two incoming edges
// (from the new dummy node in layer k-1, and from the original edge start in the same layer k).
// This way we don't need to make any shenanigans with modifying the incoming and outgoing
// properties. Additionally, while my thesis says that there should be new dummy nodes on the
// layers l-1 and k+1, these are actually already added temporarily in the crossing minimization
// phase, so all is good.
// * I am not even sure whether a replaced edge needs to be marked with the "is reversed"
// boolean... since it is replaced already ...

// Supporting left-right constraints: When we find that following a left-right constraint
// leads us to an already visited node (within our dfs) then this means there must be a back edge in
// our traversed path: Must be one where there is no .
//
// pub back_edge_removal(nodes, edges, left-rights, same-layers) {

// a -> b -> c -> d
// a leftof b
// c leftof a
// b leftof c

// Strategy:
// - Iterate all cycles:
//   - for each cycle C, determine the necessary back edge (even if there are already some back
//     edges present)
// - Iterate all cycles (again, yes):
//   - for each cycle C, gather all back edges. Now we create an ILP:
//     Variables: For each back edge there is a variable `be_i ∊ {0, 1}`
//     `Min Sum_i be_i`
//     Constraints:
//      - For each cycle, at least one back edge must be 1. (be_i + be_{i+1} + ... >= 1)
//      - Each user-defined back edge must be 1.

pub fn back_edge_removal(graph: &mut Graph) {}

fn iterate_all_cycles<F>(callback: F, graph: &Graph) {
    for node in &graph.nodes {
        if !node.incoming.is_empty() {
            continue;
        }
    }
}

/// Message flows and data flows are not considered as back edges.
enum EdgeType {
    SequenceFlow,
    LeftOfConstraint,
    AboveConstraint,
    SameLayerConstraint,
}

/// Name `Edge` is already used.
struct PathSegment {
    etype: EdgeType,
    from: NodeId,
    to: NodeId,
}

struct BackEdge {
    from: NodeId,
    to: NodeId,
}

fn traverse_path<F>(callback: F, graph: &Graph, current_node: &Node, path: &mut Vec<PathSegment>) {
    for edge in &current_node.outgoing {
        if e!(*edge).is_sequence_flow() {
            recurse_maybe(
                callback,
                graph,
                current_node,
                &to!(*edge),
                EdgeType::SequenceFlow,
                path,
            );
        }
    }
    TODO go iterate layout constraints of the graph.
}

fn recurse_maybe<F>(
    callback: F,
    graph: &Graph,
    current_node: &Node,
    next_node: &Node,
    etype: EdgeType,
    path: &mut Vec<PathSegment>,
) {
    path.push(PathSegment {
        etype,
        from: current_node.id,
        to: next_node.id,
    });
    for (idx, segment) in path.iter().enumerate() {
        if segment.from == next_node.id {
            callback(&path[idx..]);
            path.pop();
            return;
        }
    }

    traverse_path(callback, graph, next_node, path);
    path.pop();
}

use crate::common::graph::{EdgeId, Graph, NodeId};
use crate::common::node::Node;
use crate::layout::constraint::{Above, LeftOf, SameLayer};
use good_lp::*;
use proc_macros::e;
use proc_macros::n;
use proc_macros::to;
use std::collections::HashMap;
use std::iter::Sum;

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
//      - For each cycle, at least one back edge must be 1. `(be_i + be_{i+1} + ... >= 1)`
//      - Each user-defined back edge must be 1.
//
// Invalid situations to discover:
//  (1) A cycle with left-of and no sequence flow exists.
//  (2) A cycle of only above edges exist (where the directed variant creates a cycle, so not
//      following above edges in reverse direction as is done in (1))

pub fn back_edge_removal(graph: &mut Graph) -> Result<(), String> {
    let mut back_edges = Vec::new();
    let mut bad_cycle: Option<String> = None;
    iterate_all_cycles(
        &mut |cycle| match analyse_cycle(cycle) {
            Ok(Some(back_edge)) => back_edges.push(back_edge),
            Ok(None) => (),
            Err(error) => bad_cycle = Some(error),
        },
        graph,
    );
    if let Some(error) = bad_cycle {
        return Err(error);
    }
    let mut back_edge_groups = Vec::new();
    iterate_all_cycles(
        &mut |cycle| {
            back_edge_groups.push(
                cycle
                    .iter()
                    .filter_map(|segment| {
                        if let EdgeType::SequenceFlow(edge_id) = &segment.etype
                            && back_edges.contains(edge_id)
                        {
                            Some(*edge_id)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>(),
            );
        },
        graph,
    );
    if back_edge_groups.iter().all(|group| group.len() <= 1) {
        // We only have trivial groups. So no need to spin up the ILP to solve minimum SAT.
        graph.computed_back_edges = back_edge_groups
            .into_iter()
            .flat_map(|group| group.first().cloned())
            .collect();
    } else {
        solve_ilp(graph, &back_edge_groups);
    }
    Ok(())
}

fn solve_ilp(graph: &mut Graph, back_edge_groups: &[Vec<EdgeId>]) {
    let mut vars = variables!();
    let mut edge_vars = HashMap::new();
    for edge_id in back_edge_groups.iter().flatten() {
        edge_vars
            .entry(*edge_id)
            .or_insert_with(|| vars.add(variable().binary()));
    }

    // Minimize the amount of back edges.
    let mut objective = Expression::from(0.0);
    for (var, _) in vars.iter_variables_with_def() {
        objective += var;
    }

    // Create the satisfiability constraints (a || b) & (b) & (c || d) & ...
    let mut problem = vars.minimise(objective).using(default_solver);
    for group in back_edge_groups {
        problem.add_constraint(Expression::sum(group.iter().map(|edge| edge_vars[edge])).geq(1));
    }

    // Back edges are those who have been kept at 1.0.
    let solution = problem.solve().unwrap();
    graph.computed_back_edges = edge_vars
        .into_iter()
        .filter(|(_, var)| solution.value(*var) == 1.0)
        .map(|(edge_id, _)| edge_id)
        .collect();
}

fn analyse_cycle(cycle: &[PathSegment]) -> Result<Option<EdgeId>, String> {
    //let mut earliest_sf = None;
    let mut contains_left_of_constraint = false;
    let mut contains_above_flipped = false;
    let mut contains_above_not_flipped = false;
    let mut contains_same_layer = false;
    for segment in cycle.iter().rev() {
        match &segment.etype {
            EdgeType::SequenceFlow(edge_id) /*if earliest_sf.is_none()*/ => {
                return Ok(Some(*edge_id));
                //earliest_sf = Some(*edge_id)
            }
            //EdgeType::SequenceFlow(edge_id) => (),
            EdgeType::LeftOfConstraint => contains_left_of_constraint = true,
            EdgeType::AboveConstraint { flipped } if *flipped == true => {
                contains_above_flipped = true
            }
            EdgeType::AboveConstraint { .. } => contains_above_not_flipped = true,
            EdgeType::SameLayerConstraint => contains_same_layer = true,
        }
    }
    match (
        contains_left_of_constraint,
        contains_above_not_flipped,
        contains_above_flipped,
        contains_same_layer,
    ) {
        (false, false, false, false) => unreachable!(),
        (true, _, _, _) => {
            // A path consisting only of constraints.
            todo!("return an error")
        }
        // Only "Above" constraints in the same direction.
        (false, true, false, false) | (false, false, true, false) => {
            todo!("return an error")
        }
        // Same layer constraints but not in a dangerous circle.
        (false, _, _, _) => Ok(None),
    }
}

fn iterate_all_cycles(callback: &mut dyn FnMut(&[PathSegment]), graph: &Graph) {
    let mut path = Vec::new();
    for node in &graph.nodes {
        if !node.incoming.is_empty() {
            continue;
        }
        traverse_path(callback, graph, node, &mut path);
        assert!(path.is_empty());
    }
}

/// Message flows and data flows are not considered as back edges.
#[derive(Debug)]
enum EdgeType {
    SequenceFlow(EdgeId),
    LeftOfConstraint,
    AboveConstraint { flipped: bool },
    SameLayerConstraint,
}

/// Name `Edge` is already used.
#[derive(Debug)]
struct PathSegment {
    etype: EdgeType,
    from: NodeId,
    to: NodeId,
}

struct BackEdge {
    from: NodeId,
    to: NodeId,
}

fn traverse_path(
    callback: &mut dyn FnMut(&[PathSegment]),
    graph: &Graph,
    current_node: &Node,
    path: &mut Vec<PathSegment>,
) {
    for edge in &current_node.outgoing {
        if e!(*edge).is_sequence_flow() {
            recurse_maybe(
                callback,
                graph,
                current_node,
                &to!(*edge),
                EdgeType::SequenceFlow(*edge),
                path,
            );
        }
    }
    for LeftOf { left, right } in &graph.layout_constraints.left_of {
        if current_node.id == *left {
            recurse_maybe(
                callback,
                graph,
                current_node,
                &n!(*right),
                EdgeType::LeftOfConstraint,
                path,
            );
        }
    }
    for Above { above, below } in &graph.layout_constraints.above {
        if current_node.id == *above {
            recurse_maybe(
                callback,
                graph,
                current_node,
                &n!(*below),
                EdgeType::AboveConstraint { flipped: false },
                path,
            );
        }
        if current_node.id == *below {
            recurse_maybe(
                callback,
                graph,
                current_node,
                &n!(*above),
                EdgeType::AboveConstraint { flipped: true },
                path,
            );
        }
    }
    for SameLayer(node1, node2) in &graph.layout_constraints.same_layer {
        if current_node.id == *node1 {
            recurse_maybe(
                callback,
                graph,
                current_node,
                &n!(*node2),
                EdgeType::SameLayerConstraint,
                path,
            );
        }
        if current_node.id == *node2 {
            recurse_maybe(
                callback,
                graph,
                current_node,
                &n!(*node1),
                EdgeType::SameLayerConstraint,
                path,
            );
        }
    }
}

fn recurse_maybe(
    callback: &mut dyn FnMut(&[PathSegment]),
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

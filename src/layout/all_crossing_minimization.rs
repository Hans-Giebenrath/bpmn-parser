use crate::common::edge::DummyEdgeBendPoints;
use crate::common::edge::Edge;
use crate::common::edge::EdgeType;
use crate::common::graph::EdgeId;
use crate::common::graph::Place;
use crate::common::graph::PoolId;
use crate::common::graph::adjust_above_and_below_for_new_inbetween;
use crate::common::graph::{Graph, NodeId};
use crate::common::node::LayerId;
use crate::common::node::LoneDataElement;
use crate::common::node::Node;
use crate::common::node::NodePhaseAuxData;
use crate::common::node::NodeType;
use good_lp::*;
use itertools::Itertools;
use proc_macros::e;
use rustc_hash::FxBuildHasher;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::vec;

// XXX: Some remark for the ILP: The ILP should be parallelizable by solving one ILP per pool (not
// per lane!).
// Right now all is solved at once. So if there is some performance bottleneck here, try to
// decompose it. For one parallelism is good, but maybe the solver can solve a bit more efficiently
// when there are less useless constraints (mainly that nodes of one pool must be above nodes of
// another pool, for all pool combinations).

struct Vars {
    x_vars: HashMap<IlpNodeSiblingPair, Variable, FxBuildHasher>,
    c_vars: HashMap<Ijkl, Variable, FxBuildHasher>,
}

#[derive(Debug)]
pub struct CrossingMinimizationNodeData {
    ilp_node_id: IlpNodeId,
}

// TODO can the IlpXyz datatypes be moved into Node::aux?
#[derive(Copy, Clone, PartialEq, Hash, Eq, Debug)]
struct IlpNodeId(usize);
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
struct IlpEdge(IlpNodeId, IlpNodeId);

struct IlpGraph {
    layers: Vec<IlpLayer>,
    all_nodes: Vec<IlpNode>,
}

type GroupSize = usize;
struct IlpLayer {
    /// Nodes within one lane must not be mixed with nodes of another lane. So all nodes within one
    /// lane make up a group.
    ///
    /// Subprocesses are actually more tricky and are not solved with groups.
    groups: Vec<(IlpNodeId, GroupSize)>,
}

struct IlpNode {
    node_id: NodeId,
    outgoing: Vec<IlpNodeId>,
    /// For final sorting, counting how many times this node was below another node
    /// (x_{self, other} == 0). Then one can just sort the values by increasing scores
    /// to get the top-to-bottom correct order. Well, we actually don't sort but just write the
    /// value to `Node::node_below_in_same_lane` and Node::node_below_in_same_lane`.
    score: usize,
}

#[derive(Debug, Hash, Eq, PartialEq)]
struct Ijkl {
    ij: IlpEdge,
    kl: IlpEdge,
}
impl Ijkl {
    fn i(&self) -> IlpNodeId {
        self.ij.0
    }
    fn j(&self) -> IlpNodeId {
        self.ij.1
    }
    fn k(&self) -> IlpNodeId {
        self.kl.0
    }
    fn l(&self) -> IlpNodeId {
        self.kl.1
    }

    fn ki(&self) -> IlpNodeSiblingPair {
        IlpNodeSiblingPair(self.k(), self.i())
    }
    fn jl(&self) -> IlpNodeSiblingPair {
        IlpNodeSiblingPair(self.j(), self.l())
    }
    fn ik(&self) -> IlpNodeSiblingPair {
        IlpNodeSiblingPair(self.i(), self.k())
    }
    fn lj(&self) -> IlpNodeSiblingPair {
        IlpNodeSiblingPair(self.l(), self.j())
    }
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
struct IlpNodeSiblingPair(IlpNodeId, IlpNodeId);
impl IlpNodeSiblingPair {
    fn flip(&self) -> Self {
        Self(self.1, self.0)
    }
}

impl IlpLayer {
    fn iter_ijkl<'a>(&self, n: &'a [IlpNode]) -> impl Iterator<Item = Ijkl> + use<'a> {
        // i < j, k < l, i < k, j != l
        // This just says: Pairwise all edges which don't share one of the same endpoints.

        let all_nodes = self.iterate_node_ids_1();
        let end = all_nodes.end;
        all_nodes
            .flat_map(move |i| ((i + 1)..end).map(move |k| (i, k)))
            .flat_map(move |(i, k)| {
                n[i].outgoing
                    .iter()
                    .map(move |j| IlpEdge(IlpNodeId(i), *j))
                    .flat_map(move |ij| {
                        n[k].outgoing.iter().map(move |l| Ijkl {
                            ij,
                            kl: IlpEdge(IlpNodeId(k), *l),
                        })
                    })
            })
            .filter(|ijkl| ijkl.j() != ijkl.l())
    }

    fn iterate_node_ids_1(&self) -> std::ops::Range<usize> {
        if self.groups.is_empty() {
            0..0
        } else {
            let (IlpNodeId(last_group_start), group_size) = self.groups[self.groups.len() - 1];
            (self.groups[0].0.0)..(last_group_start + group_size)
        }
    }

    fn iterate_node_ids_2(&self) -> impl Iterator<Item = IlpNodeSiblingPair> {
        let r @ std::ops::Range { start: _, end } = self.iterate_node_ids_1();
        r.flat_map(move |i| {
            ((i + 1)..end).map(move |j| IlpNodeSiblingPair(IlpNodeId(i), IlpNodeId(j)))
        })
    }

    fn iterate_node_ids_3(&self) -> impl Iterator<Item = (IlpNodeId, IlpNodeId, IlpNodeId)> {
        let r @ std::ops::Range { start: _, end } = self.iterate_node_ids_1();
        r.flat_map(move |i| ((i + 1)..end).map(move |j| (i, j)))
            .flat_map(move |(i, j)| {
                ((j + 1)..end).map(move |k| (IlpNodeId(i), IlpNodeId(j), IlpNodeId(k)))
            })
    }
}

impl Vars {
    fn new(graph: &mut Graph) -> (ProblemVariables, Vars, IlpGraph) {
        let mut vars = variables![];
        let mut ilp_graph = IlpGraph {
            layers: vec![],
            all_nodes: vec![],
        };
        let mut v = Vars {
            x_vars: HashMap::with_hasher(FxBuildHasher),
            c_vars: HashMap::with_hasher(FxBuildHasher),
        };

        // For every horizontal group (lane, later maybe also standalone interrupt nodes),
        // there is an iterator going from the lower layer_id to the higher layer_id (this is an
        // invariant on `Lane::nodes`).
        // We will go layer_id by layer_id through all those iterators and advance them through
        // that layer_id (if present in the given horizontal group) and this way build up the
        // layers in the IlpGraph.
        let mut layer_iterators = vec![];

        for pool in &graph.pools {
            for lane in &pool.lanes {
                layer_iterators.push(lane.nodes.iter().cloned().peekable());
            }
        }

        // First construct the graph without edges.
        for layer_idx in 0..graph.num_layers {
            let mut ilp_layer = IlpLayer { groups: vec![] };
            for group in layer_iterators.iter_mut() {
                let mut ilp_group = None;
                loop {
                    let Some(next_node_in_group) =
                        group.next_if(|node_id| graph.nodes[node_id.0].layer_id.0 == layer_idx)
                    else {
                        break;
                    };
                    if graph.nodes[next_node_in_group.0].is_data_with_only_one_edge() {
                        // This one is added afterwards manually around the recipient.
                        continue;
                    }
                    let ilp_node_id = IlpNodeId(ilp_graph.all_nodes.len());
                    match ilp_group {
                        Some((_, ref mut group_size)) => *group_size += 1,
                        None => ilp_group = Some((ilp_node_id, 1)),
                    }
                    ilp_graph.all_nodes.push(IlpNode {
                        node_id: next_node_in_group,
                        outgoing: vec![],
                        score: 0,
                    });
                    graph.nodes[next_node_in_group.0].aux =
                        NodePhaseAuxData::CrossingMinimizationNodeData(
                            CrossingMinimizationNodeData { ilp_node_id },
                        );
                }
                if let Some(ilp_group) = ilp_group {
                    ilp_layer.groups.push(ilp_group);
                }
            }
            ilp_graph.layers.push(ilp_layer);
        }

        // Now create the connections between the nodes
        for node in &mut ilp_graph.all_nodes {
            for outgoing in graph.nodes[node.node_id.0]
                .outgoing
                .iter()
                .map(|edge_id| &graph.edges[edge_id.0])
                // Message flows are handled directly in the minimisation objective.
                .filter(|edge| !Edge::is_message_flow(edge))
                .flat_map(|edge| aux(&graph.nodes[edge.to.0]))
            {
                node.outgoing.push(outgoing);
            }
        }

        // Create all the ILP variables.
        for layer in &ilp_graph.layers {
            for ij in layer.iterate_node_ids_2() {
                v.x_vars.insert(ij, vars.add(variable().binary()));
            }

            for ijkl in layer.iter_ijkl(&ilp_graph.all_nodes) {
                v.c_vars.insert(ijkl, vars.add(variable().binary()));
            }
        }

        // debug_print_graph(&v, &ilp_graph, graph);
        (vars, v, ilp_graph)
    }

    #[track_caller]
    fn x_orig(&mut self, node_idces: IlpNodeSiblingPair) -> Variable {
        match self.x_vars.get(&node_idces) {
            Some(v) => *v,
            None => panic!(
                "should have been prepared previously for ilpn({})-ilpn({})",
                node_idces.0.0, node_idces.1.0
            ),
        }
    }

    // `x_ij == 1` means that `i` is above `j`, 0 otherwise.
    // This automatically transforms `x_ji` into `(1 - x_ij)`, as described in switch 4 of
    // https://ieeevis.b-cdn.net/vis_2024/pdfs/v-full-1874.pdf
    // The extended version would be to have both x variables, and then an additional constraint
    // would be necessary (x_ij + x_ji = 1).
    #[track_caller]
    fn x(&mut self, ij: IlpNodeSiblingPair) -> Expression {
        if ij.0.0 < ij.1.0 {
            self.x_orig(ij).into_expression()
        } else {
            1 - self.x_orig(ij.flip())
        }
    }

    #[track_caller]
    fn c(&mut self, ijkl: &Ijkl) -> Variable {
        match self.c_vars.get(ijkl) {
            Some(v) => *v,
            None => panic!(
                "should have been prepared previously for ilpn({})-ilpn({})-ilpn({})-ilpn({})",
                ijkl.i().0,
                ijkl.j().0,
                ijkl.k().0,
                ijkl.l().0
            ),
        }
    }
}

pub fn reduce_all_crossings(graph: &mut Graph) {
    let undo = temporarily_add_dummy_nodes_for_edges_within_same_layer(graph);
    let (vars, mut v, mut g) = Vars::new(graph);
    let v = &mut v;
    let mut objective = Expression::from(0.0);
    for layer in &g.layers {
        layer.iter_ijkl(&g.all_nodes).for_each(|ijkl| {
            objective += v.c(&ijkl);
        });
    }

    handle_message_flows(graph, v, &mut objective);

    let mut problem = vars.minimise(objective).using(default_solver);
    //problem.set_parameter("loglevel", "0");

    for layer in &g.layers {
        layer.iter_ijkl(&g.all_nodes).for_each(|ijkl| {
            // Crossing constraints taken from https://ieeevis.b-cdn.net/vis_2024/pdfs/v-full-1874.pdf
            // c(i,k),( j,l) +x j,i +xk,l ≥ 1
            // c(i,k),( j,l) +xi, j +xl,k ≥ 1
            // Indexes fixed to the older paper version:
            // c(i,j),( k,l) +x k,i +xj,l ≥ 1
            // c(i,j),( k,l) +xi, k +xl,j ≥ 1
            problem.add_constraint((v.c(&ijkl) + v.x(ijkl.ki()) + v.x(ijkl.jl())).geq(1));
            problem.add_constraint((v.c(&ijkl) + v.x(ijkl.ik()) + v.x(ijkl.lj())).geq(1));
        });
        layer.iterate_node_ids_3().for_each(|(i, j, k)| {
            let ij = IlpNodeSiblingPair(i, j);
            let jk = IlpNodeSiblingPair(j, k);
            let ik = IlpNodeSiblingPair(i, k);
            problem.add_constraint((v.x(ij) + v.x(jk) - v.x(ik)).leq(1));
            problem.add_constraint((v.x(ij) + v.x(jk) - v.x(ik)).geq(0));
        });

        // Now we need to ensure that the groups stay together as a group and are not mixed.
        (0..layer.groups.len())
            .flat_map(|g1| ((g1 + 1)..layer.groups.len()).map(move |g2| (g1, g2)))
            .flat_map(|(g1, g2)| {
                let (IlpNodeId(g1start), g1len) = layer.groups[g1];
                let (IlpNodeId(g2start), g2len) = layer.groups[g2];
                (g1start..g1start + g1len).flat_map(move |n1| {
                    (g2start..g2start + g2len)
                        .map(move |n2| IlpNodeSiblingPair(IlpNodeId(n1), IlpNodeId(n2)))
                })
            })
            .for_each(|ij| {
                problem.add_constraint(v.x(ij).eq(1));
                // redundant, since x_ji is expressed as `1 - x_ij`
                // problem.add_constraint(v.x(ij.flip()).eq(0));
            });
    }

    let solution = problem.solve().unwrap();
    struct SortNode {
        node_id: NodeId,
        score: usize,
    }
    let mut sort_buffer = Vec::<SortNode>::new();
    for layer in &g.layers {
        layer.iterate_node_ids_2().for_each(|ij| {
            let IlpNodeSiblingPair(i, j) = ij;
            if solution.value(v.x_orig(ij)) == 1.0 {
                g.all_nodes[j.0].score += 1;
            } else {
                g.all_nodes[i.0].score += 1;
            }
        });
        {
            let scores: Vec<_> = layer
                .iterate_node_ids_1()
                .map(|i| g.all_nodes[i].score)
                .collect();
            let hs = scores.iter().collect::<std::collections::HashSet<_>>();
            assert!(scores.len() == hs.len());
        }

        // Assign the neighbor information to the nodes.
        for (start_ilp_node_id, size) in &layer.groups {
            sort_buffer.clear();
            for ilp_node_id in start_ilp_node_id.0..start_ilp_node_id.0 + size {
                sort_buffer.push(SortNode {
                    node_id: g.all_nodes[ilp_node_id].node_id,
                    score: g.all_nodes[ilp_node_id].score,
                });
            }
            sort_buffer.sort_unstable_by(|a, b| a.score.cmp(&b.score));
            sort_buffer.array_windows().for_each(|[above, below]| {
                graph.nodes[above.node_id].node_below_in_same_lane = Some(below.node_id);
                graph.nodes[below.node_id].node_above_in_same_lane = Some(above.node_id);
            });
        }
    }

    process_data_nodes_which_should_become_immediate_neighbors(graph);
    remove_temporarily_added_dummy_nodes_for_edges_within_same_layer(graph, undo);
    sort_incoming_and_outgoing(graph);
}

fn aux(node: &Node) -> Option<IlpNodeId> {
    match node.aux {
        NodePhaseAuxData::CrossingMinimizationNodeData(CrossingMinimizationNodeData {
            ilp_node_id: res,
        }) => Some(res),
        _ => None,
    }
}

fn handle_message_flows(graph: &Graph, v: &mut Vars, objective: &mut Expression) {
    // BPMN extension: minimise crossings introduced by message flows: Instead of modelling message
    // flow crossings as normal edges and normal crossings, we instead add a penalty as high as the
    // additional edge crossing count if a message flow-connected node is put farther away from the
    // top/bottom (wherever the message flow is leaving), as each swap would mean more crossings.
    // This is possible since the message flow part in the middle of two pools is kinda like a
    // fixed-position node, so we don't need to solve for that node's position within.

    #[derive(Debug)]
    enum AboveOrBelow {
        Above,
        Below,
        SolveViaIlp,
        DoesntMatter,
    }

    // Tweaked by chatGPT + errors corrected by me
    fn compare_for_message_flow(
        this_to: &Node,
        other_to: &Node,
        from_pool: PoolId,
        from_layer_id: LayerId,
    ) -> AboveOrBelow {
        assert_ne!(this_to.id, other_to.id);

        let this_up = this_to.pool < from_pool;
        let other_up = other_to.pool < from_pool;

        if this_up && !other_up {
            return AboveOrBelow::Above;
        }
        if !this_up && other_up {
            return AboveOrBelow::Below;
        }

        let this_right = this_to.layer_id >= from_layer_id;
        let other_right = other_to.layer_id >= from_layer_id;
        let this_left = this_to.layer_id <= from_layer_id;
        let other_left = other_to.layer_id <= from_layer_id;

        let invert = match (this_up, this_right, other_right, this_left, other_left) {
            (true, true, true, _, _) => false,  // both up-right
            (true, _, _, true, true) => true,   // both up-left
            (false, true, true, _, _) => true,  // both down-right
            (false, _, _, true, true) => false, // both down-left
            _ => return AboveOrBelow::DoesntMatter,
        };

        match (this_to.layer_id, this_to.pool, this_to.lane).cmp(&(
            other_to.layer_id,
            other_to.pool,
            other_to.lane,
        )) {
            Ordering::Less => {
                if invert {
                    AboveOrBelow::Below
                } else {
                    AboveOrBelow::Above
                }
            }
            Ordering::Greater => {
                if invert {
                    AboveOrBelow::Above
                } else {
                    AboveOrBelow::Below
                }
            }
            Ordering::Equal => AboveOrBelow::SolveViaIlp,
        }
    }

    fn analyse_node(
        the_node: &Node,
        edge_under_iteration: EdgeId,
        remote_node: &Node,
        graph: &Graph,
        v: &mut Vars,
        objective: &mut Expression,
    ) {
        for neighbor_node in graph.pools[the_node.pool].lanes[the_node.lane]
            .nodes
            .iter()
            .map(|node_id| &graph.nodes[*node_id])
            // They are sorted by layer_id
            .skip_while(|n| n.layer_id != the_node.layer_id)
            .take_while(|n| n.layer_id == the_node.layer_id)
            .filter(|n| n.id != the_node.id)
        {
            let Some(neighbor_node_ilp) = aux(neighbor_node) else {
                continue;
            };
            let the_node_ilp = aux(the_node).unwrap();
            let goes_up = the_node.pool > remote_node.pool;
            // TODO this does result in douple-penalty count for directly up or down connections, but
            // this is OK actually, because those we really want to be at the edge so the flow can be straight.
            let goes_right = the_node.layer_id >= remote_node.layer_id;
            let goes_left = the_node.layer_id <= remote_node.layer_id;

            #[rustfmt::skip]
            assert_eq!(
                (the_node.pool, the_node.lane, the_node.layer_id),
                (neighbor_node.pool, neighbor_node.lane, neighbor_node.layer_id)
            );
            for other_remote_message_flow_node in neighbor_node
                .outgoing
                .iter()
                .filter(|outgoing| edge_under_iteration.0 < outgoing.0)
                .map(|edge_id| &graph.edges[*edge_id])
                .filter(|e| Edge::is_message_flow(e))
                .map(|e| &graph.nodes[e.to])
                .chain(
                    neighbor_node
                        .incoming
                        .iter()
                        .filter(|incoming| edge_under_iteration.0 < incoming.0)
                        .map(|edge_id| &graph.edges[*edge_id])
                        .filter(|e| Edge::is_message_flow(e))
                        .map(|e| &graph.nodes[e.from]),
                )
            {
                match compare_for_message_flow(
                    remote_node,
                    other_remote_message_flow_node,
                    the_node.pool,
                    the_node.layer_id,
                ) {
                    AboveOrBelow::Above => {
                        *objective += v.x(IlpNodeSiblingPair(neighbor_node_ilp, the_node_ilp))
                    }
                    AboveOrBelow::Below => {
                        *objective += v.x(IlpNodeSiblingPair(the_node_ilp, neighbor_node_ilp))
                    }
                    AboveOrBelow::SolveViaIlp => {
                        dbg!("TODO");
                    }
                    AboveOrBelow::DoesntMatter => (),
                }
            }

            let mut other_crossed_edges = 0;
            // Note: a message flow going directly up or down (same layer_id) is marked as both
            // `goes_right` and `goes_left`, so is basically weighted twice. This is intentional,
            // since for visual purposes we *really* would like to have them at the border so we
            // could draw a straight line.
            if goes_right {
                other_crossed_edges += the_node
                    .outgoing
                    .iter()
                    .map(|edge_id| &graph.edges[*edge_id])
                    .filter(|e| Edge::is_message_flow(e))
                    .count();
            }
            if goes_left {
                other_crossed_edges += the_node
                    .incoming
                    .iter()
                    .map(|edge_id| &graph.edges[*edge_id])
                    .filter(|e| Edge::is_message_flow(e))
                    .count();
            }
            // It might be that there are no edges to the given direction on other nodes. In that
            // case we still want `the_node` to go to the border to reduce the arrow length (i.e.
            // we need a value >0), but only if this does not introduce unnecessary crossings
            // (i.e. <1). The chosen value is just arbitrary within (0, 1).
            let nudge = 0.1;
            if goes_up {
                *objective += (other_crossed_edges as f64).max(nudge)
                    * v.x(IlpNodeSiblingPair(neighbor_node_ilp, the_node_ilp));
            } else {
                *objective += (other_crossed_edges as f64).max(nudge)
                    * v.x(IlpNodeSiblingPair(the_node_ilp, neighbor_node_ilp));
            }
        }
    }

    for (edge_idx, edge) in graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| Edge::is_message_flow(e))
    {
        let from_node = &graph.nodes[edge.from];
        let to_node = &graph.nodes[edge.to];

        analyse_node(from_node, EdgeId(edge_idx), to_node, graph, v, objective);
        analyse_node(to_node, EdgeId(edge_idx), from_node, graph, v, objective);
    }
}

/// TODO the images are wrong, make a BPMN from this to correct it.
/// Sort order images only show the upper part, but the lower part is equal, just mirrored.
///
/// Sort order of incoming nodes:
///   NB: Horizontal part is closest to the `edge.from` node's pool.
///
///              ┌────┐
///  ┌┐          │    ┼─┐
///  │┼──┐       └────┘ │
///  └┘  │              │      (Pool 1)                  ┌──┐    ┌─┐
///      │              │                            ┌───┼  │ ┌──┼ │
///      │              │                            │   └──┘ │  └─┘
///      │              └─────┐ ┌────────────────────┘        │
///      └──────────────────┐ │ │ ┌───────────────────────────┘
///  ┌┐          ┌─────┐    │ │ │ │                       ┌┐
///  │┼─┐        │     ┼──┐ │ │ │ │   (Pool 2)         ┌──┼│   ┌┐
///  └┘ │        └─────┘  │ │ │ │ │                    │  └┘ ┌─┼│
///     │                 │ │ │ │ │ ┌──────────────────┘     │ └┘
///     │                 │ │ │ │ │ │ ┌──────────────────────┘
///     │                 │ │ │ │ │ │ │  ┌───────┐
///     │                 │ │ │ │ │ │ └──►       │
///     │                 │ │ │ │ │ └────►       │
///     │                 │ │ │ │ └──────►       │
///     │    (Pool 3)     │ │ │ └────────►       │
///     │                 │ │ └──────────►       │
///     │                 │ └────────────►       │
///     │                 └──────────────►       │
///     └────────────────────────────────►       │
///                                      │       │
///                                      └───────┘
///
///
///
/// Sort order of outgoing nodes:
///   NB: Horizontal part is closest to the `edge.from` node's pool.
///
/// ┌────┐   ┌────┐   ┌────┐                    ┌────┐      ┌────┐
/// │    ◄──┐│    ◄─┐ │    ◄─┐   (Pool 1)   ┌───►    │  ┌──►│    │
/// └────┘  │└────┘ │ └────┘ │              │   └────┘  │   └────┘
///         │       │        │              │           │
///         │       │        │              │           │
///         │       │        │              │           │
/// ┌────┐  │       │        │              │   ┌────┐  │   ┌────┐
/// │    ◄─┐        │        │   (Pool 2)   │┌──►    │  │┌──►    │
/// └────┘ ││       │        └────┐ ┌───────┘│  └────┘  ││  └────┘
///        ││       └───────────┐ │ │ ┌──────┘          ││
///        │└──────────────────┐│ │ │ │┌────────────────┘│
///        └─────────────────┐ ││ │ │ ││ ┌───────────────┘
///                   ┌────┐ │ ││ │ │ ││ │
///                   │    │─┘ ││ │ │ ││ │
///                   │    │───┘│ │ │ ││ │
///                   │    │────┘ │ │ ││ │
///                   │    │──────┘ │ ││ │
///                   │    │────────┼►││ │   (Pool 3)
///                   │    │────────┴─┘│ │
///                   │    │───────────┘ │
///                   │    │─────────────┘
///                   │    │
///                   └────┘
///
///
fn sort_incoming_and_outgoing(graph: &mut Graph) {
    // Sorting is done a bit inefficiently (n^2), feel free to improve this if it becomes a
    // bottleneck.
    // The strategy is to count how far we can go up in the `.node_above_in_same_lane` linked list.
    // The farther we can go up, the later it should come in the `.incoming`/`.outgoing` vec.
    fn rank_within_lane(node: &Node, graph: &Graph) -> usize {
        let mut rank_within_lane = 0;
        let mut node = node; // for the borrow checker, lol
        while let Some(above) = node.node_above_in_same_lane {
            rank_within_lane += 1;
            node = &graph.nodes[above];
        }
        rank_within_lane
    }
    for node_id in (0..graph.nodes.len()).map(NodeId) {
        let node = &mut graph.nodes[node_id];
        if node.incoming.len() > 1 {
            let mut incoming_cpy = std::mem::take(&mut node.incoming);
            incoming_cpy.sort_by_cached_key(|edge_id| {
                let to_node = &graph.nodes[graph.edges[*edge_id].to];
                let from_node = &graph.nodes[graph.edges[*edge_id].from];
                let from_rank_within_lane = rank_within_lane(from_node, graph);
                let to_rank_within_lane = rank_within_lane(to_node, graph);

                if to_node.pool == from_node.pool
                    && to_node.layer_id == from_node.layer_id
                    && from_rank_within_lane < to_rank_within_lane
                {
                    // Looping downwards (toward this node) in the same pool
                    let group_order = 0;
                    (
                        group_order,
                        -(from_node.pool.0 as isize),
                        LayerId(0),
                        -(from_node.lane.0 as isize),
                        -(from_rank_within_lane as isize),
                    )
                } else if to_node.pool == from_node.pool
                    && to_node.layer_id == from_node.layer_id
                    && from_rank_within_lane > to_rank_within_lane
                {
                    // Looping upwards (towards this node) in the same pool
                    let group_order = 2;
                    (
                        group_order,
                        -(from_node.pool.0 as isize),
                        LayerId(0),
                        -(from_node.lane.0 as isize),
                        -(from_rank_within_lane as isize),
                    )
                } else {
                    assert!(
                        to_node.pool != from_node.pool
                            || from_node.layer_id.0 + 1 == to_node.layer_id.0,
                        "{from_node:#?}, {to_node:#?}"
                    );
                    // Some upper pool, lower pool, or going right in the same pool.
                    let group_order = 1;
                    (
                        group_order,
                        (from_node.pool.0 as isize),
                        from_node.layer_id,
                        (from_node.lane.0 as isize),
                        (from_rank_within_lane as isize),
                    )
                }
            });
            graph.nodes[node_id].incoming = incoming_cpy;
        }

        let node = &mut graph.nodes[node_id];
        if node.outgoing.len() > 1 {
            let mut outgoing_cpy = std::mem::take(&mut node.outgoing);
            outgoing_cpy.sort_by_cached_key(|edge_id| {
                let from_node = &graph.nodes[graph.edges[*edge_id].from];
                let to_node = &graph.nodes[graph.edges[*edge_id].to];
                let to_rank_within_lane = rank_within_lane(to_node, graph);
                let from_rank_within_lane = rank_within_lane(from_node, graph);

                if from_node.pool == to_node.pool
                    && from_node.layer_id == to_node.layer_id
                    && to_rank_within_lane < from_rank_within_lane
                {
                    // Looping upwards in the same pool
                    let group_order = 0;
                    (
                        group_order,
                        LayerId(0),
                        -(to_node.pool.0 as isize),
                        -(to_node.lane.0 as isize),
                        -(to_rank_within_lane as isize),
                    )
                } else if from_node.pool == to_node.pool
                    && from_node.layer_id == to_node.layer_id
                    && to_rank_within_lane > from_rank_within_lane
                {
                    // Looping upwards in the same pool
                    let group_order = 2;
                    (
                        group_order,
                        LayerId(0),
                        -(to_node.pool.0 as isize),
                        -(to_node.lane.0 as isize),
                        -(to_rank_within_lane as isize),
                    )
                } else {
                    assert!(
                        from_node.pool != to_node.pool
                            || from_node.layer_id.0 + 1 == to_node.layer_id.0,
                        "{from_node:?}, {to_node:?}"
                    );
                    // Some upper pool, lower pool, or going right in the same pool.
                    let group_order = 1;
                    (
                        group_order,
                        to_node.layer_id,
                        (to_node.pool.0 as isize),
                        (to_node.lane.0 as isize),
                        (to_rank_within_lane as isize),
                    )
                }
            });
            graph.nodes[node_id].outgoing = outgoing_cpy;
        }
    }
}

/// Process the data nodes which shall be placed as a neighbor of their sole connected node. They
/// are treated specially to avoid adding useless nodes to the ILP (also, formulating the need to
/// place a node exactly next to another node is a bit involved, so doing it by hand should be
/// faster and easier to understand).
fn process_data_nodes_which_should_become_immediate_neighbors(graph: &mut Graph) {
    for data_node_id in (0..graph.nodes.len()).map(NodeId) {
        let node = &graph.nodes[data_node_id];
        let place = match node.data_with_only_one_edge() {
            LoneDataElement::IsOutput(edge_id) => Place::Below(e!(edge_id).from),
            LoneDataElement::IsInput(edge_id) => Place::Above(e!(edge_id).to),
            LoneDataElement::Nope => continue,
        };

        adjust_above_and_below_for_new_inbetween(data_node_id, place, graph);
    }
}

struct Undo {
    original_num_nodes: usize,
    original_edges: Vec<(EdgeId, Edge)>,
}

fn temporarily_add_dummy_nodes_for_edges_within_same_layer(graph: &mut Graph) -> Undo {
    let original_edge_count = graph.edges.len();
    let mut undo = Undo {
        original_num_nodes: graph.nodes.len(),
        original_edges: Vec::new(),
    };

    for edge_id in (0..original_edge_count).map(EdgeId) {
        let edge = &mut graph.edges[edge_id];
        let from = &graph.nodes[edge.from];
        let to = &graph.nodes[edge.to];
        if from.layer_id != to.layer_id {
            continue;
        }
        if from.pool != to.pool {
            dbg!(&"TODO");
            continue;
        }
        if from.is_data_with_only_one_edge() || to.is_data_with_only_one_edge() {
            // These are processed specially.
            continue;
        }
        let layer_id = from.layer_id;
        let pool_and_lane = from.pool_and_lane();

        undo.original_edges.push((edge_id, edge.clone()));
        let right_node_id = graph.add_node(
            NodeType::LongEdgeDummy,
            pool_and_lane,
            Some(LayerId(layer_id.0 + 1)),
        );
        let left_node_id = if layer_id.0 > 0 {
            Some(graph.add_node(
                NodeType::LongEdgeDummy,
                pool_and_lane,
                Some(LayerId(layer_id.0 - 1)),
            ))
        } else {
            // No room to the left. In that case we anyway don't gain anything from having
            // additional dummy nodes there, since it wouldn't cross with anything (or at least it
            // would not be meaningful I think).
            None
        };
        reroute_vertical_edge(graph, edge_id, left_node_id, right_node_id);
    }

    undo
}

fn reroute_vertical_edge(
    graph: &mut Graph,
    to_be_rerouted_edge_id: EdgeId,
    left_intermediate_node_id: Option<NodeId>,
    right_intermediate_node_id: NodeId,
) {
    let edge = &mut graph.edges[to_be_rerouted_edge_id];
    let from_node_id = edge.from;
    let to_node_id = std::mem::replace(&mut edge.to, right_intermediate_node_id);
    let flow_type = edge.flow_type.clone();
    // These value are not used within this phase, so just put something random here.
    let is_reversed = false;
    let stays_within_lane = false;

    let right_second_edge_id = EdgeId(graph.edges.len());
    graph.edges.push(Edge {
        from: to_node_id,
        to: right_intermediate_node_id,
        edge_type: EdgeType::DummyEdge {
            original_edge: to_be_rerouted_edge_id,
            bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
        },
        flow_type: flow_type.clone(),
        is_reversed,
        stays_within_lane,
        stroke_color: None,
        is_vertical: false,
    });

    let to_node = &mut graph.nodes[to_node_id];
    let Some(i) = to_node
        .incoming
        .iter()
        .position(|i| *i == to_be_rerouted_edge_id)
    else {
        unreachable!();
    };
    to_node.incoming.remove(i);
    to_node.outgoing.push(right_second_edge_id);

    let right_intermediate_node = &mut graph.nodes[right_intermediate_node_id];
    right_intermediate_node
        .incoming
        .push(to_be_rerouted_edge_id);
    right_intermediate_node.incoming.push(right_second_edge_id);

    if let Some(left_intermediate_node_id) = left_intermediate_node_id {
        let left_first_edge_id = EdgeId(graph.edges.len());
        graph.edges.push(Edge {
            from: left_intermediate_node_id,
            to: from_node_id,
            edge_type: EdgeType::DummyEdge {
                original_edge: to_be_rerouted_edge_id,
                bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
            },
            flow_type: flow_type.clone(),
            is_reversed,
            stays_within_lane,
            stroke_color: None,
            is_vertical: false,
        });

        let left_second_edge_id = EdgeId(graph.edges.len());
        graph.edges.push(Edge {
            from: left_intermediate_node_id,
            to: to_node_id,
            edge_type: EdgeType::DummyEdge {
                original_edge: to_be_rerouted_edge_id,
                bend_points: DummyEdgeBendPoints::ToBeDeterminedOrStraight,
            },
            flow_type,
            is_reversed,
            stays_within_lane,
            stroke_color: None,
            is_vertical: false,
        });

        let left_intermediate_node = &mut graph.nodes[left_intermediate_node_id];
        left_intermediate_node.outgoing.push(left_first_edge_id);
        left_intermediate_node.outgoing.push(left_second_edge_id);

        graph.nodes[from_node_id].incoming.push(left_first_edge_id);
        graph.nodes[to_node_id].incoming.push(left_second_edge_id);
    }
}

fn remove_temporarily_added_dummy_nodes_for_edges_within_same_layer(graph: &mut Graph, undo: Undo) {
    // also fix the `node_below_in_same_lane` etc properties
    let mut num_edges_to_retain: usize = graph.edges.len() - undo.original_edges.len();
    let num_nodes_to_retain: usize = undo.original_num_nodes;
    for pool in &mut graph.pools {
        for lane in &mut pool.lanes {
            lane.nodes.retain(|node_id| node_id.0 < num_nodes_to_retain);
        }
    }

    for (rerouted_edge_id, original_edge_value) in undo.original_edges {
        // I have a feeling that this can be written more cleanly, need to ask an AI.
        let to_node_id = original_edge_value.to;
        let from_node_id = original_edge_value.from;
        graph.edges[rerouted_edge_id] = original_edge_value;
        // XXX don't move this down, as there is a `to_node.incoming.push(.)`.
        if graph.nodes[to_node_id].layer_id.0 > 0 {
            graph.nodes[to_node_id].incoming.pop();
            graph.nodes[from_node_id].incoming.pop();
            num_edges_to_retain -= 2;
        }
        let to_node = &mut graph.nodes[to_node_id];
        to_node.outgoing.pop();
        to_node.incoming.push(rerouted_edge_id);
    }

    for node_id in (num_nodes_to_retain..graph.nodes.len()).map(NodeId) {
        let intermediate_node = &graph.nodes[node_id];
        let node_above_in_same_lane = intermediate_node.node_above_in_same_lane;
        let node_below_in_same_lane = intermediate_node.node_below_in_same_lane;
        match (node_above_in_same_lane, node_below_in_same_lane) {
            (Some(above), Some(below)) => {
                let node_above = &graph.nodes[above];
                let node_below = &graph.nodes[below];
                if node_above.pool == node_below.pool && node_above.lane == node_below.lane {
                    graph.nodes[above].node_below_in_same_lane = node_below_in_same_lane;
                    graph.nodes[below].node_above_in_same_lane = node_above_in_same_lane;
                } else {
                    graph.nodes[above].node_below_in_same_lane = None;
                    graph.nodes[below].node_above_in_same_lane = None;
                }
            }
            (Some(above), None) => {
                graph.nodes[above].node_below_in_same_lane = None;
            }
            (None, Some(below)) => {
                graph.nodes[below].node_above_in_same_lane = None;
            }
            (None, None) => (),
        }
    }

    graph.nodes.truncate(num_nodes_to_retain);
    graph.edges.truncate(num_edges_to_retain);
}

// Only necessary for debugging.
#[allow(dead_code)]
fn debug_print_graph(v: &Vars, ig: &IlpGraph, g: &Graph) {
    let node_str = |n: &Node| -> String {
        let aux = match aux(n) {
            Some(IlpNodeId(idx)) => idx.to_string(),
            None => "-".to_string(),
        };
        let layer = n.layer_id.0;
        let what = match &n.node_type {
            NodeType::RealNode { display_text, .. } => display_text.as_str(),
            NodeType::LongEdgeDummy => "(dummy node)",
            NodeType::BendDummy { .. } => "(bend dummy - IMPOSSIBLE HERE?)",
        };
        format!("nid({}) {what} ilpn({aux}) - Lyr({layer})", n.id)
    };

    println!("Nodes:");
    for n in &g.nodes {
        println!("{}", node_str(n));
    }

    println!();
    println!("Edges:");
    for (edge_idx, e) in g.edges.iter().enumerate() {
        let flow_type = match e.flow_type {
            crate::common::edge::FlowType::MessageFlow(_) => "MF",
            crate::common::edge::FlowType::DataFlow(_) => "DF",
            crate::common::edge::FlowType::SequenceFlow => "SF",
        };

        match &e.edge_type {
            EdgeType::Regular { text, .. } => {
                println!(
                    "{} --[{flow_type} eid({edge_idx}) {}]--> {}",
                    node_str(&g.nodes[e.from]),
                    text.as_ref().map(|s| s.as_str()).unwrap_or(""),
                    node_str(&g.nodes[e.to])
                );
            }
            EdgeType::ReplacedByDummies { text, .. } => {
                println!(
                    "(replaced by dummies {} --[{flow_type} eid({edge_idx}) {}]--> {})",
                    node_str(&g.nodes[e.from]),
                    text.as_ref().map(|s| s.as_str()).unwrap_or(""),
                    node_str(&g.nodes[e.to])
                );
            }
            EdgeType::DummyEdge { original_edge, .. } => {
                println!(
                    "{} --[{flow_type} eid({edge_idx}), orig: {}]--> {})",
                    node_str(&g.nodes[e.from]),
                    original_edge.0,
                    node_str(&g.nodes[e.to])
                );
            }
        }
    }

    println!();
    println!("X Vars:");
    for pair in v.x_vars.keys() {
        println!("X var ilpn({}) - ilpn({})", pair.0.0, pair.1.0);
    }

    println!();
    println!("C Vars:");
    for ijkl in v.c_vars.keys() {
        println!(
            "C var ilpn({}) - ilpn({}) - ilpn({}) - ilpn({})",
            ijkl.i().0,
            ijkl.j().0,
            ijkl.k().0,
            ijkl.l().0
        );
    }

    println!();
    println!("Ilp Layers:");
    for (layer_idx, layer) in ig.layers.iter().enumerate() {
        println!("IlpLayer {layer_idx}:");
        for (start, group_size) in &layer.groups {
            println!("    group {}", (start.0..start.0 + group_size).join(" "));
        }
        println!(
            "    iterate_node_ids_1: {}",
            layer
                .iterate_node_ids_1()
                .map(|id| format!("({id})"))
                .join(" ")
        );
        println!(
            "    iterate_node_ids_2: {}",
            layer
                .iterate_node_ids_2()
                .map(|pair| format!("({} {})", pair.0.0, pair.1.0))
                .join(" ")
        );
        println!(
            "    iterate_node_ids_3: {}",
            layer
                .iterate_node_ids_3()
                .map(|pair| format!("({} {} {})", pair.0.0, pair.1.0, pair.2.0))
                .join(" ")
        );
        println!(
            "    iter_ijkl: {}",
            layer
                .iter_ijkl(&ig.all_nodes)
                .map(|ijkl| format!(
                    "({} {} {} {})",
                    ijkl.i().0,
                    ijkl.j().0,
                    ijkl.k().0,
                    ijkl.l().0
                ))
                .join(" ")
        );
    }
}

use crate::common::bpmn_node;
use crate::common::bpmn_node::BpmnNode;
use crate::common::bpmn_node::EventVisual;
use crate::common::bpmn_node::InterruptingKind;
use crate::common::edge::{DataFlowAux, EdgeType, FlowType};
use crate::common::edge::{MessageFlowAux, RegularEdgeBendPoints};
use crate::common::graph::{Graph, SdeId};
use crate::common::graph::{LaneId, NodeId, PoolId};
use crate::common::node::NodeType;
use crate::lexer::ActivityMeta;
use crate::lexer::DataMeta;
use crate::lexer::Direction;
use crate::lexer::EdgeMeta;
use crate::lexer::EventType;
use crate::lexer::GatewayInnerMeta;
use crate::lexer::GatewayNodeMeta;
use crate::lexer::SequenceFlowMeta;
use crate::lexer::lex;
use crate::lexer::{self, MessageFlowMeta};
use crate::lexer::{DataAux, DataFlowMeta};
use crate::lexer::{Statement, StatementStream, TokenCoordinate};
use crate::node_id_matcher::NodeIdMatcher;
use crate::pool_id_matcher::PoolIdMatcher;
use std::collections::HashMap;
use std::collections::HashSet;
use std::dbg;

use annotate_snippets::Level;
use annotate_snippets::Snippet;
use annotate_snippets::renderer::Renderer;

pub struct ParseContext {
    last_node_id: Option<usize>,
    grouping_state: GroupingState,
    lifeline_state: LifelineState,
    pub current_token_coordinate: TokenCoordinate,
    /// Sequence flows are always within a pool, so the identifiers are scoped per PoolId
    /// through this HashMap.
    dangling_sequence_flows: HashMap<PoolId, DanglingSequenceFlows>,
    // Shorthand blank events maybe need to be transformed into message events, such that they are
    // allowed to send or receive data. This automatic transformation is done to allow quickly
    // prototyping of the BPMD file using shorthand syntax in a first iteration.
    shorthand_blank_events: HashSet<NodeId>,
    node_id_matcher: NodeIdMatcher,
    pub pool_id_matcher: PoolIdMatcher,
}

// X ->l1 ; l1: - task; J end; X<-end
#[derive(Debug)]
enum LifelineState {
    /// At the beginning of the text file, a pool or lane, and after a terminal
    /// node.
    NoLifelineActive {
        // None if it is the beginning of the text file.
        previous_lifeline_termination_statement: Option<TokenCoordinate>,
    },
    /// When a branch target was encountered.
    StartedFromSequenceFlowLanding {
        name: String,
        token_coordinate: TokenCoordinate,
    },
    /// During a chain of tasks/events.
    ActiveLifeline {
        last_node_id: NodeId,
        token_coordinate: TokenCoordinate,
    },
}

// Later this might also track deeper nesting from other special constructs.
#[derive(Debug)]
enum GroupingState {
    Init,
    /// In this case some node was parsed while we were in the Init state, i.e. no previous Pool
    /// expression was parsed. This means that the rendered image shall not contain a rendered pool
    /// at all (this is for the tiny examples). But also, any later encountered pool or lane
    /// statement are errors. The `first_encountered_node` is stored for better error reporting.
    WithinAnonymousPool {
        pool_id: PoolId,
        lane_id: LaneId,
        first_encountered_node: TokenCoordinate,
    },
    /// Just parsed a pool, all good an regular.
    WithinPool {
        pool_id: PoolId,
        ptc: TokenCoordinate,
    },
    /// When a pool was parsed and then some node, the lane is implicitly added in our code for a
    /// unified structure, but since it does not have a name nor a location (TokenCoordinate) in
    /// the input text file, it is considered anonymous.
    /// Invariant: No explicit lane is allowed in this pool going forward.
    WithinAnonymousLane {
        pool_id: PoolId,
        ptc: TokenCoordinate,
        lane_id: LaneId,
        first_encountered_node: TokenCoordinate,
    },
    /// We are in a lane, all good and regular.
    WithinLane {
        pool_id: PoolId,
        ptc: TokenCoordinate,
        lane_id: LaneId,
        ltc: TokenCoordinate,
    },
}

#[derive(Debug)]
struct DanglingEdgeInfo {
    known_node_id: NodeId,
    // Might be empty
    edge_text: String,
    tc: TokenCoordinate,
}

/// I'd somehow like to impose some restrictions here, but even for an n:m relationship there is a
/// good reason (some gateway criss-crossing). So better analyse the resulting graph and complain
/// if invalid connection were created and then let the user fix up their mess.
#[derive(Default)]
struct DanglingSequenceFlows {
    /// An edge whose start is known but end is unknown: `->label`
    dangling_end_map: HashMap<String, Vec<DanglingEdgeInfo>>,
    /// An edge whose end is known but start is unknown: `<-label`
    dangling_start_map: HashMap<String, Vec<DanglingEdgeInfo>>,
}

pub type ParseError = Vec<(String, TokenCoordinate, Level)>;

pub struct Parser {
    pub graph: Graph,
    pub context: ParseContext,
}

impl Parser {
    /// Create a new parser from a lexer
    fn new() -> Self {
        Parser {
            graph: Graph::default(),
            context: ParseContext {
                last_node_id: None,
                grouping_state: GroupingState::Init,
                lifeline_state: LifelineState::NoLifelineActive {
                    previous_lifeline_termination_statement: None,
                },
                current_token_coordinate: TokenCoordinate::default(),
                dangling_sequence_flows: HashMap::default(),
                shorthand_blank_events: Default::default(),
                node_id_matcher: NodeIdMatcher::new(),
                pool_id_matcher: PoolIdMatcher::new(),
            },
        }
    }

    /// Parses the input and returns a graph
    fn parse(
        self,
        input: String,
        tokens: StatementStream,
    ) -> Result<Graph, Box<dyn std::error::Error>> {
        self.parse_inner(tokens).map_err(|e| {
            let result = Renderer::styled()
                .render(
                    Level::Error.title("Parser error").snippet(
                        Snippet::source(&input)
                            .line_start(1)
                            .fold(true)
                            .annotations(e.iter().map(
                                |(label, TokenCoordinate { start, end }, annotation_type)| {
                                    annotation_type.span(*start..*end).label(label)
                                },
                            )),
                    ),
                )
                .to_string();

            Box::new(lexer::MyParseError(result)).into()
        })
    }

    /// Exists purely for easier mapping of ParseError to Box<dyn...>
    fn parse_inner(mut self, tokens: StatementStream) -> Result<Graph, ParseError> {
        // Parse the input
        for (coordinate, token) in tokens {
            self.context.current_token_coordinate = coordinate;
            match token {
                Statement::Pool(label) => self.parse_pool(&label)?,
                Statement::Lane(label) => self.parse_lane(&label)?,
                Statement::Event(meta) => self.parse_event(meta, false)?,
                Statement::EventEnd(meta) => self.parse_event(meta, true)?,
                Statement::Activity(meta) => self.parse_activity(meta)?,
                Statement::GatewayBranchStart(meta) => self.parse_gateway_branch_start(meta)?,
                Statement::GatewayBranchEnd(meta) => self.parse_gateway_branch_end(meta)?,
                Statement::GatewayJoinStart(meta) => self.parse_gateway_join_start(meta)?,
                Statement::GatewayJoinEnd(meta) => self.parse_gateway_join_end(meta)?,
                Statement::SequenceFlowJump(meta) => self.parse_sequence_flow_jump(meta)?,
                Statement::SequenceFlowLand(meta) => self.parse_sequence_flow_land(meta)?,
                Statement::MessageFlow(meta) => self.parse_message_flow(meta)?,
                Statement::Data(meta) => self.parse_data(meta)?,
                Statement::PeBpmn(pe_bpmn) => self.parse_pe_bpmn(pe_bpmn)?,
                Statement::Layout(_) => {
                    eprintln!("Warning: layout instructions are currently ignored.")
                } //Token::Go => {
                  //    self.parse_go()?;
                  //    continue;
                  //}
                  //Token::Label(label) => self.parse_label(&label)?,
                  //Token::Join(exit_label, text) => self.parse_join(exit_label, text)?,
            }
        }

        for dangling_sequence_flows in self.context.dangling_sequence_flows.values() {
            let dangling_end_map = &dangling_sequence_flows.dangling_end_map;
            let dangling_start_map = &dangling_sequence_flows.dangling_start_map;
            let all_keys: std::collections::HashSet<_> = dangling_end_map
                .keys()
                .chain(dangling_start_map.keys())
                .collect();

            for key in &all_keys {
                match (dangling_end_map.get(*key), dangling_start_map.get(*key)) {
                    (Some(starts), Some(ends)) => {
                        for start in starts {
                            for end in ends {
                                if self.graph.nodes[start.known_node_id.0].pool
                                    != self.graph.nodes[end.known_node_id.0].pool
                                {
                                    return Err(vec![
                                        (
                                            format!(
                                                "The sequence flow connection `{key}` is not allowed to cross pools, but this node ..."
                                            ),
                                            start.tc,
                                            Level::Error,
                                        ),
                                        (
                                            "... and this node live in different pools."
                                                .to_string(),
                                            end.tc,
                                            Level::Error,
                                        ),
                                    ]);
                                }
                                self.graph.add_edge(
                                    start.known_node_id,
                                    end.known_node_id,
                                    EdgeType::Regular {
                                        text: Some(start.edge_text.clone()),
                                        bend_points:
                                            RegularEdgeBendPoints::ToBeDeterminedOrStraight,
                                    },
                                    FlowType::SequenceFlow,
                                );
                            }
                        }
                    }
                    (Some(still_danglings), None) | (None, Some(still_danglings)) => {
                        let label_used_in_other_pools = self
                            .context
                            .dangling_sequence_flows
                            .iter()
                            .map(|x| {
                                x.1.dangling_end_map.contains_key(*key)
                                    || x.1.dangling_start_map.contains_key(*key)
                            })
                            .filter(|x| *x)
                            .count()
                            > 1;
                        let additional_hint = if label_used_in_other_pools {
                            // TODO the error message should contain an example construct.
                            " (in case you wanted to cross pools: you can only send and receive messages across pools)"
                        } else {
                            ""
                        };
                        return Err(still_danglings
                            .iter()
                            .map(|still_dangling| {
                                (
                                    format!("Label `{key}` is not defined elsewhere in this pool{additional_hint}."),
                                    still_dangling.tc,
                                    Level::Error,
                                )
                            })
                            .collect());
                    }
                    (None, None) => unreachable!(), // Keys come from at least one map
                }
            }
        }

        self.process_shorthand_blank_events()?;

        crate::common::graph::validate_invariants(&self.graph)?;

        Ok(self.graph)
    }

    /// Set the current pool
    fn parse_pool(&mut self, label: &str) -> Result<(), ParseError> {
        let old_state = std::mem::replace(
            &mut self.context.lifeline_state,
            LifelineState::NoLifelineActive {
                previous_lifeline_termination_statement: None,
            },
        );
        err_from_unfinished_lifeline(old_state, self.context.current_token_coordinate)?;
        let pool_id = self.graph.add_pool(Some(label.to_string()));
        self.context.grouping_state = GroupingState::WithinPool {
            pool_id: pool_id,
            ptc: self.context.current_token_coordinate,
        };
        self.context.last_node_id = None;
        // For better error reporting, reset this to "fresh".
        self.context
            .pool_id_matcher
            .register(pool_id, Some(label.to_string()));
        Ok(())
    }

    /// Set the current lane
    fn parse_lane(&mut self, label: &str) -> Result<(), ParseError> {
        let old_state = std::mem::replace(
            &mut self.context.lifeline_state,
            LifelineState::NoLifelineActive {
                previous_lifeline_termination_statement: None,
            },
        );
        err_from_unfinished_lifeline(old_state, self.context.current_token_coordinate)?;

        match self.context.grouping_state {
            GroupingState::Init => {
                return Err(vec![(
                    format!(
                        "Lanes must always be part of a pool, please add a pool (e.g. `= My Pool`) above this line."
                    ),
                    self.context.current_token_coordinate,
                    Level::Error,
                )]);
            }
            GroupingState::WithinPool { pool_id, ptc }
            | GroupingState::WithinLane { pool_id, ptc, .. } => {
                let lane_id = self.graph.pools[pool_id.0].add_lane(Some(label.to_string()));
                self.context.grouping_state = GroupingState::WithinLane {
                    pool_id,
                    ptc,
                    lane_id,
                    ltc: self.context.current_token_coordinate,
                };
            }
            GroupingState::WithinAnonymousPool {
                first_encountered_node,
                ..
            } => {
                return Err(vec![
                    (
                        format!(
                            "This lane is part of a diagram without pools and lanes, since the diagram does not start with a pool. Hence this lane statement is illegal."
                        ),
                        self.context.current_token_coordinate,
                        Level::Error,
                    ),
                    (
                        format!(
                            "If you want to use pools and lanes, please add a pool (e.g. `= My Pool`) and a lane (e.g. `== My Lane`) *before your first node*."
                        ),
                        first_encountered_node,
                        Level::Note,
                    ),
                ]);
            }
            GroupingState::WithinAnonymousLane {
                ptc,
                first_encountered_node,
                ..
            } => {
                return Err(vec![
                    (
                        format!(
                            "This lane is part of a pool without explicit lanes, since the pool does not start with a lane. Hence this lane statement is illegal."
                        ),
                        self.context.current_token_coordinate,
                        Level::Error,
                    ),
                    (
                        format!(
                            "If you want to use multiple lanes within this pool, please add a lane (e.g. `== My Lane`) as the first statement inside of you pool *before any other nodes*."
                        ),
                        first_encountered_node,
                        Level::Note,
                    ),
                    (format!("The pool started here."), ptc, Level::Note),
                ]);
            }
        }

        self.context.last_node_id = None;
        Ok(())
    }

    /// Parse a gateway
    fn parse_gateway_branch_start(&mut self, meta: GatewayNodeMeta) -> Result<(), ParseError> {
        // Assign a unique node ID to this gateway
        let (pool_id, lane_id) = self.manage_pool_and_lane_ids_for_new_node();
        let node_id = self.graph.add_node(
            NodeType::RealNode {
                event: BpmnNode::Gateway(meta.gateway_type),
                display_text: meta.node_meta.display_text,
                tc: self.context.current_token_coordinate,
                transported_data: vec![],
                pe_bpmn_hides_protection_operations: false,
            },
            pool_id,
            lane_id,
        );

        self.connect_nodes(
            node_id,
            pool_id,
            LifelineState::NoLifelineActive {
                previous_lifeline_termination_statement: Some(
                    self.context.current_token_coordinate,
                ),
            },
        )?;

        for EdgeMeta { target, text_label } in meta.sequence_flow_jump_metas.into_iter() {
            self.context
                .dangling_sequence_flows
                .entry(pool_id)
                .or_default()
                .dangling_end_map
                .entry(target)
                .or_default()
                .push(DanglingEdgeInfo {
                    known_node_id: node_id,
                    edge_text: text_label,
                    tc: self.context.current_token_coordinate,
                });
        }
        Ok(())
    }

    // Helper to handle joins. Note, this is actually a real BPMN node, other than the Go target
    // and the intermediate labels.
    fn parse_gateway_join_end(&mut self, meta: GatewayNodeMeta) -> Result<(), ParseError> {
        // Assign a unique node ID to this gateway
        let (pool_id, lane_id) = self.manage_pool_and_lane_ids_for_new_node();
        let node_id = self.graph.add_node(
            NodeType::RealNode {
                event: BpmnNode::Gateway(meta.gateway_type),
                display_text: meta.node_meta.display_text,
                tc: self.context.current_token_coordinate,
                transported_data: vec![],
                pe_bpmn_hides_protection_operations: false,
            },
            pool_id,
            lane_id,
        );
        let old_state = std::mem::replace(
            &mut self.context.lifeline_state,
            LifelineState::ActiveLifeline {
                last_node_id: node_id,
                token_coordinate: self.context.current_token_coordinate,
            },
        );
        err_from_unfinished_lifeline(old_state, self.context.current_token_coordinate)?;
        for EdgeMeta { target, text_label } in meta.sequence_flow_jump_metas.into_iter() {
            self.context
                .dangling_sequence_flows
                .entry(pool_id)
                .or_default()
                .dangling_start_map
                .entry(target.clone())
                .or_default()
                .push(DanglingEdgeInfo {
                    known_node_id: node_id,
                    edge_text: text_label,
                    tc: self.context.current_token_coordinate,
                });
        }
        Ok(())
    }

    fn parse_gateway_branch_end(&mut self, meta: GatewayInnerMeta) -> Result<(), ParseError> {
        match self.context.lifeline_state {
LifelineState::ActiveLifeline { token_coordinate, .. } | LifelineState::StartedFromSequenceFlowLanding { token_coordinate, .. } =>
            return Err(vec![
(
                "Any sequence flow above this statement should be finished, but this is not the case.".to_string(),
                self.context.current_token_coordinate,Level::Error
            ),
            (
                "This statement does not finish the sequence flow. Try using `. End Event`, `F ->somewhere` or `X ->lbl1 ->lbl2` to finish the sequence flow.".to_string(),
                    token_coordinate, Level::Note
            )
            ]),
_ => (),
        }
        if !meta.sequence_flow_jump_meta.text_label.is_empty() {
            dbg!(
                "The label text should be stored within the edge or so, and warn if the other end already provided a text for this edge."
            );
        }
        self.context.lifeline_state = LifelineState::StartedFromSequenceFlowLanding {
            name: meta.sequence_flow_jump_meta.target,
            token_coordinate: self.context.current_token_coordinate,
        };
        Ok(())
    }

    fn parse_gateway_join_start(&mut self, meta: GatewayInnerMeta) -> Result<(), ParseError> {
        let (pool_id, _lane_id) = self.manage_pool_and_lane_ids_for_new_node();
        let old_state = std::mem::replace(
            &mut self.context.lifeline_state,
            LifelineState::NoLifelineActive {
                previous_lifeline_termination_statement: Some(
                    self.context.current_token_coordinate,
                ),
            },
        );
        let known_node_id = match old_state {
            LifelineState::ActiveLifeline { last_node_id, .. } => last_node_id,
            a => {
                return Err(err_from_no_lifeline_active(
                    a,
                    self.context.current_token_coordinate,
                ));
            }
        };
        if let NodeType::RealNode { ref mut tc, .. } = self.graph.nodes[known_node_id.0].node_type {
            tc.end = self.context.current_token_coordinate.end;
        } else {
            panic!("in the beginning there are no dummy nodes.");
        };
        // The only place where there are multiple uses of the same label allowed.
        self.context
            .dangling_sequence_flows
            .entry(pool_id)
            .or_default()
            .dangling_end_map
            .entry(meta.sequence_flow_jump_meta.target)
            .or_default()
            .push(DanglingEdgeInfo {
                known_node_id,
                edge_text: meta.sequence_flow_jump_meta.text_label,
                tc: self.context.current_token_coordinate,
            });
        Ok(())
    }

    /// Connects the current node with the previous node, handling incoming joining/jumping
    /// lifelines as well.
    fn connect_nodes(
        &mut self,
        current_node_id: NodeId,
        current_pool_id: PoolId,
        new_lifeline_state: LifelineState,
    ) -> Result<(), ParseError> {
        // Use `replace` here to avoid lifetime errors.
        match std::mem::replace(&mut self.context.lifeline_state, new_lifeline_state) {
            LifelineState::StartedFromSequenceFlowLanding {
                name,
                token_coordinate,
            } => {
                self.context
                    .dangling_sequence_flows
                    .entry(current_pool_id)
                    .or_default()
                    .dangling_start_map
                    .entry(name)
                    .or_default()
                    .push(DanglingEdgeInfo {
                        known_node_id: current_node_id,
                        edge_text: "".to_string(),
                        tc: token_coordinate,
                    });
                // Extend the token coordinate of the node onto the previous sequence flow landing
                // node, so that error reporting with bad branching connections is more helpful.
                if let NodeType::RealNode { ref mut tc, .. } =
                    self.graph.nodes[current_node_id.0].node_type
                {
                    tc.start = token_coordinate.start;
                } else {
                    panic!("in the beginning there are no dummy nodes.");
                };
            }
            LifelineState::ActiveLifeline { last_node_id, .. } => {
                // No text for regular sequence flow edges.
                self.graph.add_edge(
                    last_node_id,
                    current_node_id,
                    EdgeType::Regular {
                        text: None,
                        bend_points: RegularEdgeBendPoints::ToBeDeterminedOrStraight,
                    },
                    FlowType::SequenceFlow,
                );
            }
            a => {
                return Err(err_from_no_lifeline_active(
                    a,
                    self.context.current_token_coordinate,
                ));
            }
        }
        Ok(())
    }

    /// Common function to parse an event or task
    fn parse_event(&mut self, meta: lexer::EventMeta, is_end: bool) -> Result<(), ParseError> {
        let (pool_id, lane_id) = self.manage_pool_and_lane_ids_for_new_node();
        let ids = meta.node_meta.ids;
        let node_id = match self.context.lifeline_state {
            LifelineState::NoLifelineActive { .. } if !is_end => {
                let last_node_id = self.graph.add_node(
                    NodeType::RealNode {
                        event: BpmnNode::Event(
                            meta.event_type,
                            bpmn_node::EventVisual::default_start(meta.event_visual)?,
                        ),
                        display_text: meta.node_meta.display_text,
                        tc: self.context.current_token_coordinate,
                        transported_data: vec![],
                        pe_bpmn_hides_protection_operations: false,
                    },
                    pool_id,
                    lane_id,
                );
                self.context.lifeline_state = LifelineState::ActiveLifeline {
                    last_node_id,
                    token_coordinate: self.context.current_token_coordinate,
                };
                last_node_id
            }
            _ if is_end => {
                let node_id = self.graph.add_node(
                    NodeType::RealNode {
                        event: BpmnNode::Event(
                            meta.event_type,
                            bpmn_node::EventVisual::default_end(meta.event_visual)?,
                        ),
                        display_text: meta.node_meta.display_text,
                        tc: self.context.current_token_coordinate,
                        transported_data: vec![],
                        pe_bpmn_hides_protection_operations: false,
                    },
                    pool_id,
                    lane_id,
                );
                self.connect_nodes(
                    node_id,
                    pool_id,
                    LifelineState::NoLifelineActive {
                        previous_lifeline_termination_statement: Some(
                            self.context.current_token_coordinate,
                        ),
                    },
                )?;
                node_id
            }
            _ => {
                let node_id = self.graph.add_node(
                    NodeType::RealNode {
                        event: BpmnNode::Event(
                            meta.event_type,
                            bpmn_node::EventVisual::default_intermediate(
                                meta.event_visual,
                                meta.event_type,
                            )?,
                        ),
                        display_text: meta.node_meta.display_text,
                        tc: self.context.current_token_coordinate,
                        transported_data: vec![],
                        pe_bpmn_hides_protection_operations: false,
                    },
                    pool_id,
                    lane_id,
                );
                self.connect_nodes(
                    node_id,
                    pool_id,
                    LifelineState::ActiveLifeline {
                        last_node_id: node_id,
                        token_coordinate: self.context.current_token_coordinate,
                    },
                )?;
                node_id
            }
        };

        self.context
            .node_id_matcher
            .register(node_id, ids, &self.graph);

        if meta.shorthand_syntax {
            self.context.shorthand_blank_events.insert(node_id);
        }

        Ok(())
    }

    fn parse_activity(&mut self, meta: ActivityMeta) -> Result<(), ParseError> {
        let (pool_id, lane_id) = self.manage_pool_and_lane_ids_for_new_node();
        let ids = meta.node_meta.ids;
        let node_id = self.graph.add_node(
            NodeType::RealNode {
                event: BpmnNode::Activity(meta.activity_type),
                display_text: meta.node_meta.display_text,
                tc: self.context.current_token_coordinate,
                transported_data: vec![],
                pe_bpmn_hides_protection_operations: false,
            },
            pool_id,
            lane_id,
        );

        self.context
            .node_id_matcher
            .register(node_id, ids, &self.graph);

        self.connect_nodes(
            node_id,
            pool_id,
            LifelineState::ActiveLifeline {
                last_node_id: node_id,
                token_coordinate: self.context.current_token_coordinate,
            },
        )
    }

    fn parse_sequence_flow_jump(&mut self, meta: SequenceFlowMeta) -> Result<(), ParseError> {
        let (pool_id, _lane_id) = self.manage_pool_and_lane_ids_for_new_node();
        let old_state = std::mem::replace(
            &mut self.context.lifeline_state,
            LifelineState::NoLifelineActive {
                previous_lifeline_termination_statement: Some(
                    self.context.current_token_coordinate,
                ),
            },
        );
        let known_node_id = match old_state {
            LifelineState::ActiveLifeline { last_node_id, .. } => last_node_id,
            a => {
                return Err(err_from_no_lifeline_active(
                    a,
                    self.context.current_token_coordinate,
                ));
            }
        };

        if let NodeType::RealNode { ref mut tc, .. } = self.graph.nodes[known_node_id.0].node_type {
            tc.end = self.context.current_token_coordinate.end;
        } else {
            panic!("in the beginning there are no dummy nodes.");
        };

        self.context
            .dangling_sequence_flows
            .entry(pool_id)
            .or_default()
            .dangling_end_map
            .entry(meta.sequence_flow_jump_meta.target)
            .or_default()
            .push(DanglingEdgeInfo {
                known_node_id,
                edge_text: meta.sequence_flow_jump_meta.text_label,
                tc: self.context.current_token_coordinate,
            });

        Ok(())
    }

    fn parse_sequence_flow_land(&mut self, meta: SequenceFlowMeta) -> Result<(), ParseError> {
        // Make sure that this is not in some weird position.
        let (_pool_id, _lane_id) = self.manage_pool_and_lane_ids_for_new_node();

        // Check that a valid node type follows
        self.context.lifeline_state = LifelineState::StartedFromSequenceFlowLanding {
            name: meta.sequence_flow_jump_meta.target,
            token_coordinate: self.context.current_token_coordinate,
        };

        Ok(())
    }

    fn parse_message_flow(&mut self, meta: MessageFlowMeta) -> Result<(), ParseError> {
        let sender_node = self
            .context
            .node_id_matcher
            .find_node_id(&meta.sender_id, None)
            .ok_or_else(|| {
                vec![(
                    format!(
                        "Sender node with id {:?} was not found. Have you defined it?",
                        meta.sender_id
                    ),
                    self.context.current_token_coordinate,
                    Level::Error,
                )]
            })?;

        let receiver_node = self
            .context
            .node_id_matcher
            .find_node_id(&meta.receiver_id, None)
            .ok_or_else(|| {
                vec![(
                    format!(
                        "Receiver node with id {:?} was not found. Have you defined it?",
                        meta.receiver_id
                    ),
                    self.context.current_token_coordinate,
                    Level::Error,
                )]
            })?;

        self.graph.add_edge(
            sender_node,
            receiver_node,
            EdgeType::Regular {
                text: Some(meta.display_text),
                bend_points: RegularEdgeBendPoints::ToBeDeterminedOrStraight,
            },
            FlowType::MessageFlow(MessageFlowAux {
                transported_data: vec![],
                pebpmn_protection: vec![],
            }),
        );
        Ok(())
    }

    fn parse_data(&mut self, meta: DataMeta) -> Result<(), ParseError> {
        let mut pool_id = None;
        let mut lane_ids = Vec::<usize>::new();
        let mut edges = Vec::new();

        if meta.data_flow_metas.is_empty() {
            return Err(vec![(
                "Data element without recipients is unsupported at the moment.".to_string(),
                self.context.current_token_coordinate,
                Level::Error,
            )]);
        }

        for DataFlowMeta {
            direction,
            target,
            text_label,
        } in meta.data_flow_metas
        {
            let Some(node_id) = self.find_node_id(&target) else {
                // TODO this should not only print the `target` but also underline it. But for that
                // the DataFlowMeta must also record its TokenCoordinate.
                return Err(vec![(
                    format!(
                        "Data node recipient {target:?} has not yet been defined within the current pool. (Have you defined it afterwards? TODO).",
                    ),
                    self.context.current_token_coordinate,
                    Level::Error,
                )]);
            };
            let recipient_node = &self.graph.nodes[node_id];
            if let Some(prev_pool_id) = pool_id {
                if prev_pool_id != recipient_node.pool {
                    return Err(vec![(
                        format!(
                            "Data node recipient {target:?} is in another pool than the previous recipients. However, they must appear within the same pool (but may appear in different lanes)."
                        ),
                        self.context.current_token_coordinate,
                        Level::Error,
                    )]);
                }
            }
            pool_id = Some(recipient_node.pool);
            lane_ids.push(recipient_node.lane.0);
            edges.push((direction, node_id, text_label));
        }

        let sde_id;
        if meta.is_continuation {
            sde_id = SdeId(self.graph.data_elements.len() - 1);
        } else {
            sde_id = self
                .graph
                .add_sde(meta.node_meta.display_text.clone(), vec![]);
        }

        let event = BpmnNode::Data(
            meta.data_type,
            DataAux {
                sde_id: sde_id.clone(),
                pebpmn_protection: vec![],
            },
        );

        assert!(!lane_ids.is_empty());
        let data_node_id = self.graph.add_node(
            NodeType::RealNode {
                event,
                display_text: meta.node_meta.display_text,
                tc: self.context.current_token_coordinate,
                transported_data: vec![],
                pe_bpmn_hides_protection_operations: false,
            },
            pool_id.unwrap(),
            LaneId(
                (lane_ids.iter().sum::<usize>() as f64 / lane_ids.len() as f64).round() as usize,
            ),
        );
        self.graph.data_elements[sde_id]
            .data_element
            .push(data_node_id);

        for (direction, node_id, text_label) in edges {
            let (from, to) = match direction {
                Direction::Outgoing => (data_node_id, node_id),
                Direction::Incoming => (node_id, data_node_id),
            };
            self.graph.add_edge(
                from,
                to,
                EdgeType::Regular {
                    text: Some(text_label),
                    bend_points: RegularEdgeBendPoints::ToBeDeterminedOrStraight,
                },
                FlowType::DataFlow(DataFlowAux {
                    transported_data: vec![sde_id],
                }),
            );
        }
        let ids = meta.node_meta.ids;
        self.context
            .node_id_matcher
            .register(data_node_id, ids, &self.graph);

        Ok(())
    }

    fn manage_pool_and_lane_ids_for_new_node(&mut self) -> (PoolId, LaneId) {
        match self.context.grouping_state {
            GroupingState::Init => {
                let pool_id = self.graph.add_pool(None);
                let lane_id = self.graph.pools[pool_id.0].add_lane(None);
                self.context.grouping_state = GroupingState::WithinAnonymousPool {
                    first_encountered_node: self.context.current_token_coordinate,
                    pool_id,
                    lane_id,
                };
                (pool_id, lane_id)
            }
            GroupingState::WithinAnonymousPool {
                pool_id, lane_id, ..
            } => (pool_id, lane_id),
            GroupingState::WithinPool { pool_id, ptc } => {
                let lane_id = self.graph.pools[pool_id.0].add_lane(None);
                self.context.grouping_state = GroupingState::WithinAnonymousLane {
                    first_encountered_node: self.context.current_token_coordinate,
                    pool_id,
                    lane_id,
                    ptc,
                };
                (pool_id, lane_id)
            }
            GroupingState::WithinAnonymousLane {
                pool_id, lane_id, ..
            } => (pool_id, lane_id),
            GroupingState::WithinLane {
                pool_id, lane_id, ..
            } => (pool_id, lane_id),
        }
    }

    pub fn find_node_id(&self, needle: &str) -> Option<NodeId> {
        self.context.node_id_matcher.find_node_id(needle, None)
    }

    fn process_shorthand_blank_events(&mut self) -> Result<(), ParseError> {
        for node_id in &self.context.shorthand_blank_events {
            let node = &mut self.graph.nodes[*node_id];
            if node
                .incoming
                .iter()
                .any(|edge_id| self.graph.edges[*edge_id].is_message_flow())
            {
                let NodeType::RealNode { event, .. } = &mut node.node_type else {
                    panic!("During graph construction there are only real nodes");
                };
                *event = BpmnNode::Event(
                    EventType::Message,
                    if node.outgoing.is_empty() {
                        EventVisual::End
                    } else {
                        EventVisual::Catch(InterruptingKind::Interrupting)
                    },
                );
            } else if node
                .outgoing
                .iter()
                .any(|edge_id| self.graph.edges[*edge_id].is_message_flow())
            {
                let NodeType::RealNode { event, .. } = &mut node.node_type else {
                    panic!("During graph construction there are only real nodes");
                };
                *event = BpmnNode::Event(
                    EventType::Message,
                    if node.incoming.is_empty() {
                        EventVisual::Start(InterruptingKind::Interrupting)
                    } else {
                        EventVisual::Throw
                    },
                );
            }
        }
        Ok(())
    }
}

#[track_caller]
fn err_from_no_lifeline_active(
    l: LifelineState,
    current_token_coordinate: TokenCoordinate,
) -> ParseError {
    match l {
            LifelineState::NoLifelineActive { previous_lifeline_termination_statement: None } => vec![
(
                "This statement requires an active sequence flow. Try to add a start event `#` before this line.".to_string(),
                current_token_coordinate, Level::Error
)]
            ,
            LifelineState::NoLifelineActive { previous_lifeline_termination_statement: Some(tc) } => vec![
(
                "This statement requires an active sequence flow.".to_string(),
                current_token_coordinate, Level::Error
),(
                "The sequence flow was previously terminated here.".to_string(),
                tc, Level::Note
)]
            ,
            LifelineState::StartedFromSequenceFlowLanding { token_coordinate, .. } => vec![
(
                "This statement requires an active sequence flow.".to_string(),
                current_token_coordinate, Level::Error
),(
                "This statement itself is just a meta instruction and does not add a node. Try adding a `- Some Activity` after this statement.".to_string(),
                token_coordinate, Level::Note
)]
            ,
        l => panic!("{l:?}, {current_token_coordinate:?}"),
    }
}

fn err_from_unfinished_lifeline(
    l: LifelineState,
    current_token_coordinate: TokenCoordinate,
) -> Result<(), ParseError> {
    match l {
        LifelineState::StartedFromSequenceFlowLanding {
            token_coordinate, ..
        } => Err(vec![
            (
                "This statement cannot appear within an active sequence flow.".to_string(),
                current_token_coordinate,
                Level::Error,
            ),
            (
                // Have a slightly clearer error message here.
                "This statement introduced a sequence flow.".to_string(),
                token_coordinate,
                Level::Note,
            ),
        ]),
        LifelineState::ActiveLifeline {
            token_coordinate, ..
        } => Err(vec![
            (
                "This statement cannot appear within an active sequence flow.".to_string(),
                current_token_coordinate,
                Level::Error,
            ),
            (
                "This statement is part of the currently active sequence flow.".to_string(),
                token_coordinate,
                Level::Note,
            ),
        ]),
        LifelineState::NoLifelineActive { .. } => Ok(()),
    }
}

pub fn parse(input: String) -> Result<Graph, Box<dyn std::error::Error>> {
    Parser::new().parse(input.clone(), lex(input)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"
# Start Event
- Middle Event
. End Event
"#;

        let graph = parse(input.to_string())?;
        assert!(
            graph.edges.len() == 2,
            "Edge count is wrong, should be: {}",
            graph.edges.len()
        );
        Ok(())
    }

    #[test]
    fn gateway() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"
# Start Event
X ->Branch1 "Condition 1" ->Branch2 "Cond 2"

G <- Branch1
- Task B1
G ->JoinPoint "Joining Branches"

G <- Branch2
- Task B1
G ->JoinPoint "Joining Branches"

X<-JoinPoint
. End Event
"#;

        let graph = parse(input.to_string())?;
        assert!(
            graph.edges.len() == 6,
            "Edge count is wrong, should be: {}",
            graph.edges.len()
        );
        Ok(())
    }

    #[test]
    fn pool() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"
= Pool
== Lane1
# Start Event
- Task1
. End Event
== Lane2
# Start Event2
- Task2
. End Event 2
"#;

        let graph = parse(input.to_string())?;
        assert!(
            graph.edges.len() == 4,
            "Edge count is wrong, should be: {}",
            graph.edges.len()
        );
        Ok(())
    }

    #[test]
    fn jump() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"
= Pool
== Lane1
# Start Event
- Task
F ->jump
== Lane2
F <-jump
- Task
. End Event
"#;

        let graph = parse(input.to_string())?;
        assert!(
            graph.edges.len() == 3,
            "Edge count is wrong, should be: {}",
            graph.edges.len()
        );
        Ok(())
    }

    #[test]
    fn message_flow() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"
= Pool 1
# Start Event
- Task @sender
. End Event 
= Pool 2
# Start Event
- Task @receiver
. End Event

MF <-sender ->receiver
"#;

        let graph = parse(input.to_string())?;
        assert!(
            graph.edges.len() == 5,
            "Edge count is wrong, should be: {}",
            graph.edges.len()
        );

        assert!(
            graph.edges.iter().any(|x| x.from.0 == 1 && x.to.0 == 4),
            "No edge found for the message flow between node ids 1 to 4"
        );
        Ok(())
    }
}

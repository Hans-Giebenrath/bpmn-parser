//! The lexer is not speed optimized at all. Likely not optimized for anything. Just here to get
//! the job done. The most important thing is that we can get useful error location reporting out
//! of it. I assume that its speed is not the bottleneck of the end-to-end runtime, however in the
//! case of using it for real-time error diagnostics (as an LSP basically which does not do
//! layouting) then maybe(?) it needs to be optimized to get sub-millisecond speed out of it.

use core::fmt::Display;

use crate::common::bpmn_node::ActivityType;
use crate::common::bpmn_node::BoundaryEvent;
use crate::common::bpmn_node::BoundaryEventType;
use crate::common::bpmn_node::InterruptKind;
use crate::common::bpmn_node::TaskType;
use crate::common::graph::SdeId;
use crate::lexer::*;
use crate::parser::ParseError;

// TODO in the future make this &'static str
#[derive(Debug, Default, Clone)]
struct StrType(String);

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Pool(String),           // `=` for pool
    Lane(String),           // `==` for lane
    Event(EventMeta),       // `#` for start event
    EventEnd(EventMeta),    // `.` for end event
    Activity(ActivityMeta), // `-` for task activity
    // X ->somewhere
    GatewayBranchStart(GatewayNodeMeta),
    // X <-end
    GatewayJoinEnd(GatewayNodeMeta),
    SequenceFlowJump(SequenceFlowMeta), // `F ->`
    SequenceFlowLand(SequenceFlowMeta), // `F <-`
    MessageFlow(MessageFlowMeta),
    Data(DataMeta), // 'SD' for datastore 'OD' for dataobject '&' for continuation
    Layout(LayoutStatement),
    PeBpmn(PeBpmn),
    BoundaryEvent(BoundaryEventMeta),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutStatement {
    LeftOf(String, String),
    Above(String, String),
    /// The diagram will only contain at most so many nodes in one row, after which it will start
    /// from the left below again, similar to a line break.
    RowWidth(usize),
}

pub type StatementStream = std::vec::IntoIter<(TokenCoordinate, Statement)>;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SequenceFlowMeta {
    pub(crate) sequence_flow_jump_meta: EdgeMeta,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BoundaryEventMeta {
    pub(crate) sequence_flow_jump_meta: EdgeMeta,
    pub(crate) boundary_event: BoundaryEvent,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MessageFlowMeta {
    pub display_text: String,
    pub sender_id: String,
    pub receiver_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GatewayNodeMeta {
    pub(crate) gateway_type: GatewayType,
    pub(crate) node_meta: NodeMeta,
    pub(crate) sequence_flow_jump_metas: Vec<EdgeMeta>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct EdgeMeta {
    /// The name of the thing this edge is connected to. This is context dependent.
    pub(crate) target: String,
    /// The text which shall be displayed on the edge.
    pub(crate) text_label: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DataType {
    Store,
    Object,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DataAux {
    pub sde_id: SdeId,
    pub pebpmn_protection: Vec<PeBpmnProtection>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub enum PeBpmnProtection {
    SecureChannel(TokenCoordinate),
    Tee(TokenCoordinate),
    Mpc(TokenCoordinate),
}

impl Display for PeBpmnProtection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PeBpmnProtection::Mpc(..) => write!(f, "mpc"),
            PeBpmnProtection::Tee(..) => write!(f, "tee"),
            PeBpmnProtection::SecureChannel(..) => write!(f, "secure-channel"),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct DataMeta {
    pub(crate) node_meta: NodeMeta,
    pub(crate) data_type: DataType,
    pub(crate) data_flow_metas: Vec<DataFlowMeta>,
    /// & Object [Processed] <-t1 ->t2
    pub(crate) is_continuation: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct DataFlowMeta {
    pub(crate) direction: Direction,
    pub(crate) target: String,
    pub(crate) text_label: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum GatewayType {
    /// X
    Exclusive,
    /// +
    Parallel,
    /// O
    Inclusive,
    /// *
    Event,
}

#[derive(Eq, Debug, Clone, PartialEq)]
pub(crate) struct EventMeta {
    pub(crate) node_meta: NodeMeta,
    // When `#` or `.` was used.
    pub(crate) shorthand_syntax: bool,
    pub(crate) event_type: EventType,
    pub(crate) event_visual: (EventVisual, TokenCoordinate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActivityMeta {
    pub(crate) node_meta: NodeMeta,
    pub(crate) activity_type: ActivityType,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EventVisual {
    // Have `None` instead of using Option<EventVisual> because it makes matching less cluttered.
    #[default]
    None,
    Send,
    Receive,
    Throw,
    Catch,
}

#[derive(Eq, Debug, Clone, PartialEq)]
pub(crate) enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Eq, Debug, Clone, Copy, PartialEq)]
pub enum EventType {
    Blank,
    Message,
    Timer,
    Conditional,
    Link,
    Signal,
    Error,
    Escalation,
    Termination,
    Compensation,
    Cancel,
    Multiple,
    MultipleParallel,
}

#[derive(Debug, Clone, PartialEq)]
struct TaskMeta {
    pub(crate) node_meta: NodeMeta,
}

#[derive(Eq, Default, Debug, Clone, PartialEq)]
pub(crate) struct NodeMeta {
    pub(crate) display_text: String,
    pub(crate) ids: Vec<String>,
}

//enum Attribute {
//Id(String),
//DisplayText(String),
//SequenceFlowEdge(Direction, EdgeMeta),
//}

pub type Tokens = std::vec::IntoIter<(TokenCoordinate, Token)>;

enum AROptionalAttribute {
    Forbidden,
    Optional,
}

enum ARAttribute {
    Required,
    RequiredExact(usize),
    Optional,
    /// Contains error message for when the attribute is present.
    Forbidden,
}

enum ARFlowAttribute {
    Forbidden,
    //OptionalLeftAndRightArrows,
    RequiredExactlyOneArrow,
    RequiredExactlyOneRightArrow,
    RequiredLeftAndRightArrows,
    RequiredArrowsOfSameDirection,
    RequiredExactlyOneLeftAndOneRightArrow,
}

struct AssemblyRequest {
    display_text: ARAttribute,
    ids: ARAttribute,
    flows: ARFlowAttribute,
    task_type: AROptionalAttribute,
    event_visual: AROptionalAttribute,
}

#[derive(Default)]
struct AssembledAttributes {
    display_text: Option<String>,
    ids: Option<Vec<String>>,
    arrows_with_same_direction: Option<(Direction, Vec<EdgeMeta>)>,
    left_and_right_arrows: Option<Vec<(Direction, EdgeMeta)>>,
    task_type: TaskType,
    event_visual: EventVisual,
    // Parses the virtual token, as this is rather ubiquitous, so parse it centrally.
    used_shorthand_syntax: bool,
}

//struct EdgeAttribute {
//display_text: String,
//id: String,
//}

pub type AResult = Result<Statement, ParseError>;

fn to_pool(atts: Tokens, backup_tc: TokenCoordinate) -> AResult {
    let atts = assemble_attributes(
        "Pools",
        atts,
        AssemblyRequest {
            display_text: ARAttribute::Required,
            ids: ARAttribute::Forbidden,
            flows: ARFlowAttribute::Forbidden,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
        backup_tc,
    )?;
    Ok(Statement::Pool(atts.display_text.unwrap()))
}

fn to_lane(atts: Tokens, backup_tc: TokenCoordinate) -> AResult {
    let atts = assemble_attributes(
        "Lanes",
        atts,
        AssemblyRequest {
            display_text: ARAttribute::Required,
            ids: ARAttribute::Forbidden,
            flows: ARFlowAttribute::Forbidden,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
        backup_tc,
    )?;
    Ok(Statement::Lane(atts.display_text.unwrap()))
}

fn to_event(atts: Tokens, backup_tc: TokenCoordinate) -> AResult {
    let atts = assemble_attributes(
        "Action Events",
        atts,
        AssemblyRequest {
            display_text: ARAttribute::Required,
            ids: ARAttribute::Optional,
            flows: ARFlowAttribute::Forbidden,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
        backup_tc,
    )?;
    Ok(Statement::Event(EventMeta {
        node_meta: NodeMeta {
            display_text: atts.display_text.unwrap(),
            ids: atts.ids.unwrap_or_default(),
        },
        // TODO
        event_type: EventType::Blank,
        // TODO (Start is discovered by the parser)
        event_visual: (EventVisual::None, Default::default()),
        shorthand_syntax: atts.used_shorthand_syntax,
    }))
}

fn to_event_end(atts: Tokens, backup_tc: TokenCoordinate) -> AResult {
    // TODO use virtual tokens to distinguish end from non-end.
    let atts = assemble_attributes(
        "End events",
        atts,
        AssemblyRequest {
            display_text: ARAttribute::Required,
            ids: ARAttribute::Optional,
            flows: ARFlowAttribute::Forbidden,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
        backup_tc,
    )?;
    Ok(Statement::EventEnd(EventMeta {
        node_meta: NodeMeta {
            display_text: atts.display_text.unwrap(),
            ids: atts.ids.unwrap_or_default(),
        },
        // TODO
        event_type: EventType::Blank,
        event_visual: (EventVisual::None, Default::default()),
        shorthand_syntax: atts.used_shorthand_syntax,
    }))
}

fn to_task_activity(atts: Tokens, backup_tc: TokenCoordinate) -> AResult {
    let atts = assemble_attributes(
        "Activity Tasks",
        atts,
        AssemblyRequest {
            display_text: ARAttribute::Required,
            ids: ARAttribute::Optional,
            flows: ARFlowAttribute::Forbidden,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
        backup_tc,
    )?;
    Ok(Statement::Activity(ActivityMeta {
        activity_type: ActivityType::Task(TaskType::None),
        node_meta: NodeMeta {
            display_text: atts.display_text.unwrap(),
            ids: atts.ids.unwrap_or_default(),
        },
    }))
}

fn to_gateway_outer(mut atts: Tokens, backup_tc: TokenCoordinate) -> AResult {
    let (_, Token::GatewayType(gateway_type)) = atts.next().unwrap() else {
        unreachable!();
    };
    let atts = assemble_attributes(
        "Gateway Nodes",
        atts,
        AssemblyRequest {
            display_text: ARAttribute::Optional,
            ids: ARAttribute::Optional,
            flows: ARFlowAttribute::RequiredArrowsOfSameDirection,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
        backup_tc,
    )?;
    let sequence_flows = atts.arrows_with_same_direction.unwrap();
    let meta = GatewayNodeMeta {
        gateway_type,
        node_meta: NodeMeta {
            display_text: atts.display_text.unwrap_or_default(),
            ids: atts.ids.unwrap_or_default(),
        },
        sequence_flow_jump_metas: sequence_flows.1,
    };
    Ok(match sequence_flows.0 {
        Direction::Outgoing => Statement::GatewayBranchStart(meta),
        Direction::Incoming => Statement::GatewayJoinEnd(meta),
    })
}

fn to_data(mut tokens: Tokens, backup_tc: TokenCoordinate) -> AResult {
    let Some((_, Token::DataKind(data_kind, is_continuation))) = tokens.next() else {
        unreachable!();
    };

    let atts = assemble_attributes(
        "Data statements",
        tokens,
        AssemblyRequest {
            display_text: ARAttribute::Optional,
            ids: ARAttribute::Optional,
            flows: ARFlowAttribute::RequiredLeftAndRightArrows,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
        backup_tc,
    )?;

    let node_meta = NodeMeta {
        display_text: atts.display_text.unwrap_or_default(),
        ids: atts.ids.unwrap_or_default(),
    };

    let data_flow_metas = atts
        .left_and_right_arrows
        .unwrap_or_default()
        .into_iter()
        .map(|(direction, edge_meta)| DataFlowMeta {
            direction,
            target: edge_meta.target,
            text_label: edge_meta.text_label,
        })
        .collect::<Vec<_>>();

    Ok(Statement::Data(DataMeta {
        data_type: data_kind,
        node_meta,
        data_flow_metas,
        is_continuation,
    }))
}

fn to_boundary_event(mut tokens: Tokens, backup_tc: TokenCoordinate) -> AResult {
    let Some((_, Token::BoundaryEvent(event_type, interrupt_kind))) = tokens.next() else {
        unreachable!();
    };

    let atts = assemble_attributes(
        "Data statements",
        tokens,
        AssemblyRequest {
            display_text: ARAttribute::Forbidden,
            // TODO Allow. Could be used to color this one, or maybe fine tune its position or some such.
            ids: ARAttribute::Forbidden,
            flows: ARFlowAttribute::RequiredExactlyOneRightArrow,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
        backup_tc,
    )?;

    let Some((Direction::Outgoing, mut edge_metas)) = atts.arrows_with_same_direction else {
        unreachable!();
    };
    assert_eq!(edge_metas.len(), 1);

    Ok(Statement::BoundaryEvent(BoundaryEventMeta {
        sequence_flow_jump_meta: edge_metas.remove(0),
        boundary_event: BoundaryEvent {
            event_type,
            interrupt_kind,
        },
    }))
}

fn to_message_flow(atts: Tokens, backup_tc: TokenCoordinate) -> AResult {
    let atts = assemble_attributes(
        "Message Flows",
        atts,
        AssemblyRequest {
            display_text: ARAttribute::Optional,
            ids: ARAttribute::Forbidden,
            flows: ARFlowAttribute::RequiredExactlyOneLeftAndOneRightArrow,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
        backup_tc,
    )?;

    let flows = atts.left_and_right_arrows.unwrap();
    let (sender_id, receiver_id) = flows.iter().fold((None, None), |mut acc, (dir, meta)| {
        match dir {
            Direction::Incoming => acc.0 = Some(meta.target.clone()),
            Direction::Outgoing => acc.1 = Some(meta.target.clone()),
        }
        acc
    });

    Ok(Statement::MessageFlow(MessageFlowMeta {
        display_text: atts.display_text.unwrap_or_default(),
        sender_id: sender_id.expect("Message flow must have a sender ID"),
        receiver_id: receiver_id.expect("Message flow must have a receiver ID"),
    }))
}

fn to_sequence_flow(atts: Tokens, backup_tc: TokenCoordinate) -> AResult {
    let atts = assemble_attributes(
        "Sequence Flows",
        atts,
        AssemblyRequest {
            display_text: ARAttribute::Forbidden,
            ids: ARAttribute::Forbidden,
            flows: ARFlowAttribute::RequiredExactlyOneArrow,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
        backup_tc,
    )?;
    let sequence_flows = atts.arrows_with_same_direction.unwrap();
    let sequence_flow_jump_meta = sequence_flows.1.into_iter().next().unwrap();
    let meta = SequenceFlowMeta {
        sequence_flow_jump_meta,
    };
    Ok(match sequence_flows.0 {
        Direction::Outgoing => Statement::SequenceFlowJump(meta),
        Direction::Incoming => Statement::SequenceFlowLand(meta),
    })
}

fn to_layout_above(atts: Tokens, backup_tc: TokenCoordinate) -> AResult {
    let atts = assemble_attributes(
        "Layout Above Statements",
        atts,
        AssemblyRequest {
            display_text: ARAttribute::Forbidden,
            ids: ARAttribute::RequiredExact(2),
            flows: ARFlowAttribute::Forbidden,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
        backup_tc,
    )?;
    let mut ids = atts.ids.unwrap().into_iter();
    let id1 = ids.next().unwrap();
    let id2 = ids.next().unwrap();
    Ok(Statement::Layout(LayoutStatement::Above(id1, id2)))
}

fn to_layout_leftof(atts: Tokens, backup_tc: TokenCoordinate) -> AResult {
    let atts = assemble_attributes(
        "Layout LeftOf Statements",
        atts,
        AssemblyRequest {
            display_text: ARAttribute::Forbidden,
            ids: ARAttribute::RequiredExact(2),
            flows: ARFlowAttribute::Forbidden,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
        backup_tc,
    )?;
    let mut ids = atts.ids.unwrap().into_iter();
    let id1 = ids.next().unwrap();
    let id2 = ids.next().unwrap();
    Ok(Statement::Layout(LayoutStatement::LeftOf(id1, id2)))
}

//Maybe find some more type safe approach. Right now if `request` contains something Required, then
//the AssembledAttributes don't reflect that. Some kind of lambda would be nice.
fn assemble_attributes(
    what_plural: &'static str,
    atts: Tokens,
    request: AssemblyRequest,
    backup_tc: TokenCoordinate,
) -> Result<AssembledAttributes, ParseError> {
    let mut tc = None;
    let mut out = AssembledAttributes::default();
    for it in atts.into_iter() {
        tc.get_or_insert(it.0).end = it.0.end;
        match it.1 {
            Token::Text(val) => {
                if matches!(request.display_text, ARAttribute::Forbidden) {
                    return Err(vec![(
                        format!("{what_plural} cannot have a display text."),
                        it.0,
                    )]);
                }
                if let Some(display_text) = out.display_text {
                    return Err(vec![(
                        format!(
                            "Only one display text should be recognized by the lexer. This seems like a programming bug. Please report this together with your input file. (1: {display_text}, 2: {val})"
                        ),
                        it.0,
                    )]);
                }
                out.display_text = Some(val);
            }
            Token::Id(val) => {
                if matches!(request.ids, ARAttribute::Forbidden) {
                    return Err(vec![(format!("{what_plural} cannot have ids."), it.0)]);
                }
                out.ids.get_or_insert_default().push(val);
            }
            Token::Flow(val_direction, val_edge) => match request.flows {
                ARFlowAttribute::Forbidden => {
                    return Err(vec![(format!("{what_plural} cannot have flows."), (it.0))]);
                }
                ARFlowAttribute::RequiredLeftAndRightArrows => {
                    out.left_and_right_arrows
                        .get_or_insert_default()
                        .push((val_direction, val_edge));
                }
                ARFlowAttribute::RequiredArrowsOfSameDirection => {
                    match &mut out.arrows_with_same_direction {
                        Some((direction, edge_metas)) => {
                            if *direction != val_direction {
                                return Err(vec![(
                                    format!(
                                        "{what_plural} can only have one direction of flows (either '->label' or '<-label')."
                                    ),
                                    it.0,
                                )]);
                            }
                            edge_metas.push(val_edge);
                        }
                        None => {
                            out.arrows_with_same_direction = Some((val_direction, vec![val_edge]));
                        }
                    }
                }
                ARFlowAttribute::RequiredExactlyOneArrow => {
                    match &mut out.arrows_with_same_direction {
                        Some((_, _)) => {
                            return Err(vec![(
                                format!(
                                    "{what_plural} must have exactly 1 flow (e.g. <-label or ->label), not more."
                                ),
                                it.0,
                            )]);
                        }
                        None => {
                            out.arrows_with_same_direction = Some((val_direction, vec![val_edge]));
                        }
                    }
                }
                ARFlowAttribute::RequiredExactlyOneRightArrow => {
                    match &mut out.arrows_with_same_direction {
                        Some((_, _)) => {
                            return Err(vec![(
                                format!(
                                    "{what_plural} must have exactly 1 outgoing flow (->label), not more."
                                ),
                                it.0,
                            )]);
                        }
                        None => {
                            if val_direction == Direction::Incoming {
                                return Err(vec![(
                                    format!(
                                        "{what_plural} must have exactly 1 outgoing flow (->label). Incoming flows (<-label) are not allowed."
                                    ),
                                    it.0,
                                )]);
                            }
                            out.arrows_with_same_direction = Some((val_direction, vec![val_edge]));
                        }
                    }
                }
                ARFlowAttribute::RequiredExactlyOneLeftAndOneRightArrow => {
                    match &mut out.left_and_right_arrows {
                        Some(edge_metas) => {
                            if edge_metas.len() == 1 {
                                let first_dir = edge_metas[0].0.clone();
                                if first_dir == val_direction {
                                    return Err(vec![(
                                        format!(
                                            "Arrows have to be different directions. Found multiple arrows with the same direction: {first_dir:?} and {val_direction:?}."
                                        ),
                                        it.0,
                                    )]);
                                }
                                edge_metas.push((val_direction, val_edge));
                            } else {
                                return Err(vec![(
                                    "Too many arrows. Only one left and one right arrow allowed"
                                        .to_string(),
                                    it.0,
                                )]);
                            }
                        }
                        None => {
                            out.left_and_right_arrows = Some(vec![(val_direction, val_edge)]);
                        }
                    }
                }
            },
            Token::GatewayType(_) => {
                return Err(vec![(
                    "Programming error: GatewayType should be handled by the calling function."
                        .to_string(),
                    it.0,
                )]);
            }
            Token::DataKind(_, _) => {
                return Err(vec![(
                    "Programming error: DataKind should be handled by the calling function."
                        .to_string(),
                    it.0,
                )]);
            }
            Token::BoundaryEvent(_, _) => {
                return Err(vec![(
                    "Programming error: BoundaryEvent should be handled by the calling function."
                        .to_string(),
                    it.0,
                )]);
            }
            Token::UsedShorthandSyntax => out.used_shorthand_syntax = true,
            Token::ExtensionArgument(_) | Token::Separator => (),
        };
    }

    //let check_required = |req, opt: &mut Option<_>| matches!(req, ARAttribute::Required if opt.is_none());
    //let check_required_exact = |req, opt_vec: &mut Option<Vec<_>>| matches!(req, ARAttribute::RequiredExact(len) if opt_vec.get_or_insert_default().len() != len);

    // TODO ensure data flow errors are triggered even when no attributes are provided with a data-statement
    // Example: "OD" should prompt the user to add data flows.
    let Some(tc) = tc else {
        return Err(vec![("This statement seems to be missing attributes? Consider adding a display text, IDs (@id), flows (<-label\"Text\") etc".to_string(), backup_tc, )]);
    };

    if matches!(request.display_text, ARAttribute::Required if out.display_text.is_none()) {
        return Err(vec![(
            format!("{what_plural} must have a display text, but it is missing."),
            tc,
        )]);
    }
    assert!(
        !matches!(request.display_text, ARAttribute::RequiredExact(_)),
        "There can always only be one display text. Using RequiredExact is wrong here."
    );

    if matches!(request.ids, ARAttribute::Required) && out.ids.is_none() {
        return Err(vec![(
            format!("{what_plural} must have at least one ID (@id), but it is missing."),
            tc,
        )]);
    }

    if let ARAttribute::RequiredExact(len) = request.ids {
        let found_len = out.ids.as_ref().map_or_else(|| 0, |x| x.len());
        if len != found_len {
            return Err(vec![(
                format!(
                    "{what_plural} must have exactly {len} ID (@id) attribute(s), but you gave {found_len}."
                ),
                tc,
            )]);
        }
    }

    if matches!(
        request.flows,
        ARFlowAttribute::RequiredArrowsOfSameDirection
    ) && out.arrows_with_same_direction.is_none()
    {
        return Err(vec![(
            format!(
                "{what_plural} must have one flow (e.g. <-label or ->label), but it is missing."
            ),
            tc,
        )]);
    }

    if matches!(request.flows, ARFlowAttribute::RequiredLeftAndRightArrows)
        && out.left_and_right_arrows.is_none()
    {
        return Err(vec![(
            format!(
                "{what_plural} must have at least one data flow (e.g. <-label or ->label), but it is missing."
            ),
            tc,
        )]);
    }

    if let ARFlowAttribute::RequiredExactlyOneArrow = request.flows {
        let found_len = out
            .arrows_with_same_direction
            .as_ref()
            .map_or_else(|| 0, |x| x.1.len());
        if found_len != 1 {
            return Err(vec![(
                format!(
                    "{what_plural} must have exactly 1 flow (e.g. <-label or ->label), but you gave {found_len}."
                ),
                tc,
            )]);
        }
    }

    if let ARFlowAttribute::RequiredExactlyOneRightArrow = request.flows {
        let found_len = out
            .arrows_with_same_direction
            .as_ref()
            .map_or_else(|| 0, |x| x.1.len());
        if found_len != 1 {
            return Err(vec![(
                format!(
                    "{what_plural} must have exactly 1 outgoing flow (->label), but you gave {found_len}."
                ),
                tc,
            )]);
        }
    }

    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // =================
    // == Real Tokens ==
    // =================
    // Usually the order is (regex-ish): Text? (Id|Flow)*
    /// Any freeform test. This should always come first.
    Text(String), // `# This is freeform test ->this-is-not`

    // These are in any order
    Id(String),
    Flow(Direction, EdgeMeta), // `->label"text"`

    // ====================
    // == Virtual Tokens ==
    // ====================
    // Virtual tokens always come first. They encode some additional information of the statement
    // type. Usually the statement type is represented by the `StatementCallback`, but for
    // additional meta information these virtual tokens are used (to avoid having too many
    // `StatementCallback` functions around with otherwise duplicated code.
    GatewayType(GatewayType),
    DataKind(DataType, /* is continuation */ bool),
    UsedShorthandSyntax,
    BoundaryEvent(BoundaryEventType, InterruptKind),

    // ======================
    // == Extension Tokens ==
    // ======================
    ExtensionArgument(String),
    Separator,
}

macro_rules! charify {
    (M) => {
        'M'
    };
    (T) => {
        'T'
    };
    (C) => {
        'C'
    };
    (S) => {
        'S'
    };
    (E) => {
        'E'
    };
    (^) => {
        '^'
    };
    (<) => {
        '<'
    };
    (X) => {
        'X'
    };
    (#) => {
        '#'
    };
    (+) => {
        '+'
    };
}

// For the boundary events I use a macro to generate the arms. ChatGPT helped me to do this, so if
// this is stupid please tell me.
macro_rules! tt_as_boundary_event_type {
    (M) => {
        crate::common::bpmn_node::BoundaryEventType::Message
    };
    (T) => {
        crate::common::bpmn_node::BoundaryEventType::Timer
    };
    (C) => {
        crate::common::bpmn_node::BoundaryEventType::Conditional
    };
    (S) => {
        crate::common::bpmn_node::BoundaryEventType::Signal
    };
    (E) => {
        crate::common::bpmn_node::BoundaryEventType::Error
    };
    (^) => {
        crate::common::bpmn_node::BoundaryEventType::Escalation
    };
    (<) => {
        crate::common::bpmn_node::BoundaryEventType::Compensation
    };
    (X) => {
        crate::common::bpmn_node::BoundaryEventType::Cancel
    };
    (#) => {
        crate::common::bpmn_node::BoundaryEventType::Multiple
    };
    (+) => {
        crate::common::bpmn_node::BoundaryEventType::MultipleParallel
    };
}

macro_rules! tt_as_interrupt_kind {
    (!) => {
        crate::common::bpmn_node::InterruptKind::Interrupting
    };
    (+) => {
        crate::common::bpmn_node::InterruptKind::NonInterrupting
    };
}

macro_rules! maybe_parse_boundary_event {
    ($a:tt $b:tt, $self:ident) => {{
        if $self.current_char == Some(charify!($a)) && $self.continues_with(stringify!($b)) {
            let tc = $self.current_coord();
            $self.advance(); // $a
            $self.advance(); // $b

            $self
                .sas
                .next_statement(tc, $self.position, to_boundary_event)?;
            $self.sas.add_implicit_fragment(
                tc,
                $self.position,
                Token::BoundaryEvent(tt_as_boundary_event_type!($a), tt_as_interrupt_kind!($b)),
            );
            continue;
        }
    }};
}

#[derive(Debug, Clone, Default, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TokenCoordinate {
    pub source_file_idx: usize,
    pub start: usize,
    pub end: usize,
}

type StatementCallback = fn(Tokens, TokenCoordinate) -> AResult;

#[derive(Default, Clone)]
pub struct StatementAssemblyState {
    pub fragments: Vec<(TokenCoordinate, Token)>,
    assemble_statement_callback: Option<(TokenCoordinate, StatementCallback)>,
    /// Goal is to build this up
    assembled_statements: Vec<(TokenCoordinate, Statement)>,
    /// A statement is roughly one line, e.g. a full pool declaration, or a full gateway
    /// declaration. However, they could be broken across lines.
    /// To enable the use of the statement symbols also in the freeform text,
    /// we disable parsing them as statement symbols if a new statement was just activated.
    pub allow_new_statement: bool,
    fts: FreeformTextState,
    /// Copy of the input file for error reporting.
    input: String,
}

impl StatementAssemblyState {
    /// To be called immediately after `next_statement`.
    /// For fragments that are not in the code but just exist due to the way the parser is
    /// implemented. E.g. the to_gateway_outer function expects as the first fragment the gateway
    /// type, which is added implicitly.
    fn add_implicit_fragment(&mut self, mut tc: TokenCoordinate, new_end: usize, t: Token) {
        tc.end = new_end;
        assert!(
            self.assemble_statement_callback.is_some(),
            "This function should only be called immediately after a call to `next_statement`."
        );
        assert!(
            self.fragments.is_empty(),
            "This function should only be called immediately after a call to `next_statement`."
        );
        self.fragments.push((tc, t));
    }

    pub fn add_fragment(
        &mut self,
        mut tc: TokenCoordinate,
        new_end: usize,
        t: Token,
    ) -> Result<(), ParseError> {
        tc.end = new_end;
        if self.assemble_statement_callback.is_none() {
            return Err(vec![("You have not begun a statement, yet. Please start a statement (#, =, ==, -, etc) before writing any other tokens.".to_string(), tc, )]);
        }
        // Freetext is the first fragment to come, ever. So if we are about to push another
        // fragment, then implicitly also flush the freetext buffer.
        self.finish_freetext()?;
        self.fragments.push((tc, t));
        Ok(())
    }

    pub fn add_freetext(&mut self, tc: TokenCoordinate, c: char) -> Result<(), ParseError> {
        if !self.fts.active {
            if self.allow_new_statement {
                return Err(vec![(
                    format!(
                        "Found freeform text ({c}) which is not allowed in this position. Freeform text is only allowed as the first element of a statement. Did you intend to start a new statement? In this case, this statement type ({c}) is not known."
                    ),
                    tc,
                )]);
            } else {
                return Err(vec![(
                    format!(
                        "Found freeform text ({c}) which is not allowed in this position. Freeform text is only allowed as the first element of a statement."
                    ),
                    tc,
                )]);
            }
        }
        match self.fts.text {
            Some((TokenCoordinate { ref mut end, .. }, ref mut s)) => {
                *end = tc.end;
                s.push(c);
            }
            None => self.fts.text = Some((tc, c.to_string())),
        }
        Ok(())
    }

    fn finish_freetext(&mut self) -> Result<(), ParseError> {
        fn trim_with_counts(input: &str) -> (String, usize, usize) {
            let trimmed_start = input.trim_start();
            let start_trimmed = input.len() - trimmed_start.len();

            let trimmed_end = trimmed_start.trim_end();
            let end_trimmed = trimmed_start.len() - trimmed_end.len();

            (trimmed_end.to_string(), start_trimmed, end_trimmed)
        }

        if let Some((mut tc, s)) = self.fts.text.take() {
            if self.assemble_statement_callback.is_none() {
                return Err(vec![("You have not begun a statement, yet. Please start a statement (#, =, ==, -, etc) before writing any other tokens.".to_string(), tc, )]);
            }
            let (s, start, end) = trim_with_counts(&s);
            if !s.is_empty() {
                tc.start += start;
                tc.end -= end;
                self.fragments.push((tc, Token::Text(s.to_string())));
            }
        }
        self.fts.active = false;
        Ok(())
    }

    pub fn next_statement(
        &mut self,
        mut tc: TokenCoordinate,
        new_end: usize,
        callback: StatementCallback,
    ) -> Result<(), ParseError> {
        tc.end = new_end;
        assert!(self.allow_new_statement);
        self.allow_new_statement = false;
        if let Some((mut tc, callback)) = self.assemble_statement_callback.replace((tc, callback)) {
            // Update the token coordinate of the whole statement to point to the end of the last element.
            self.fragments
                .last()
                .inspect(|(token_tc, _)| tc.end = token_tc.end);
            let s = callback(std::mem::take(&mut self.fragments).into_iter(), tc)?;
            self.assembled_statements.push((tc, s));
        }
        self.fts.active = true;
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), ParseError> {
        self.finish_freetext()?;
        // If we have parsed any fragments, then we are open to parsing a new statement in the next
        // line.
        self.allow_new_statement =
            self.assemble_statement_callback.is_none() || !self.fragments.is_empty();
        Ok(())
    }

    pub fn finish(mut self) -> Result<StatementStream, ParseError> {
        self.finish_freetext()?;

        // Just insert some random statement to finish off the old one.
        self.allow_new_statement = true;
        self.next_statement(TokenCoordinate::default(), 0, to_pool)?;

        Ok(self.assembled_statements.into_iter())
    }

    fn previous_data_type(&mut self, tc: TokenCoordinate) -> Result<DataType, ParseError> {
        if let Some((_, Token::DataKind(data_type, _))) = self.fragments.first() {
            Ok(data_type.clone())
        } else {
            Err(vec![("You are not continuing a data element. Please only use '&' when continuing a data element ('OD' or 'SD').".to_string(), tc, )])
        }
    }
}

#[derive(Default, Clone)]
struct FreeformTextState {
    text: Option<(TokenCoordinate, String)>,
    active: bool,
}

#[derive(Clone)]
pub struct Lexer<'a> {
    // Technically could be &str, but this just adds lifetimes and is not necessary.
    input: String, // Input string
    remaining_input: std::str::Chars<'a>,
    pub position: usize,            // Current position in the input
    pub current_char: Option<char>, // Current character being examined
    pub line: usize,                // Current line number
    pub sas: StatementAssemblyState,
    pub source_file_idx: usize,
}

impl<'a> Lexer<'a> {
    // Create a new lexer from an input string
    pub fn new(
        input: String,
        mut remaining_input: std::str::Chars<'a>,
        source_file_idx: usize,
    ) -> Self {
        let current_char = remaining_input.next();

        Lexer {
            input: input.clone(),
            source_file_idx,
            remaining_input,
            position: 0,
            current_char,
            line: 0,
            sas: StatementAssemblyState {
                input,
                allow_new_statement: true,
                ..Default::default()
            },
        }
    }

    pub fn continues_with(&self, pat: &str) -> bool {
        self.remaining_input.as_str().starts_with(pat)
    }

    // Advance to the next character in the input
    pub fn advance(&mut self) {
        if self.current_char == Some('\n') {
            self.line += 1; // Move to the next line
        }

        if self.current_char.is_some() {
            self.current_char = self.remaining_input.next();
            self.position += 1;
        }
    }

    // Get the next token from the input
    pub fn run(mut self) -> Result<StatementStream, ParseError> {
        self.skip_whitespace(); // Skip any unnecessary whitespace

        loop {
            if self.sas.allow_new_statement {
                // Interrupting Boundary Events
                maybe_parse_boundary_event!(M!, self);
                maybe_parse_boundary_event!(T!, self);
                maybe_parse_boundary_event!(C!, self);
                maybe_parse_boundary_event!(S!, self);
                maybe_parse_boundary_event!(E!, self);
                maybe_parse_boundary_event!(^!, self);
                maybe_parse_boundary_event!(<!, self);
                maybe_parse_boundary_event!(X!, self);
                maybe_parse_boundary_event!(#!, self);
                maybe_parse_boundary_event!(+!, self);

                // Non-Interrupting Boundary Events
                maybe_parse_boundary_event!(M+, self);
                maybe_parse_boundary_event!(T+, self);
                maybe_parse_boundary_event!(C+, self);
                maybe_parse_boundary_event!(S+, self);
                //maybe_parse_boundary_event!(E+, self); // TODO diagnostics?
                maybe_parse_boundary_event!(^+, self);
                //maybe_parse_boundary_event!(<+, self); // TODO diagnostics?
                //maybe_parse_boundary_event!(X+, self); // TODO diagnostics?
                maybe_parse_boundary_event!(#+, self);
                maybe_parse_boundary_event!(++, self);
            }
            match self.current_char {
                Some('/') if self.continues_with("/") => {
                    while self.current_char != Some('\n') {
                        self.advance(); // Skip the comment
                    }
                }

                // Tokens
                Some('@') => {
                    let tc = self.current_coord();
                    self.advance();
                    let (tc_end, id) = self.read_label()?;
                    self.sas.add_fragment(tc, tc_end.end, Token::Id(id))?;
                }
                Some('-') if self.continues_with(">") => {
                    let tc = self.current_coord();
                    self.advance();
                    self.advance();

                    let (tc_end, target) = self.read_label()?;
                    let (tc_end, text_label) = self
                        .read_quoted_text()?
                        .unwrap_or_else(|| (tc_end, String::new()));

                    self.sas.add_fragment(
                        tc,
                        tc_end.end,
                        Token::Flow(Direction::Outgoing, EdgeMeta { target, text_label }),
                    )?;
                }
                Some('<') if self.continues_with("-") => {
                    let tc = self.current_coord();
                    self.advance();
                    self.advance();

                    let (tc_end, target) = self.read_label()?;
                    let (tc_end, text_label) = self
                        .read_quoted_text()?
                        .unwrap_or_else(|| (tc_end, String::new()));

                    self.sas.add_fragment(
                        tc,
                        tc_end.end,
                        Token::Flow(Direction::Incoming, EdgeMeta { target, text_label }),
                    )?;
                }

                // Statements
                Some('=') if self.sas.allow_new_statement => {
                    let tc = self.current_coord();
                    self.advance();
                    if self.current_char == Some('=') {
                        self.advance();
                        self.sas.next_statement(tc, self.position, to_lane)?;
                    } else {
                        self.sas.next_statement(tc, self.position, to_pool)?;
                    }
                }
                Some('#') if self.sas.allow_new_statement => {
                    let tc = self.current_coord();
                    self.advance();
                    self.sas.next_statement(tc, self.position, to_event)?;
                    self.sas
                        .add_implicit_fragment(tc, self.position, Token::UsedShorthandSyntax);
                }
                Some('-') if self.sas.allow_new_statement => {
                    let tc = self.current_coord();
                    self.advance();
                    self.sas
                        .next_statement(tc, self.position, to_task_activity)?;
                }
                Some('.') if self.sas.allow_new_statement => {
                    let tc = self.current_coord();
                    self.advance();
                    self.sas.next_statement(tc, self.position, to_event_end)?;
                    self.sas
                        .add_implicit_fragment(tc, self.position, Token::UsedShorthandSyntax);
                }
                Some('X') if self.sas.allow_new_statement => {
                    let tc = self.current_coord();
                    self.advance();
                    self.sas
                        .next_statement(tc, self.position, to_gateway_outer)?;
                    self.sas.add_implicit_fragment(
                        tc,
                        self.position,
                        Token::GatewayType(GatewayType::Exclusive),
                    );
                }
                Some('+') if self.sas.allow_new_statement => {
                    let tc = self.current_coord();
                    self.advance();
                    self.sas
                        .next_statement(tc, self.position, to_gateway_outer)?;
                    self.sas.add_implicit_fragment(
                        tc,
                        self.position,
                        Token::GatewayType(GatewayType::Parallel),
                    );
                }
                Some('O') if self.sas.allow_new_statement && !self.continues_with("D") => {
                    let tc = self.current_coord();
                    self.advance();
                    self.sas
                        .next_statement(tc, self.position, to_gateway_outer)?;
                    self.sas.add_implicit_fragment(
                        tc,
                        self.position,
                        Token::GatewayType(GatewayType::Inclusive),
                    );
                }
                Some('*') if self.sas.allow_new_statement => {
                    let tc = self.current_coord();
                    self.advance();
                    self.sas
                        .next_statement(tc, self.position, to_gateway_outer)?;
                    self.sas.add_implicit_fragment(
                        tc,
                        self.position,
                        Token::GatewayType(GatewayType::Event),
                    );
                }
                Some('M') if self.sas.allow_new_statement && self.continues_with("F") => {
                    let tc = self.current_coord();
                    self.advance(); // M
                    self.advance(); // F

                    self.sas
                        .next_statement(tc, self.position, to_message_flow)?;
                }
                Some('S') if self.sas.allow_new_statement && self.continues_with("D") => {
                    let tc = self.current_coord();
                    self.advance(); // S
                    self.advance(); // D

                    self.sas.next_statement(tc, self.position, to_data)?;
                    self.sas.add_implicit_fragment(
                        tc,
                        self.position,
                        Token::DataKind(DataType::Store, false),
                    );
                }

                Some('O') if self.sas.allow_new_statement && self.continues_with("D") => {
                    let tc = self.current_coord();
                    self.advance(); // O
                    self.advance(); // D

                    self.sas.next_statement(tc, self.position, to_data)?;
                    self.sas.add_implicit_fragment(
                        tc,
                        self.position,
                        Token::DataKind(DataType::Object, false),
                    );
                }
                Some('&') if self.sas.allow_new_statement => {
                    let tc = self.current_coord();
                    self.advance();

                    let previous_data_type = self.sas.previous_data_type(tc)?;
                    self.sas.next_statement(tc, self.position, to_data)?;
                    self.sas.add_implicit_fragment(
                        tc,
                        self.position,
                        Token::DataKind(previous_data_type, true),
                    );
                }
                Some('F') if self.sas.allow_new_statement => {
                    let tc = self.current_coord();
                    self.advance();
                    self.sas
                        .next_statement(tc, self.position, to_sequence_flow)?;
                }
                Some('[') if self.sas.allow_new_statement => {
                    let tc = self.current_coord();
                    self.advance();
                    self.start_extension(tc)?;
                }
                Some('a') if self.sas.allow_new_statement && self.continues_with("bove") => {
                    let tc = self.current_coord();
                    self.advance(); // a
                    self.advance(); // b
                    self.advance(); // o
                    self.advance(); // v
                    self.advance(); // e
                    self.sas
                        .next_statement(tc, self.position, to_layout_above)?;
                }
                Some('l') if self.sas.allow_new_statement && self.continues_with("eftof") => {
                    let tc = self.current_coord();
                    self.advance(); // l
                    self.advance(); // e
                    self.advance(); // f
                    self.advance(); // t
                    self.advance(); // o
                    self.advance(); // f
                    self.sas
                        .next_statement(tc, self.position, to_layout_leftof)?;
                }
                Some('\n') | Some('\r') => {
                    self.advance();
                    self.sas.end_line()?;
                }
                // Allow skipping whitespace at the beginning of the line.
                Some(' ') if self.sas.allow_new_statement => {
                    self.advance();
                }
                Some(c) => {
                    let tc = self.current_coord();
                    self.advance();
                    self.sas.add_freetext(tc, c)?;
                }
                None => {
                    break;
                }
            }
        }
        self.sas.finish()
    }

    // Skip over any whitespace
    pub fn skip_whitespace(&mut self) {
        while let Some(c) = self.current_char
            && c.is_whitespace()
            && c != '\n'
            && c != '\r'
        {
            self.advance();
        }
    }

    /// Labels are always mandatory, so if we would return an empty string, then we return an error
    /// instead.
    pub fn read_label(&mut self) -> Result<(TokenCoordinate, String), ParseError> {
        self.skip_whitespace();
        let coord_start = self.current_coord();
        let mut text = String::with_capacity(15);

        let mut coord_end = self.current_coord();
        while let Some(c) = self.current_char
            && is_allowed_symbol_in_label_or_id(c)
        {
            text.push(c);
            coord_end = self.current_coord();
            self.advance();
        }
        self.skip_whitespace();

        if text.is_empty() {
            Err(vec![(
                "Expected a label appearing here.".to_string(),
                coord_start,
            )])
        } else {
            Ok((coord_end, text))
        }
    }

    fn read_quoted_text(&mut self) -> Result<Option<(TokenCoordinate, String)>, ParseError> {
        self.skip_whitespace();
        let coord_start = self.current_coord();
        if self.current_char != Some('"') {
            return Ok(None);
        }
        self.advance(); // Skip the opening quote
        let mut text = String::new();
        loop {
            let Some(c) = self.current_char else {
                return Err(vec![(
                    "Quoted string was not finished, file input ended".to_string(),
                    coord_start,
                )]);
            };
            self.advance();
            if c == '"' {
                break;
            }
            if c == '\n' {
                return Err(vec![(
                    "Quoted string was not finished before line break".to_string(),
                    coord_start,
                )]);
            }
            text.push(c);
        }
        let mut coord_end = self.current_coord();
        // We skipped the closing '"' so point to the following symbol. But we only want to point
        // until the closing '"'.
        coord_end.end -= 1;

        self.skip_whitespace();
        Ok(Some((coord_end, text)))
    }

    pub fn current_coord(&self) -> TokenCoordinate {
        TokenCoordinate {
            start: self.position,
            end: self.position + 1,
            source_file_idx: self.source_file_idx,
        }
    }
}

pub fn is_allowed_symbol_in_label_or_id(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.')
}

pub(crate) fn lex(input: String, source_file_idx: usize) -> Result<StatementStream, ParseError> {
    Lexer::new(input.clone(), input.chars(), source_file_idx).run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_pool() -> Result<(), ParseError> {
        let mut result = lex("= Pool".to_string(), 0)?;
        assert!(matches!(result.next().unwrap().1, Statement::Pool(a) if a == "Pool"));
        Ok(())
    }

    #[test]
    fn basic_lane() -> Result<(), ParseError> {
        let mut result = lex("== Lane Yeah ".to_string(), 0)?;
        assert!(matches!(result.next().unwrap().1, Statement::Lane(a) if a == "Lane Yeah"));
        Ok(())
    }

    #[test]
    fn basic_activity_task() -> Result<(), ParseError> {
        let mut result = lex("- Lane Yeah @diddy ".to_string(), 0)?;
        assert!(matches!(result.next().unwrap().1, Statement::Activity(_)));
        Ok(())
    }

    #[test]
    fn basic_outer_gateway() -> Result<(), ParseError> {
        let mut result = lex(
            "X Lane Yeah ->label \"text\" ->label2 \"text\"".to_string(),
            0,
        )?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::GatewayBranchStart(_)
        ));

        let mut result = lex(
            "X Lane Yeah <-label \"text\" <-label2 \"text\"".to_string(),
            0,
        )?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::GatewayJoinEnd(_)
        ));

        Ok(())
    }

    #[test]
    fn basic_message_flow() -> Result<(), ParseError> {
        let mut result = lex("MF <-sender ->receiver".to_string(), 0)?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::MessageFlow(_)
        ));

        Ok(())
    }

    #[test]
    fn basic_sequence_flow() -> Result<(), ParseError> {
        let mut result = lex("F ->lbl".to_string(), 0)?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::SequenceFlowJump(_)
        ));

        let mut result = lex("F <-label \"text\"".to_string(), 0)?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::SequenceFlowLand(_)
        ));

        Ok(())
    }

    #[test]
    fn basic_leftof() -> Result<(), ParseError> {
        let mut result = lex("leftof @id1 @id2".to_string(), 0)?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::Layout(LayoutStatement::LeftOf(..))
        ));

        Ok(())
    }

    #[test]
    fn basic_above() -> Result<(), ParseError> {
        let mut result = lex("above @id1 @id2".to_string(), 0)?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::Layout(LayoutStatement::Above(..))
        ));

        Ok(())
    }

    #[test]
    fn basic_rowcount() -> Result<(), ParseError> {
        let mut result = lex("rowwidth 5".to_string(), 0)?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::Layout(LayoutStatement::RowWidth(..))
        ));

        Ok(())
    }
}

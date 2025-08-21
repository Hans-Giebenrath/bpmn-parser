use annotate_snippets::Level;
use annotate_snippets::Snippet;
use annotate_snippets::renderer::Renderer;

use crate::common::bpmn_node::ActivityType;
use crate::common::bpmn_node::TaskType;
use crate::common::graph::SdeId;
use crate::lexer::*;

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
    // G <-somewhere -- does not correspond to a visually visible node.
    GatewayBranchEnd(GatewayInnerMeta),
    // G ->end -- does not correspond to a visually visible node.
    GatewayJoinStart(GatewayInnerMeta),
    // X <-end
    GatewayJoinEnd(GatewayNodeMeta),
    SequenceFlowJump(SequenceFlowMeta), // `F ->`
    SequenceFlowLand(SequenceFlowMeta), // `F <-`
    MessageFlow(MessageFlowMeta),
    Data(DataMeta), // 'SD' for datastore 'OD' for dataobject '&' for continuation
    Layout(LayoutStatement),
    PeBpmn(PeBpmn),
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GatewayInnerMeta {
    pub(crate) sequence_flow_jump_meta: EdgeMeta,
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PeBpmnProtection {
    SecureChannel,
    Tee(PeBpmnProtectionSubType),
    Mpc(PeBpmnProtectionSubType),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PeBpmnProtectionSubType {
    Pool,
    Lane,
    Tasks,
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
pub(crate) enum GatewayType {
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
pub(crate) enum EventType {
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
    // Parses the virtual token, as this is rather ubiquitous, so parse it centrally..
    used_shorthand_syntax: bool,
}

//struct EdgeAttribute {
//display_text: String,
//id: String,
//}

pub type AResult = Result<Statement, (TokenCoordinate, String)>;

fn to_pool(atts: Tokens) -> AResult {
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
    )?;
    Ok(Statement::Pool(atts.display_text.unwrap()))
}

fn to_lane(atts: Tokens) -> AResult {
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
    )?;
    Ok(Statement::Lane(atts.display_text.unwrap()))
}

fn to_event(atts: Tokens) -> AResult {
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

fn to_event_end(atts: Tokens) -> AResult {
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

fn to_task_activity(atts: Tokens) -> AResult {
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
    )?;
    Ok(Statement::Activity(ActivityMeta {
        activity_type: ActivityType::Task(TaskType::None),
        node_meta: NodeMeta {
            display_text: atts.display_text.unwrap(),
            ids: atts.ids.unwrap_or_default(),
        },
    }))
}

fn to_gateway_outer(mut atts: Tokens) -> AResult {
    let (_, Token::GatewayType(gateway_type)) = atts.next().unwrap() else {
        return Err((TokenCoordinate::default(), "Programming error: an outer gateway node should always have its type as the first attribute.".to_string()));
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

fn to_gateway_inner(atts: Tokens) -> AResult {
    let atts = assemble_attributes(
        "Inner Gateway Label",
        atts,
        AssemblyRequest {
            display_text: ARAttribute::Forbidden,
            ids: ARAttribute::Forbidden,
            flows: ARFlowAttribute::RequiredExactlyOneArrow,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
    )?;
    let sequence_flows = atts.arrows_with_same_direction.unwrap();
    let sequence_flow_jump_meta = sequence_flows.1.into_iter().next().unwrap();
    let meta = GatewayInnerMeta {
        sequence_flow_jump_meta,
    };
    Ok(match sequence_flows.0 {
        Direction::Incoming => Statement::GatewayBranchEnd(meta),
        Direction::Outgoing => Statement::GatewayJoinStart(meta),
    })
}

fn to_data(mut tokens: Tokens) -> AResult {
    let (_, Token::DataKind(data_kind, is_continuation)) = tokens.next().ok_or_else(|| {
        (
            TokenCoordinate::default(),
            "Programming error: a data statement should always have a DataKind token as the first attribute.".to_string(),
        )
    })? else {
        return Err((
            TokenCoordinate::default(),
            "Programming error: a data statement should always have its kind (SD/OD) as the first attribute.".to_string(),
        ));
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

fn to_message_flow(atts: Tokens) -> AResult {
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

fn to_sequence_flow(atts: Tokens) -> AResult {
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

fn to_layout_above(atts: Tokens) -> AResult {
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
    )?;
    let mut ids = atts.ids.unwrap().into_iter();
    let id1 = ids.next().unwrap();
    let id2 = ids.next().unwrap();
    Ok(Statement::Layout(LayoutStatement::Above(id1, id2)))
}

fn to_layout_leftof(atts: Tokens) -> AResult {
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
    )?;
    let mut ids = atts.ids.unwrap().into_iter();
    let id1 = ids.next().unwrap();
    let id2 = ids.next().unwrap();
    Ok(Statement::Layout(LayoutStatement::LeftOf(id1, id2)))
}

fn to_layout_rowwidth(atts: Tokens) -> AResult {
    let atts = assemble_attributes(
        "Layout RowWidth Statements",
        atts,
        AssemblyRequest {
            display_text: ARAttribute::Required,
            ids: ARAttribute::Forbidden,
            flows: ARFlowAttribute::Forbidden,
            task_type: AROptionalAttribute::Forbidden,
            event_visual: AROptionalAttribute::Forbidden,
        },
    )?;
    let text = atts.display_text.unwrap();
    let rownumber = text.parse().map_err(|e| {
        (
            TokenCoordinate::default(),
            format!("rowwidth expects a positive number as argument (e.g. `rowwidth 5`). The argument `{text}` could not be parsed as a positive number: {e}"),
        )
    })?;
    Ok(Statement::Layout(LayoutStatement::RowWidth(rownumber)))
}

//Maybe find some more type safe approach. Right now if `request` contains something Required, then
//the AssembledAttributes don't reflect that. Some kind of lambda would be nice.
fn assemble_attributes(
    what_plural: &'static str,
    atts: Tokens,
    request: AssemblyRequest,
) -> Result<AssembledAttributes, (TokenCoordinate, String)> {
    let mut tc = None;
    let mut out = AssembledAttributes::default();
    for it in atts.into_iter() {
        tc.get_or_insert(it.0).end = it.0.end;
        match it.1 {
            Token::Text(val) => {
                if matches!(request.display_text, ARAttribute::Forbidden) {
                    return Err((it.0, format!("{what_plural} cannot have a display text.")));
                }
                if let Some(display_text) = out.display_text {
                    return Err((
                        it.0,
                        format!(
                            "Only one display text should be recognized by the lexer. This seems like a programming bug. Please report this together with your input file. (1: {display_text}, 2: {val})"
                        ),
                    ));
                }
                out.display_text = Some(val);
            }
            Token::Id(val) => {
                if matches!(request.ids, ARAttribute::Forbidden) {
                    return Err((it.0, format!("{what_plural} cannot have ids.")));
                }
                out.ids.get_or_insert_default().push(val);
            }
            Token::Flow(val_direction, val_edge) => match request.flows {
                ARFlowAttribute::Forbidden => {
                    return Err(((it.0), format!("{what_plural} cannot have flows.")));
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
                                return Err((
                                    it.0,
                                    format!(
                                        "{what_plural} can only have one direction of flows (either '->label' or '<-label')."
                                    ),
                                ));
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
                            return Err((
                                it.0,
                                format!(
                                    "{what_plural} must have exactly 1 flow (e.g. <-label or ->label)"
                                ),
                            ));
                        }
                        None => {
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
                                    return Err((
                                        it.0,
                                        format!(
                                            "Arrows have to be different directions. Found multiple arrows with the same direction: {first_dir:?} and {val_direction:?}."
                                        ),
                                    ));
                                }
                                edge_metas.push((val_direction, val_edge));
                            } else {
                                return Err((
                                    it.0,
                                    "Too many arrows. Only one left and one right arrow allowed"
                                        .to_string(),
                                ));
                            }
                        }
                        None => {
                            out.left_and_right_arrows = Some(vec![(val_direction, val_edge)]);
                        }
                    }
                }
                _ => {
                    return Err((it.0, "Unknown flow type.".to_string()));
                }
            },
            Token::GatewayType(_) => {
                return Err((
                    it.0,
                    "Programming error: GatewayType should be handled by the calling function."
                        .to_string(),
                ));
            }
            Token::DataKind(_, _) => {
                return Err((
                    it.0,
                    "Programming error: DataKind should be handled by the calling function."
                        .to_string(),
                ));
            }
            Token::UsedShorthandSyntax => out.used_shorthand_syntax = true,
            Token::ExtensionArgument(_) | Token::Separator => (),
        };
    }

    //let check_required = |req, opt: &mut Option<_>| matches!(req, ARAttribute::Required if opt.is_none());
    //let check_required_exact = |req, opt_vec: &mut Option<Vec<_>>| matches!(req, ARAttribute::RequiredExact(len) if opt_vec.get_or_insert_default().len() != len);

    // TODO ensure data flow errors are triggered even when no attributes are provided with a data statement
    // Example: "OD" should prompt the user to add data flows.
    let Some(tc) = tc else {
        return Err((TokenCoordinate::default(), "This statement seems to be missing attributes? Consider adding a display text, IDs (@id), flows (<-label\"Text\") etc".to_string()));
    };

    if matches!(request.display_text, ARAttribute::Required if out.display_text.is_none()) {
        return Err((
            tc,
            format!("{what_plural} must have a display text, but it is missing."),
        ));
    }
    assert!(
        !matches!(request.display_text, ARAttribute::RequiredExact(_)),
        "There can always only be one display text. Using RequiredExact is wrong here."
    );

    if matches!(request.ids, ARAttribute::Required) && out.ids.is_none() {
        return Err((
            tc,
            format!("{what_plural} must have at least one ID (@id), but it is missing."),
        ));
    }

    if let ARAttribute::RequiredExact(len) = request.ids {
        let found_len = out.ids.as_ref().map_or_else(|| 0, |x| x.len());
        if len != found_len {
            return Err((
                tc,
                format!(
                    "{what_plural} must have exactly {len} ID (@id) attribute(s), but you gave {found_len}."
                ),
            ));
        }
    }

    if matches!(
        request.flows,
        ARFlowAttribute::RequiredArrowsOfSameDirection
    ) && out.arrows_with_same_direction.is_none()
    {
        return Err((
            tc,
            format!(
                "{what_plural} must have one flow (e.g. <-label or ->label), but it is missing."
            ),
        ));
    }

    if matches!(request.flows, ARFlowAttribute::RequiredLeftAndRightArrows)
        && out.left_and_right_arrows.is_none()
    {
        return Err((
            tc,
            format!(
                "{what_plural} must have at least one data flow (e.g. <-label or ->label), but it is missing."
            ),
        ));
    }

    if let ARFlowAttribute::RequiredExactlyOneArrow = request.flows {
        let found_len = out
            .arrows_with_same_direction
            .as_ref()
            .map_or_else(|| 0, |x| x.1.len());
        if found_len != 1 {
            return Err((
                tc,
                format!(
                    "{what_plural} must have exactly 1 flow (e.g. <-label or ->label), but you gave {found_len}."
                ),
            ));
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
    // additional meta information these virtual tokens are used (to avoid having too manu
    // `StatementCallback` functions around with otherwise duplicated code.
    GatewayType(GatewayType),
    DataKind(DataType, bool),
    UsedShorthandSyntax,

    // ======================
    // == Extension Tokens ==
    // ======================
    ExtensionArgument(String),
    Separator,
}

#[derive(Debug, Clone, Default, Copy, PartialEq, Eq)]
pub struct TokenCoordinate {
    pub start: usize,
    pub end: usize,
}

pub type LexerError = (TokenCoordinate, String);
type StatementCallback = fn(Tokens) -> AResult;

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
    ) -> Result<(), Box<dyn std::error::Error>> {
        tc.end = new_end;
        if self.assemble_statement_callback.is_none() {
            return Err(self.conv_err((tc, "You have not begun a statement, yet. Please start a statement (#, =, ==, -, etc) before writing any other tokens.".to_string())));
        }
        // Freetext is the first fragment to come, ever. So if we are about to push another
        // fragment, then implicitly also flush the freetext buffer.
        self.finish_freetext()?;
        self.fragments.push((tc, t));
        Ok(())
    }

    pub fn add_freetext(
        &mut self,
        tc: TokenCoordinate,
        c: char,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.fts.active {
            if self.allow_new_statement {
                return Err(self.conv_err((
                    tc,
                    format!("Found freeform text ({c}) which is not allowed in this position. Freeform text is only allowed as the first element of a statement. Did you intend to start a new statement? In this case, this statement type ({c}) is not known."),
                )));
            } else {
                return Err(self.conv_err((
                    tc,
                    format!("Found freeform text ({c}) which is not allowed in this position. Freeform text is only allowed as the first element of a statement."),
                )));
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

    fn finish_freetext(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        fn trim_with_counts(input: &str) -> (String, usize, usize) {
            let trimmed_start = input.trim_start();
            let start_trimmed = input.len() - trimmed_start.len();

            let trimmed_end = trimmed_start.trim_end();
            let end_trimmed = trimmed_start.len() - trimmed_end.len();

            (trimmed_end.to_string(), start_trimmed, end_trimmed)
        }

        if let Some((mut tc, s)) = self.fts.text.take() {
            if self.assemble_statement_callback.is_none() {
                return Err(self.conv_err((tc, "You have not begun a statement, yet. Please start a statement (#, =, ==, -, etc) before writing any other tokens.".to_string())));
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
    ) -> Result<(), Box<dyn std::error::Error>> {
        tc.end = new_end;
        assert!(self.allow_new_statement);
        self.allow_new_statement = false;
        if let Some((mut tc, callback)) = self.assemble_statement_callback.replace((tc, callback)) {
            // Update the token coordinate of the whole statement to point to the end of the last element.
            self.fragments
                .last()
                .inspect(|(token_tc, _)| tc.end = token_tc.end);
            match callback(std::mem::take(&mut self.fragments).into_iter()) {
                Ok(s) => self.assembled_statements.push((tc, s)),
                Err(e) => return Err(self.conv_err_int(e, tc)),
            }
        }
        self.fts.active = true;
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.finish_freetext()?;
        // If we have parsed any fragments, then we are open to parsing a new statement in the new
        // line.
        self.allow_new_statement =
            self.assemble_statement_callback.is_none() || !self.fragments.is_empty();
        Ok(())
    }

    pub fn finish(&mut self) -> Result<StatementStream, Box<dyn std::error::Error>> {
        self.finish_freetext()?;

        // Just insert some random statement to finish off the old one.
        self.allow_new_statement = true;
        self.next_statement(TokenCoordinate::default(), 0, to_pool)?;

        Ok(self.assembled_statements.clone().into_iter())
    }

    pub fn conv_err(&mut self, e: LexerError) -> Box<dyn std::error::Error> {
        if let Some((tc, _)) = self.assemble_statement_callback.take() {
            self.conv_err_int(e, tc)
        } else {
            Box::new(MyParseError(self.annotate_snippet(e.0, e.1, Level::Error)))
        }
    }

    fn previous_data_type(
        &mut self,
        tc: TokenCoordinate,
    ) -> Result<DataType, Box<dyn std::error::Error>> {
        if let Some((_, Token::DataKind(data_type, _))) = self.fragments.first() {
            Ok(data_type.clone())
        } else {
            return Err(self.conv_err((tc, "You are not continuing a data element. Please only use '&' when continuing a data element ('OD' or 'SD').".to_string())));
        }
    }

    fn conv_err_int(
        &mut self,
        e: LexerError,
        statement_tc: TokenCoordinate,
    ) -> Box<dyn std::error::Error> {
        if e.0 == TokenCoordinate::default() {
            Box::new(MyParseError(self.annotate_snippet(
                statement_tc,
                e.1,
                Level::Error,
            )))
        } else {
            Box::new(MyParseError(format!(
                "{}\n\n{}",
                self.annotate_snippet(
                    statement_tc,
                    "Error occured while parsing this statement".to_string(),
                    Level::Info
                ),
                self.annotate_snippet(e.0, e.1, Level::Error)
            )))
        }
    }

    fn annotate_snippet(
        &self,
        TokenCoordinate { start, end, .. }: TokenCoordinate,
        e: String,
        annotation_type: Level,
    ) -> String {
        let result = Renderer::styled()
            .render(
                Level::Error.title("Parser error").snippet(
                    Snippet::source(&self.input)
                        .line_start(1)
                        .fold(true)
                        .annotation(annotation_type.span(start..end).label(&e)),
                ),
            )
            .to_string();
        result
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
}

impl<'a> Lexer<'a> {
    // Create a new lexer from an input string
    pub fn new(input: String, mut remaining_input: std::str::Chars<'a>) -> Self {
        let current_char = remaining_input.next();

        Lexer {
            input: input.clone(),
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
    pub fn run(&mut self) -> Result<StatementStream, Box<dyn std::error::Error>> {
        self.skip_whitespace(); // Skip any unnecessary whitespace

        loop {
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
                    let (tc_end, id) = self.read_label().map_err(|e| self.sas.conv_err(e))?;
                    self.sas.add_fragment(tc, tc_end.end, Token::Id(id))?;
                }
                Some('-') if self.continues_with(">") => {
                    let tc = self.current_coord();
                    self.advance();
                    self.advance();

                    let (tc_end, target) = self.read_label().map_err(|e| self.sas.conv_err(e))?;
                    let (tc_end, text_label) = self
                        .read_quoted_text()
                        .map_err(|e| self.sas.conv_err(e))?
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

                    let (tc_end, target) = self.read_label().map_err(|e| self.sas.conv_err(e))?;
                    let (tc_end, text_label) = self
                        .read_quoted_text()
                        .map_err(|e| self.sas.conv_err(e))?
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
                Some('G') if self.sas.allow_new_statement => {
                    let tc = self.current_coord();
                    self.advance();
                    self.sas
                        .next_statement(tc, self.position, to_gateway_inner)?;
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
                Some('r') if self.sas.allow_new_statement && self.continues_with("owwidth") => {
                    let tc = self.current_coord();
                    self.advance(); // r
                    self.advance(); // o
                    self.advance(); // w
                    self.advance(); // w
                    self.advance(); // i
                    self.advance(); // d
                    self.advance(); // t
                    self.advance(); // h
                    self.sas
                        .next_statement(tc, self.position, to_layout_rowwidth)?;
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
    pub fn read_label(&mut self) -> Result<(TokenCoordinate, String), LexerError> {
        self.skip_whitespace();
        let coord_start = self.current_coord();
        let mut text = String::with_capacity(15);

        while let Some(c) = self.current_char
            && is_allowed_symbol_in_label_or_id(c)
        {
            text.push(c);
            self.advance();
        }
        let coord_end = self.current_coord();
        self.skip_whitespace();

        if text.is_empty() {
            Err((coord_start, "Expected a label appearing here.".to_string()))
        } else {
            Ok((coord_end, text))
        }
    }

    fn read_quoted_text(&mut self) -> Result<Option<(TokenCoordinate, String)>, LexerError> {
        self.skip_whitespace();
        let coord_start = self.current_coord();
        if self.current_char != Some('"') {
            return Ok(None);
        }
        self.advance(); // Skip the opening quote
        let mut text = String::new();
        loop {
            let Some(c) = self.current_char else {
                return Err((
                    coord_start,
                    "Quoted string was not finished, file input ended".to_string(),
                ));
            };
            self.advance();
            if c == '"' {
                break;
            }
            if c == '\n' {
                return Err((
                    coord_start,
                    "Quoted string was not finished before line break".to_string(),
                ));
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
        }
    }
}

pub fn is_allowed_symbol_in_label_or_id(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.')
}

pub(crate) struct MyParseError(pub String);

impl std::fmt::Display for MyParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Debug for MyParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MyParseError {
    //fn source(&self) -> Option<&(dyn Error + 'static)> {
    //Some(&self.source)
    //}
}

pub(crate) fn lex(input: String) -> Result<StatementStream, Box<dyn std::error::Error>> {
    Lexer::new(input.clone(), input.chars()).run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_pool() -> Result<(), Box<dyn std::error::Error>> {
        let mut result = lex("= Pool".to_string())?;
        assert!(matches!(result.next().unwrap().1, Statement::Pool(a) if a == "Pool"));
        Ok(())
    }

    #[test]
    fn basic_lane() -> Result<(), Box<dyn std::error::Error>> {
        let mut result = lex("== Lane Yeah ".to_string())?;
        assert!(matches!(result.next().unwrap().1, Statement::Lane(a) if a == "Lane Yeah"));
        Ok(())
    }

    #[test]
    fn basic_activity_task() -> Result<(), Box<dyn std::error::Error>> {
        let mut result = lex("- Lane Yeah @diddy ".to_string())?;
        assert!(matches!(result.next().unwrap().1, Statement::Activity(_)));
        Ok(())
    }

    #[test]
    fn basic_outer_gateway() -> Result<(), Box<dyn std::error::Error>> {
        let mut result = lex("X Lane Yeah ->label \"text\" ->label2 \"text\"".to_string())?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::GatewayBranchStart(_)
        ));

        let mut result = lex("X Lane Yeah <-label \"text\" <-label2 \"text\"".to_string())?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::GatewayJoinEnd(_)
        ));

        Ok(())
    }

    #[test]
    fn basic_inner_gateway() -> Result<(), Box<dyn std::error::Error>> {
        let mut result = lex("G ->lbl".to_string())?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::GatewayJoinStart(_)
        ));

        let mut result = lex("G <-label \"text\"".to_string())?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::GatewayBranchEnd(_)
        ));

        Ok(())
    }

    #[test]
    fn basic_message_flow() -> Result<(), Box<dyn std::error::Error>> {
        let mut result = lex("MF <-sender ->receiver".to_string())?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::MessageFlow(_)
        ));

        Ok(())
    }

    #[test]
    fn basic_sequence_flow() -> Result<(), Box<dyn std::error::Error>> {
        let mut result = lex("F ->lbl".to_string())?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::SequenceFlowJump(_)
        ));

        let mut result = lex("F <-label \"text\"".to_string())?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::SequenceFlowLand(_)
        ));

        Ok(())
    }

    #[test]
    fn basic_leftof() -> Result<(), Box<dyn std::error::Error>> {
        let mut result = lex("leftof @id1 @id2".to_string())?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::Layout(LayoutStatement::LeftOf(..))
        ));

        Ok(())
    }

    #[test]
    fn basic_above() -> Result<(), Box<dyn std::error::Error>> {
        let mut result = lex("above @id1 @id2".to_string())?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::Layout(LayoutStatement::Above(..))
        ));

        Ok(())
    }

    #[test]
    fn basic_rowcount() -> Result<(), Box<dyn std::error::Error>> {
        let mut result = lex("rowwidth 5".to_string())?;
        assert!(matches!(
            result.next().unwrap().1,
            Statement::Layout(LayoutStatement::RowWidth(..))
        ));

        Ok(())
    }
}

use crate::lexer::DataAux;
use crate::lexer::DataType;
use crate::lexer::TokenCoordinate;
use crate::parser::ParseError;

use crate::lexer;
use crate::lexer::EventType;
use crate::lexer::GatewayType;

// TODO this should be simplified a lot: Event, Gateway, Task, BoundaryEvent, DataElement, something like that.
// The granularity here has no benefits and just makes other pieces more cumbersome.
// Also: Check if it is ok to have NodeMeta separate from EventMeta. Or also create a GatewayMeta
// and ActivityMeta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BpmnNode {
    Event(EventType, EventVisual), // Start event with label
    Gateway(GatewayType),          // Exclusive gateway event
    Activity(ActivityType),        // Task with label
    // `DataAux`: This should be some `HashMap<Something, Any>` where plugins can store arbitrary
    // data. Right now this is just hardcoded PE-BPMN data but is subject to change.
    Data(DataType, DataAux), // Data store reference with label
}

#[derive(Eq, Debug, Clone, PartialEq)]
pub struct BoundaryEvent {
    pub event_type: BoundaryEventType,
    pub interrupt_kind: InterruptKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryEventType {
    Error,
    Timer,
    Cancel,
    Signal,
    Message,
    Escalation,
    Conditional,
    Compensation,
    Multiple,
    MultipleParallel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityType {
    Task(TaskType),
    Subprocess,
    CallActivity,
    EventSubprocess,
    Transaction,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TaskType {
    #[default]
    None,
    Send,
    Receive,
    Manual,
    User,
    Script,
    Service,
    Businessrule,
    Multiple,
}

// TODO not sure if the inlined InterruptingKind is a good idea. Maybe it should be
// part of the container, but this should depend on how the logic can be written more easily in the
// emitting code (xml, svg, ...).
#[derive(Eq, Debug, Clone, PartialEq)]
pub enum EventVisual {
    Start(InterruptKind),
    Catch(InterruptKind),
    Throw,
    End,
}

impl EventVisual {
    pub(crate) fn default_start(
        (lexed, tc): (lexer::EventVisual, TokenCoordinate),
    ) -> Result<Self, ParseError> {
        use lexer::EventVisual as E;
        match lexed {
            E::None | E::Receive | E::Catch => Ok(Self::Start(InterruptKind::Interrupting)),
            E::Send | E::Throw => Err(vec![("Start events can only be ~catch or ~receive events (or simply remove this attribute).".to_string(), tc, )]),
        }
    }

    pub(crate) fn default_intermediate(
        (lexed, tc): (lexer::EventVisual, TokenCoordinate),
        event_type: EventType,
    ) -> Result<Self, ParseError> {
        use lexer::EventVisual as E;
        match lexed {
            E::Receive | E::Catch => Ok(Self::Catch(InterruptKind::Interrupting)),
            E::Send | E::Throw => Ok(Self::Throw),
            E::None => match event_type {
                EventType::Blank => Ok(Self::Throw),
                EventType::Message => Ok(Self::Catch(InterruptKind::Interrupting)),
                EventType::Timer => Ok(Self::Catch(InterruptKind::Interrupting)),
                EventType::Conditional => Ok(Self::Catch(InterruptKind::Interrupting)),
                EventType::Link => Ok(Self::Catch(InterruptKind::Interrupting)),
                EventType::Signal => Ok(Self::Catch(InterruptKind::Interrupting)),
                EventType::Error => Err(vec![(
                    "Error events cannot be intermediate events, but only end or boundary events."
                        .to_string(),
                    tc,
                )]),
                EventType::Escalation => Ok(Self::Throw),
                EventType::Termination => Err(vec![(
                    "Termination events cannot be intermediate events, but only end events."
                        .to_string(),
                    tc,
                )]),
                EventType::Compensation => Ok(Self::Throw),
                EventType::Cancel => Err(vec![(
                    "Cancel events cannot be intermediate events, but only end or boundary events."
                        .to_string(),
                    tc,
                )]),
                EventType::Multiple => Ok(Self::Catch(InterruptKind::Interrupting)),
                EventType::MultipleParallel => Ok(Self::Catch(InterruptKind::Interrupting)),
            },
        }
    }

    pub(crate) fn default_end(
        (lexed, tc): (lexer::EventVisual, TokenCoordinate),
    ) -> Result<Self, ParseError> {
        use lexer::EventVisual as E;
        match lexed {
            E::None | E::Send | E::Throw => Ok(Self::End),
            E::Receive | E::Catch => Err(vec![(
                "End events can only ~send or ~throw events (or simply remove this attribute)."
                    .to_string(),
                tc,
            )]),
        }
    }
}

#[derive(Eq, Debug, Clone, PartialEq)]
pub enum InterruptKind {
    NonInterrupting,
    Interrupting,
}

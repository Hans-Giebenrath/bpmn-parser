use crate::TokenCoordinate;
use crate::node::DataAux;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DataType {
    Store,
    Object,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BpmnNode {
    Event(EventType, EventVisual),          // Start event with label
    Gateway(GatewayType),                   // Exclusive gateway event
    Activity(ActivityType, ActivityMarker), // Task with label
    Data(DataType, DataAux),                // Data store reference with label
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
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

#[derive(Eq, Debug, Clone, PartialEq)]
pub struct BoundaryEvent {
    pub event_type: BoundaryEventType,
    pub interrupt_kind: InterruptKind,
    // Set x and y explicitly. This makes rendering a lot easier, although it is redundant.
    pub x: usize,
    pub y: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityType {
    // TODO missing: task markers, see bpmn p 155, note: the compensation marker can be combined
    // with the others.
    Task(TaskType),
    Subprocess,
    CallActivity,
    EventSubprocess,
    Transaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActivityMarker {
    pub multiple: bool,
    pub r#loop: bool,
    pub adhoc: bool,
    pub compensation: bool,
    pub plus_in_a_box: bool,
    /// Only for pools.
    pub is_blackbox: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActivityMarkerTokenCoordinates {
    pub multiple: TokenCoordinate,
    pub r#loop: TokenCoordinate,
    pub adhoc: TokenCoordinate,
    pub compensation: TokenCoordinate,
    pub plus_in_a_box: TokenCoordinate,
    pub is_blackbox: TokenCoordinate,
}

// TODO not sure if the inlined InterruptingKind is a good idea. Maybe it should be
// part of the container, but this should depend on how the logic can be written more easily in the
// emitting code (XML, SVG, etc.).
#[derive(Eq, Debug, Clone, Copy, PartialEq)]
pub enum EventVisual {
    Start(InterruptKind),
    Catch(InterruptKind),
    Throw,
    End,
}

#[derive(Eq, Debug, Clone, Copy, PartialEq)]
pub enum InterruptKind {
    NonInterrupting,
    Interrupting,
}

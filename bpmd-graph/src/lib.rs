#![no_std]

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, Default, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TokenCoordinate {
    pub source_file_idx: usize,
    pub start: usize,
    pub end: usize,
}

pub type ParseError = Vec<(String, TokenCoordinate)>;

pub mod bpmn_node;
pub mod config;
pub mod constraint;
pub mod direction;
pub mod display_text;
pub mod edge;
pub mod graph;
pub mod lane;
pub mod macros;
pub mod node;
pub mod pebpmd;
pub mod pool;

pub use bpmn_node::*;
pub use config::*;
pub use constraint::*;
pub use direction::*;
pub use display_text::*;
pub use edge::*;
pub use graph::*;
pub use lane::*;
pub use node::*;
pub use pool::*;

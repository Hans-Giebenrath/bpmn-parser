#![no_std]
extern crate alloc;
mod id_matcher;
pub mod lexer;
pub mod parser;

use bpmd_graph::ParseError;
use bpmd_graph::*;
use bpmd_util::timer::Timer;
pub use lexer::ImportHandler;
pub use lexer::Lexer;
pub use lexer::Statement;
pub use lexer::StatementStream;
pub use lexer::lex;
pub use parser::Parser;

pub fn parse(import_data: &mut dyn ImportHandler, timer: &mut Timer) -> Result<Graph, ParseError> {
    let stream = timer.time_it("lexing", || lex(import_data))?;
    timer.time_it("parsing", || parser::Parser::new().parse(stream))
}

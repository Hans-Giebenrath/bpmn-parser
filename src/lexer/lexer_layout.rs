use crate::{lexer::*, parser::ParseError};
use itertools::Itertools;
const PLACE_VARIANTS: [&str; 4] = [
    "place @node_a above @node_b",
    "place @node_a below @node_b",
    "place @node_a above or below @node_b",
    "place @node_a left of @node_b",
    // Ideas (wording suggested by ChatGPT):
    // "place @node_a (no) higher than @node_b",
];
pub fn to_place(tokens: Tokens, backup_tc: TokenCoordinate) -> AResult {
    let return_error = || {
        // Idea: maybe a "place @node_a exactly left of @node_b"? Or a "align @node_a and @node_b vertically" though that opens the question why not "... horizontally".
        // Better (ChatGPT suggestions): "place @node_a (no) higher than @node_b", or "(no) lower
        // than". I really like that. In these cases one can apply a smaller edge weight in y-coord
        // assignment, as those anyway
        let help = PLACE_VARIANTS
            .iter()
            .map(|content| format!("\n\t{content}"))
            .join("");
        Err(vec![(
            format!("Placement syntax incorrect. Expecting one of:{help}"),
            backup_tc,
        )])
    };

    let mut tokens = tokens.collect::<Vec<_>>();

    match &mut tokens[..] {
        [
            (tc_a, Token::Id(node_a)),
            (_, Token::Text(above)),
            (tc_b, Token::Id(node_b)),
        ] if above == "above" => Ok(Statement::Layout(LayoutStatement::Above {
            above: (*tc_a, std::mem::take(node_a)),
            below: (*tc_b, std::mem::take(node_b)),
        })),
        [
            (tc_a, Token::Id(node_a)),
            (_, Token::Text(below)),
            (tc_b, Token::Id(node_b)),
        ] if below == "below" => Ok(Statement::Layout(LayoutStatement::Above {
            below: (*tc_a, std::mem::take(node_a)),
            above: (*tc_b, std::mem::take(node_b)),
        })),
        [
            (tc_a, Token::Id(node_a)),
            (_, Token::Text(above)),
            (_, Token::Text(or)),
            (_, Token::Text(below)),
            (tc_b, Token::Id(node_b)),
        ] if above == "above" && or == "or" && below == "below" => {
            Ok(Statement::Layout(LayoutStatement::SameLayer(
                (*tc_a, std::mem::take(node_a)),
                (*tc_b, std::mem::take(node_b)),
            )))
        }
        [
            (tc_a, Token::Id(node_a)),
            (_, Token::Text(left)),
            (_, Token::Text(of)),
            (tc_b, Token::Id(node_b)),
        ] if left == "left" && of == "of" => Ok(Statement::Layout(LayoutStatement::LeftOf {
            left: (*tc_a, std::mem::take(node_a)),
            right: (*tc_b, std::mem::take(node_b)),
        })),
        _ => return_error(),
    }
}

impl<'a> Lexer<'a> {
    pub(crate) fn run_place(&mut self, mut tc: TokenCoordinate) -> Result<(), ParseError> {
        self.skip_whitespace();
        loop {
            match self.current_char {
                Some('/') if self.continues_with("/") => {
                    while self.current_char != Some('\n') {
                        self.advance(); // Skip the comment
                    }
                }
                Some('[') => {
                    let tc = self.current_coord();
                    return Err(vec![("We are already in a [place ...] statement, so opening another [...] statement is forbidden".to_string(), tc), ("Here starts the current [place ...] statement".to_string(), tc)]);
                }
                Some('(') | Some(')') => {
                    let tc = self.current_coord();
                    return Err(vec![("The [place ...] statement does not support grouping with brackets '(' or ')'. Use multiple [place ...] statements instead.".to_string(), tc)]);
                }
                Some(' ') => {
                    self.advance();
                }
                Some('\n') | Some('\r') => {
                    self.advance();
                }
                Some('@') => {
                    let tc = self.current_coord();
                    self.advance();
                    let (tc_end, id) = self.read_label()?;
                    self.sas.add_fragment(tc, tc_end.end, Token::Id(id))?;
                }
                Some(']') => {
                    self.advance();
                    break;
                }
                Some(_) => {
                    let tc = self.current_coord();
                    let (tc_end, argument) = self.read_label()?;
                    self.sas
                        .add_fragment(tc, tc_end.end, Token::Text(argument))?;
                }
                None => {
                    if !self.sas.fragments.is_empty() {
                        tc = TokenCoordinate {
                            start: tc.end,
                            end: self
                                .sas
                                .fragments
                                .last()
                                // TODO Which one? Is this correct?
                                .expect("Programming error: there should be at least one fragment")
                                .0
                                .end,
                            source_file_idx: tc.source_file_idx,
                        };
                    }
                    return Err(vec![("Unfinished extension block. Make sure you complete the full \"[...]\" statement.".to_string(), tc,)]);
                }
            }
        }
        Ok(())
    }
}

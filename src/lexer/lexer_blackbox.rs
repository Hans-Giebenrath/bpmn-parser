use crate::{lexer::*, parser::ParseError};
use itertools::Itertools;
const VARIANTS: [&str; 4] = [
    "blackbox all",
    "blackbox @pool_a @pool_b // any number of pools, at least 1",
    "unblackbox all",
    "unblackbox @pool_a @pool_b // any number of pools, at least 1",
];
pub fn to_blackbox(mut tokens: Tokens, backup_tc: TokenCoordinate) -> AResult {
    let return_error = |unexpected_token_tc: Option<TokenCoordinate>| {
        let help = VARIANTS
            .iter()
            .map(|content| format!("\n\t{content}"))
            .join("");
        let mut err = vec![(
            format!("Blackbox / Unblackbox syntax incorrect. Expecting one of:{help}"),
            backup_tc,
        )];
        if let Some(unexpected_token_tc) = unexpected_token_tc {
            err.push((
                "This word does not fit here".to_string(),
                unexpected_token_tc,
            ));
        }
        err
    };

    let (_, Token::BlackBox { is_unblackbox }) = tokens.next().unwrap() else {
        unreachable!();
    };
    let tokens = tokens.collect::<Vec<_>>();
    let Some(first) = tokens.first() else {
        return Err(return_error(None));
    };

    if let (_, Token::Text(text)) = first
        && text == "all"
    {
        if let Some((unexpected_token_tc, _)) = tokens.get(2) {
            // `[blackbox all @pool_a]` should point to `@pool_a` as the erroneous word.
            return Err(return_error(Some(*unexpected_token_tc)));
        } else {
            return Ok(Statement::Layout(LayoutStatement::BlackBoxAll {
                is_unblackbox,
            }));
        }
    }
    let pool_ids = tokens
        .into_iter()
        .map(|(tc, token)| {
            if let Token::Text(text) = token {
                Ok((tc, text))
            } else {
                Err(return_error(Some(tc)))
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Statement::Layout(LayoutStatement::BlackBox {
        is_unblackbox,
        pool_ids,
    }))
}

impl<'a> Lexer<'a> {
    pub(crate) fn run_blackbox(
        &mut self,
        mut tc: TokenCoordinate,
        blackbox_or_unblackbox: &str,
    ) -> Result<(), ParseError> {
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
                    return Err(vec![
                        (
                            format!(
                                "We are already in a [{blackbox_or_unblackbox} ...] statement, so opening another [...] statement is forbidden"
                            ),
                            tc,
                        ),
                        (
                            format!(
                                "Here starts the current [{blackbox_or_unblackbox} ...] statement"
                            ),
                            tc,
                        ),
                    ]);
                }
                Some('(') | Some(')') => {
                    let tc = self.current_coord();
                    return Err(vec![(
                        format!(
                            "The [{blackbox_or_unblackbox} ...] statement does not support grouping with brackets '(' or ')'. Use multiple [{blackbox_or_unblackbox} ...] statements instead."
                        ),
                        tc,
                    )]);
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

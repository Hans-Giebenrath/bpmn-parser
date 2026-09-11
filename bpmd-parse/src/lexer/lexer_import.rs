use crate::{lexer::*, parser::ParseError};

impl<'a> Lexer<'a> {
    pub(crate) fn run_import(&mut self) -> Result<(), ParseError> {
        let first_tc = self.current_coord();
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
                    return Err(vec![("We are already in a [import ...] statement, so opening another [...] statement is forbidden".to_string(), tc), ("Here starts the current [import ...] statement".to_string(), tc)]);
                }
                Some('(') | Some(')') => {
                    let tc = self.current_coord();
                    return Err(vec![("The [import ...] statement does not support grouping with brackets '(' or ')'. Use multiple [import ...] statements instead.".to_string(), tc)]);
                }
                Some(' ') => {
                    self.advance();
                }
                Some('\n') | Some('\r') => {
                    self.advance();
                }
                Some(']') => {
                    self.advance();
                    break;
                }
                Some('"') => {
                    let mut tc = self.current_coord();
                    let (tc_end, argument) = self.read_quoted_text()?.unwrap();
                    tc.end = tc_end.end;
                    {
                        let current_file_location = self.import_data.bpmd_source_files[self
                            .import_data
                            .import_stack
                            .last()
                            .unwrap()
                            .bpmn_source_file_index]
                            .canonicalized_location
                            // `.clone()` for the borrow checker.
                            .clone();
                        self.import_data.push(
                            current_file_location.parent().unwrap(),
                            argument,
                            tc,
                        )?;
                        self.sas.assembled_statements.extend(lex(self.import_data)?);
                        self.import_data.pop();
                    }
                }
                Some(_) => {
                    return Err(vec![("Illegal character detected in this extension block. The current syntax is: `[import \"path/to/my file.txt\"]`".to_string(), self.current_coord(),)]);
                }
                None => {
                    let tc = TokenCoordinate {
                        start: first_tc.start,
                        end: self.current_coord().end,
                        source_file_idx: first_tc.source_file_idx,
                    };
                    return Err(vec![("Unfinished extension block. Make sure you complete the full \"[...]\" statement.".to_string(), tc,)]);
                }
            }
        }
        Ok(())
    }
}

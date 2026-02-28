use crate::{lexer::*, parser::ParseError};
pub fn to_place(mut tokens: Tokens, _backup_tc: TokenCoordinate) -> AResult {
    let return_error = || {
        Err(
            vec![("Placement syntax incorrect. Expecting one of: (1) [place @node_a above @node_b], (2) [place @node_a below @node_b], (3) [place @node_a above or below @node_b], (4) [place @node_a left of @node_b]".to_string(), _backup_tc,
            )
            ]
        )
    };

    let t1 = tokens.next() else {
        return return_error();
    };
    //    [
    //        (_, Token::Id(node_a)),
    //        (_, Token::Text(above)),
    //        (_, Token::Id(node_b)),
    //    ] if above == "above" => Ok(Statement::Layout(LayoutStatement::Above(node_a, node_b.clone())),

    unreachable!()
}

impl<'a> Lexer<'a> {
    pub(crate) fn run_place(&mut self, mut tc: TokenCoordinate) -> Result<(), ParseError> {
        todo!()
    }
}

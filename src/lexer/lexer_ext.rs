use crate::{lexer::*, parser::ParseError};

#[derive(Debug, Clone, PartialEq)]
pub struct PeBpmn {
    pub r#type: PeBpmnType,
    pub meta: PeBpmnMeta, // stroke color, etc
}

#[derive(Debug, Clone, PartialEq)]
pub enum PeBpmnType {
    SecureChannel(SecureChannel),
    //SecureChannelWithExplicitSecret(SecureChannelWithExplicitSecret),
    Tee(Tee),
    Mpc(Mpc),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SecureChannel {
    pub sender: Option<(String, TokenCoordinate)>,
    pub receiver: Option<(String, TokenCoordinate)>,
    pub argument_ids: Vec<(String, TokenCoordinate)>,
    pub tc: TokenCoordinate,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tee {
    pub common: ComputationCommon,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mpc {
    pub common: ComputationCommon,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ComputationCommon {
    pub pebpmn_type: PeBpmnSubType,
    pub in_protect: Vec<Protection>,
    pub in_unprotect: Vec<Protection>,
    pub out_protect: Vec<Protection>,
    pub out_unprotect: Vec<Protection>,
    // Does this also need a token coordinate?
    pub data_without_protection: Vec<(String, TokenCoordinate)>,
    /// Technically this should be `Vec<Protection>`, since we need to know whether it was `no-rv`.
    pub data_already_protected: Vec<(String, TokenCoordinate)>,
    pub software_operators: Vec<(String, TokenCoordinate)>,
    /// Can contain placeholder `on-premises`: can be distinguished from `@on-premises` via
    /// TokenCoordinate length.
    pub hardware_operators: Vec<(String, TokenCoordinate)>,
    pub external_root_access: Vec<(String, TokenCoordinate)>,
    pub tc: TokenCoordinate,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PeBpmnSubType {
    Pool(String, TokenCoordinate),
    Lane(String, TokenCoordinate),
    Tasks(Vec<(String, TokenCoordinate)>),
}

impl Default for PeBpmnSubType {
    fn default() -> Self {
        PeBpmnSubType::Pool(String::new(), TokenCoordinate::default())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Protection {
    pub node: String,
    pub tc: TokenCoordinate,
    pub rv: Option<String>,
    // `no-rv` makes `rv` None, but there is still a TokenCoordinate.
    pub rv_tc: Option<TokenCoordinate>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PeBpmnMeta {
    pub stroke_color: Option<String>,
    pub fill_color: Option<String>,
}

enum BlockState {
    Closed,
    Opened,
}

fn assemble_secure_channel(mut tokens: Tokens, tc: TokenCoordinate) -> AResult {
    let mut secure_channel = SecureChannel {
        tc,
        ..Default::default()
    };
    let mut pe_bpmn_meta = PeBpmnMeta {
        stroke_color: None,
        fill_color: None,
    };

    secure_channel.sender =
        parse_id_or_placeholder(tokens.next(), &mut secure_channel.tc, "pre-sent", "sender")?;

    secure_channel.receiver = parse_id_or_placeholder(
        tokens.next(),
        &mut secure_channel.tc,
        "post-received",
        "receiver",
    )?;

    secure_channel.argument_ids = parse_optional_ids(&mut tokens, &mut secure_channel.tc)?;

    while let Some(it) = tokens.next() {
        secure_channel.tc.end = it.0.end;
        match it.1.clone() {
            Token::ExtensionArgument(val) => match val.as_str() {
                "secure-channel" => {
                    return Err(vec![("There is already a secure-channel defined".to_string(), it.0, )]);
                }
                "stroke-color" => {
                    if pe_bpmn_meta.stroke_color.is_some() {
                        return Err(vec![("There is already a stroke-color defined".to_string(),it.0, )]);
                    }
                    let nt = tokens.next();
                    let color = check_color_format(&it, nt)?;
                    pe_bpmn_meta.stroke_color = Some(color);
                    check_end_block(tokens.next())?;
                }
                "fill-color" => {
                    if pe_bpmn_meta.fill_color.is_some() {
                        return Err(vec![("There is already a fill-color defined".to_string(),it.0, )]);
                    }
                    let nt = tokens.next();
                    let color = check_color_format(&it, nt)?;
                    pe_bpmn_meta.fill_color = Some(color);
                    check_end_block(tokens.next())?;
                }
                _ => return Err(vec![("secure-channel does not allow this extension argument type. Allowed are stroke-color and fill-color.".to_string(),it.0, )]),
            },
            _ => return Err(vec![("Expected an extension argument".to_string(),it.0, )]),
        }
    }

    Ok(Statement::PeBpmn(PeBpmn {
        r#type: PeBpmnType::SecureChannel(secure_channel),
        meta: pe_bpmn_meta,
    }))
}

fn assemble_tee_or_mpc(
    mut tokens: Tokens,
    mut tc: TokenCoordinate,
    pe_bpmn_type: PeBpmnType,
    mut pe_bpmn_subtype: PeBpmnSubType,
) -> AResult {
    let tee_or_mpc = match pe_bpmn_type {
        PeBpmnType::Tee(_) => "tee",
        PeBpmnType::Mpc(_) => "mpc",
        _ => {
            return Err(vec![(
                "Invalid pe-bpmn type for tee or mpc".to_string(),
                tc,
            )]);
        }
    };
    let mut ids = parse_optional_ids(&mut tokens, &mut tc)?;
    match &mut pe_bpmn_subtype {
        PeBpmnSubType::Lane(name, out_tc) | PeBpmnSubType::Pool(name, out_tc) => match &mut ids[..]
        {
            [(_, _good_tc), (_, bad_tc), rest @ ..] => {
                bad_tc.end = rest.last().map(|id| id.1.end).unwrap_or(bad_tc.end);
                return Err(vec![(
                    format!(
                        "Expected exactly one ID for {tee_or_mpc}-pool. Please remove {}.",
                        if rest.is_empty() {
                            "this one"
                        } else {
                            " these ones"
                        }
                    ),
                    *bad_tc,
                )]);
            }
            [(a, b)] => {
                *name = std::mem::take(a);
                *out_tc = *b;
            }
            [] => {
                return Err(vec![("Missing an ID. Please add it.".to_string(), tc)]);
            }
        },
        PeBpmnSubType::Tasks(out_ids) => {
            if ids.is_empty() {
                return Err(vec![("Missing an ID. Please add it.".to_string(), tc)]);
            }
            *out_ids = ids;
        }
    }

    let mut pe_bpmn_meta = PeBpmnMeta {
        stroke_color: None,
        fill_color: None,
    };

    let mut in_protect = vec![];
    let mut in_unprotect = vec![];
    let mut out_protect = vec![];
    let mut out_unprotect = vec![];
    let mut external_root_access = vec![];
    let mut data_without_protection = vec![];
    let mut data_already_protected = vec![];
    let mut software_operators = vec![];
    let mut hardware_operators = vec![];

    let mut seen_external_root_access = false;
    let mut seen_software_operators = false;
    let mut seen_hardware_operators = false;

    while let Some(it) = tokens.next() {
        tc.end = it.0.end;
        // Check if the token is an extension argument
        if let Token::ExtensionArgument(val) = it.1.clone() {
            // Check for common arguments for tee and mpc
            if let Some(suffix) = val.strip_prefix(&format!("{tee_or_mpc}-")) {
                match suffix {
                    "data-without-protection" => {
                        data_without_protection
                            .append(&mut parse_optional_ids(&mut tokens, &mut tc)?);
                        continue;
                    }
                    "already-protected" => {
                        data_already_protected
                            .append(&mut parse_optional_ids(&mut tokens, &mut tc)?);
                        continue;
                    }
                    "in-protect" => {
                        in_protect.push(parse_data_flow_annotation(&mut tokens, tc, true, true)?);
                        continue;
                    }
                    "in-unprotect" => {
                        if matches!(pe_bpmn_subtype, PeBpmnSubType::Tasks(_)) {
                            return Err(vec![(
                                format!(
                                    "{tee_or_mpc}-tasks doesn't allow {tee_or_mpc}-in-unprotect statements. Allowed are {tee_or_mpc}-in-protect and {tee_or_mpc}-out-unprotect."
                                ),
                                it.0,
                            )]);
                        }
                        in_unprotect.push(parse_data_flow_annotation(
                            &mut tokens,
                            tc,
                            true,
                            false,
                        )?);
                        continue;
                    }
                    "out-protect" => {
                        if matches!(pe_bpmn_subtype, PeBpmnSubType::Tasks(_)) {
                            return Err(vec![(
                                format!(
                                    "{tee_or_mpc}-tasks doesn't allow {tee_or_mpc}-out-protect statements. Allowed are {tee_or_mpc}-in-protect and {tee_or_mpc}-out-unprotect."
                                ),
                                it.0,
                            )]);
                        }
                        out_protect.push(parse_data_flow_annotation(&mut tokens, tc, false, true)?);
                        continue;
                    }
                    "out-unprotect" => {
                        out_unprotect.push(parse_data_flow_annotation(
                            &mut tokens,
                            tc,
                            false,
                            false,
                        )?);
                        continue;
                    }
                    "software-operators" => {
                        if seen_software_operators {
                            return Err(vec![(
                                format!(
                                    "{tee_or_mpc}-software-operators is already defined. To add multiple pools, append them in one declaration, E.g.: ({tee_or_mpc}-software-operators @pool1 @pool2 ...)."
                                ),
                                it.0,
                            )]);
                        }
                        seen_software_operators = true;
                        software_operators = parse_optional_ids(&mut tokens, &mut tc)?;
                        continue;
                    }
                    "hardware-operators" => {
                        if seen_hardware_operators {
                            return Err(vec![(
                                format!(
                                    "{tee_or_mpc}-hardware-operators is already defined. To add multiple pools, append them in one declaration, E.g.: ({tee_or_mpc}-hardware-operators @pool1 @pool2 ...)."
                                ),
                                it.0,
                            )]);
                        }
                        seen_hardware_operators = true;
                        hardware_operators = parse_ids_or_placeholder(
                            &mut tokens,
                            it.0,
                            &mut tc,
                            "on-premises",
                            "pool",
                        )?;
                        continue;
                    }
                    "external-root-access" => {
                        if seen_external_root_access {
                            return Err(vec![(
                                format!(
                                    "{tee_or_mpc}-external-root-access is already defined. To add multiple pools, append them in one declaration, E.g.: ({tee_or_mpc}-external-root-access @pool1 @pool2 ...)."
                                ),
                                it.0,
                            )]);
                        }
                        seen_external_root_access = true;
                        external_root_access = parse_ids_or_placeholder(
                            &mut tokens,
                            it.0,
                            &mut tc,
                            "blocked",
                            "pool",
                        )?;
                        continue;
                    }
                    _ => {}
                }
            } else {
                // Handle specific tee or mpc arguments
                match tee_or_mpc {
                    "tee" => {}
                    "mpc" => {}
                    _ => {}
                }
                // Check the rest
                match val.as_str() {
                    "stroke-color" => {
                        if pe_bpmn_meta.stroke_color.is_some() {
                            return Err(vec![(
                                "There is already a stroke-color defined".to_string(),
                                it.0,
                            )]);
                        }
                        let nt = tokens.next();
                        let color = check_color_format(&it, nt)?;
                        pe_bpmn_meta.stroke_color = Some(color);
                        check_end_block(tokens.next())?;
                        continue;
                    }
                    "fill-color" => {
                        if pe_bpmn_meta.fill_color.is_some() {
                            return Err(vec![(
                                "There is already a fill-color defined".to_string(),
                                it.0,
                            )]);
                        }
                        let nt = tokens.next();
                        let color = check_color_format(&it, nt)?;
                        pe_bpmn_meta.fill_color = Some(color);
                        check_end_block(tokens.next())?;
                        continue;
                    }
                    _ => {}
                }
            }
            return Err(vec![(
                format!("{tee_or_mpc} does not allow this extension argument type"),
                it.0,
            )]);
        } else {
            return Err(vec![("Expected an extension argument".to_string(), it.0)]);
        }
    }

    if !seen_external_root_access {
        return Err(vec![(
            "Missing required 'tee-external-root-access' statement (even if empty)".to_string(),
            tc,
        )]);
    }

    let computation_common = ComputationCommon {
        pebpmn_type: pe_bpmn_subtype,
        in_protect,
        in_unprotect,
        out_protect,
        out_unprotect,
        data_without_protection,
        data_already_protected,
        software_operators,
        hardware_operators,
        external_root_access,
        tc,
    };

    match pe_bpmn_type {
        PeBpmnType::Tee(_) => Ok(Statement::PeBpmn(PeBpmn {
            r#type: PeBpmnType::Tee(Tee {
                common: computation_common,
            }),
            meta: pe_bpmn_meta,
        })),
        PeBpmnType::Mpc(_) => Ok(Statement::PeBpmn(PeBpmn {
            r#type: PeBpmnType::Mpc(Mpc {
                common: computation_common,
            }),
            meta: pe_bpmn_meta,
        })),
        _ => Err(vec![(
            "Invalid pe-bpmn type for tee or mpc".to_string(),
            tc,
        )]),
    }
}

pub fn to_pe_bpmn(mut tokens: Tokens, _backup_tc: TokenCoordinate) -> AResult {
    // Use first token is determine extension type
    let first_token = tokens.next();

    if let Some((tc, Token::ExtensionArgument(extension_type))) = first_token {
        // Implement extension types
        match extension_type.as_str() {
            "secure-channel" => assemble_secure_channel(tokens, tc),
            "tee-pool" => assemble_tee_or_mpc(
                tokens,
                tc,
                PeBpmnType::Tee(Tee::default()),
                PeBpmnSubType::Pool(String::new(), TokenCoordinate::default()),
            ),
            "tee-lane" => assemble_tee_or_mpc(
                tokens,
                tc,
                PeBpmnType::Tee(Tee::default()),
                PeBpmnSubType::Lane(String::new(), TokenCoordinate::default()),
            ),
            "tee-tasks" => assemble_tee_or_mpc(
                tokens,
                tc,
                PeBpmnType::Tee(Tee::default()),
                PeBpmnSubType::Tasks(Vec::new()),
            ),
            "mpc-pool" => assemble_tee_or_mpc(
                tokens,
                tc,
                PeBpmnType::Mpc(Mpc::default()),
                PeBpmnSubType::Pool(String::new(), TokenCoordinate::default()),
            ),
            "mpc-lane" => assemble_tee_or_mpc(
                tokens,
                tc,
                PeBpmnType::Mpc(Mpc::default()),
                PeBpmnSubType::Lane(String::new(), TokenCoordinate::default()),
            ),
            "mpc-tasks" => assemble_tee_or_mpc(
                tokens,
                tc,
                PeBpmnType::Mpc(Mpc::default()),
                PeBpmnSubType::Tasks(Vec::new()),
            ),
            _ => Err(vec![("Unknown pe-bpmn extension type".to_string(), tc)]),
        }
    } else if let Some((tc, Token::Text(text))) = first_token {
        Err(vec![(
            format!(
                "Found freeform text ({text}) which is not allowed in this position. Did you intend to start a new (...) block?"
            ),
            tc,
        )])
    } else {
        Err(vec![(
            "Found wrong format text which is not allowed in this position. Did you intend to start a new (...) block?".to_string(),
            first_token
                .expect("Programming error: first token should be of type ExtensionArgument")
                .0,

        )])
    }
}

impl<'a> Lexer<'a> {
    pub fn start_extension(&mut self, tc: TokenCoordinate) -> Result<(), ParseError> {
        self.skip_whitespace();
        loop {
            match self.current_char {
                Some('/') if self.continues_with("/") => {
                    while self.current_char != Some('\n') {
                        self.advance(); // Skip the comment
                    }
                }
                Some('\n') | Some('\r') => {
                    self.advance();
                }
                Some(' ') => {
                    self.advance();
                }
                // Empty []
                Some(']') => {
                    let tc = self.current_coord();
                    self.sas.next_statement(tc, self.position, to_pe_bpmn)?;
                    return Err(vec![("Empty extension block. Make sure you complete the full \"[...]\" statement".to_string(), tc, )]);
                }
                Some(_) => {
                    // Read extension
                    let mut tc = self.current_coord();
                    let (tc_end, extension_type) = self.read_label()?;
                    // Check extension type
                    if extension_type == "pe-bpmn" {
                        self.sas.next_statement(tc, self.position, to_pe_bpmn)?;
                        tc = TokenCoordinate {
                            start: tc.start,
                            end: tc_end.end,
                            source_file_idx: tc.source_file_idx,
                        };
                        self.run_pe_bpmn(tc)?;
                        // Empty [pe-bpmn]
                        if self.sas.fragments.is_empty() {
                            return Err(vec![("Empty extension block. Make sure you complete the full \"[pe-bpmn...]\" statement".to_string(), tc, )]);
                        }
                        break;
                    }
                    return Err(vec![(
                        "Invalid extension".to_string(),
                        TokenCoordinate {
                            start: tc.start,
                            end: tc_end.end,
                            source_file_idx: tc.source_file_idx,
                        },
                    )]);
                }
                None => {
                    return Err(vec![("Unfinished extension block. Make sure you complete the full \"[...]\" statement.".to_string(),tc, )]);
                }
            }
        }
        Ok(())
    }

    fn run_pe_bpmn(&mut self, mut tc: TokenCoordinate) -> Result<(), ParseError> {
        self.skip_whitespace();

        let mut block_state = BlockState::Closed;
        loop {
            match self.current_char {
                Some('(') => {
                    let tc = self.current_coord();
                    if matches!(block_state, BlockState::Opened) {
                        return Err(vec![("Already inside an active block".to_string(), tc)]);
                    }
                    block_state = BlockState::Opened;
                    self.advance();
                    let (tc_end, target) = self.read_argument(false)?;
                    self.sas
                        .add_fragment(tc, tc_end.end, Token::ExtensionArgument(target))?;
                }
                Some(')') => {
                    tc = self.current_coord();
                    if matches!(block_state, BlockState::Closed) {
                        return Err(vec![("There is no active block to close".to_string(), tc)]);
                    }
                    if self
                        .sas
                        .fragments
                        .last()
                        .map(|(_, token)| {
                            matches!(
                                token,
                                Token::ExtensionArgument(_) | Token::Id(_) | Token::Text(_)
                            )
                        })
                        .unwrap_or(false)
                    {
                        self.sas.add_fragment(tc, tc.end, Token::Separator)?;
                        block_state = BlockState::Closed;
                        self.advance();
                    }
                }
                Some('@') => {
                    let tc = self.current_coord();
                    self.advance();
                    let (tc_end, id) = self.read_label()?;
                    self.sas.add_fragment(tc, tc_end.end, Token::Id(id))?;
                }
                Some('/') if self.continues_with("/") => {
                    while self.current_char != Some('\n') {
                        self.advance(); // Skip the comment
                    }
                }
                Some('\n') | Some('\r') => {
                    self.advance();
                }
                Some(' ') => {
                    self.advance();
                }
                Some(']') => {
                    let tc = self.current_coord();
                    if !matches!(block_state, BlockState::Closed) {
                        return Err(vec![("Unfinished (...) block. Add a \")\" first to close the block before you can close the \"[...]\" statement.".to_string(),tc, )]);
                    }
                    self.advance();
                    break;
                }
                Some(_) => {
                    let tc = self.current_coord();
                    let (tc_end, argument) = self.read_argument(true)?;
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
                                .expect("Programming error: there should be at least one fragment")
                                .0
                                .end,
                            source_file_idx: tc.source_file_idx,
                        };
                    }
                    if !matches!(block_state, BlockState::Closed) {
                        return Err(vec![(
                            "Unfinished (...) block. Add a \")\" to close the block.".to_string(),
                            tc,
                        )]);
                    }
                    return Err(vec![("Unfinished extension block. Make sure you complete the full \"[...]\" statement.".to_string(), tc,)]);
                }
            }
        }
        Ok(())
    }

    pub fn read_argument(
        &mut self,
        is_text: bool,
    ) -> Result<(TokenCoordinate, String), ParseError> {
        self.skip_whitespace();
        let coord_start = self.current_coord();
        let mut text = String::with_capacity(15);

        while let Some(c) = self.current_char
            && is_allowed_symbol_in_args(c)
        {
            text.push(c);
            self.advance();
        }
        let coord_end = self.current_coord();
        self.skip_whitespace();

        if text.is_empty() {
            if is_text {
                Err(vec![(
                    "Forbidden text/symbol in this context".to_string(),
                    coord_start,
                )])
            } else {
                Err(vec![(
                    "This extension seems to be missing arguments. Consider adding a text, IDs (@id) etc".to_string(),
                    coord_start,

                )])
            }
        } else {
            Ok((coord_end, text))
        }
    }
}

fn parse_data_flow_annotation(
    tokens: &mut Tokens,
    tc: TokenCoordinate,
    is_in: bool,
    is_protected: bool,
) -> Result<Protection, ParseError> {
    let (tc, id) = match tokens.next() {
        Some((tc, Token::Id(id))) => Ok((tc, id)),
        Some((tc, Token::Separator)) => Err(vec![("Missing ID. Add an ID.".to_string(), tc)]),
        Some((tc, _)) => Err(vec![("Only accepting IDs.".to_string(), tc)]),
        None => Err(vec![("Missing ID. Add an ID.".to_string(), tc)]),
    }?;

    let (rv_tc, rv) = if (is_in && is_protected) || (!is_in && !is_protected) {
        match tokens.next() {
            Some((tc, Token::Id(id))) => Ok((Some(tc), Some(id))),
            Some((tc, Token::Text(text))) if text == "no-rv" => Ok((Some(tc), None)),
            Some((tc, Token::Separator)) => Err(vec![(
                "Reference value missing. Add an ID or 'no-rv' before closing the block."
                    .to_string(),
                tc,
            )]),
            Some((tc, _)) => Err(vec![(
                "Unknown argument. Expected an ID or 'no-rv'.".to_string(),
                tc,
            )]),
            None => Err(vec![("Missing second argument.".to_string(), tc)]),
        }?
    } else {
        (None, None)
    };

    check_end_block(tokens.next())?;

    let result = Protection {
        node: id,
        tc,
        rv,
        rv_tc,
    };

    Ok(result)
}

fn check_end_block(token: Option<(TokenCoordinate, Token)>) -> Result<(), ParseError> {
    match token {
        Some((_, Token::Separator)) => Ok(()),
        Some((tc, _)) => Err(vec![(
            "The block doesn't take any more arguments. Please remove it.".to_string(),
            tc,
        )]),
        None => Ok(()),
    }?;
    Ok(())
}

fn parse_id_or_placeholder(
    token: Option<(TokenCoordinate, Token)>,
    prev_tc: &mut TokenCoordinate,
    placeholder: &str,
    actor: &str,
) -> Result<Option<(String, TokenCoordinate)>, ParseError> {
    if let Some((tc, token)) = token {
        prev_tc.end = tc.end;
        match token {
            Token::Id(id) => Ok(Some((id, tc))),
            Token::Text(text) => {
                if text == placeholder {
                    Ok(None)
                } else {
                    Err(vec![(
                        format!("Unexpected argument. Did you mean \"{placeholder}\"?",),
                        tc,
                    )])
                }
            }
            Token::Separator => Err(vec![(
                format!("Expected more arguments. Add a {actor} ID or \"{placeholder}\""),
                tc,
            )]),
            _ => Err(vec![(
                format!("Unexpected argument. Did you mean \"{placeholder}\"?",),
                tc,
            )]),
        }
    } else {
        Err(vec![(
            format!("Missing {actor}. Add a {actor} ID or \"{placeholder}\"",),
            *prev_tc,
        )])
    }
}

fn parse_optional_ids(
    tokens: &mut impl Iterator<Item = (TokenCoordinate, Token)>,
    prev_tc: &mut TokenCoordinate,
) -> Result<Vec<(String, TokenCoordinate)>, ParseError> {
    let mut arguments_ids = Vec::new();
    for (tc, t) in tokens.by_ref() {
        prev_tc.end = tc.end;
        match t {
            Token::Separator => {
                break;
            }
            Token::Id(id) => {
                arguments_ids.push((id, tc));
            }
            _ => {
                return Err(vec![(
                    "Unexpected argument. Only accepting IDs".to_string(),
                    tc,
                )]);
            }
        }
    }

    Ok(arguments_ids)
}

fn parse_ids_or_placeholder(
    tokens: &mut impl Iterator<Item = (TokenCoordinate, Token)>,
    prev_tc: TokenCoordinate,
    full_pebpmn_tc: &mut TokenCoordinate,
    placeholder: &str,
    actor: &str,
) -> Result<Vec<(String, TokenCoordinate)>, ParseError> {
    let mut arguments_ids = Vec::<(String, TokenCoordinate)>::new();
    let mut placeholder_found = false;
    for (tc, t) in tokens.by_ref() {
        full_pebpmn_tc.end = tc.end;
        match t {
            Token::Separator => {
                break;
            }
            Token::Text(text) => {
                if text == placeholder {
                    placeholder_found = true;
                } else {
                    return Err(vec![(
                        format!("Unexpected argument. Did you mean \"{placeholder}\"?",),
                        tc,
                    )]);
                }
                if let [.., last] = arguments_ids.as_slice() {
                    return Err(vec![
                        (
                            format!(
                                "You already supplied an ID, so you cannot use \"{placeholder}\" anymore",
                            ),
                            tc,
                        ),
                        (
                            format!("Remove this one if you want to use \"{placeholder}\""),
                            last.1,
                        ),
                    ]);
                }
            }
            Token::Id(id) => {
                arguments_ids.push((id, tc));
                if placeholder_found {
                    return Err(vec![(
                        format!(
                            "You already supplied \"{placeholder}\", so no more IDs are allowed. Either use \"{placeholder}\" or use IDs",
                        ),
                        tc,
                    )]);
                }
            }
            _ => {
                return Err(vec![(
                    "Unexpected argument. Only accepting IDs".to_string(),
                    tc,
                )]);
            }
        }
    }
    if !placeholder_found && arguments_ids.is_empty() {
        return Err(vec![(
            format!("Expected more arguments. Add a {actor} ID or \"{placeholder}\""),
            prev_tc,
        )]);
    }
    Ok(arguments_ids)
}

fn check_color_format(
    it: &(TokenCoordinate, Token),
    nt: Option<(TokenCoordinate, Token)>,
) -> Result<String, ParseError> {
    if let Some(meta) = nt {
        if let (tc2, Token::Text(text)) = meta {
            if is_valid_hex_color(&text) {
                Ok(text)
            } else {
                Err(vec![(
                "Invalid hex color format. Expected # followed by 3 or 6 hex digits (e.g., #ABC or #FFFFFF)"
                    .to_string(),
                tc2,

            )])
            }
        } else if let (_, Token::Separator) = meta {
            Err(vec![(
                "This extension seems to be missing a hex color argument (e.g #FFFFFF)."
                    .to_string(),
                it.0,
            )])
        } else {
            Err(vec![(
                "This extension argument only accepts hex colors (e.g #FFFFFF)".to_string(),
                TokenCoordinate {
                    start: it.0.start,
                    end: meta.0.end,
                    source_file_idx: it.0.source_file_idx,
                },
            )])
        }
    } else {
        Err(vec![(
            "This extension seems to be missing a hex color argument (e.g #FFFFFF).".to_string(),
            it.0,
        )])
    }
}

pub fn is_allowed_symbol_in_args(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '#')
}

fn is_valid_hex_color(s: &str) -> bool {
    s.len() == 4 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
        || s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

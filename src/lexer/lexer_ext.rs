use crate::lexer::*;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PeBpmn {
    pub r#type: PeBpmnType,
    pub meta: PeBpmnMeta, // stroke color, etc
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PeBpmnType {
    SecureChannel(SecureChannel),
    //SecureChannelWithExplicitSecret(SecureChannelWithExplicitSecret),
    Tee(Tee),
    Mpc(Mpc),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SecureChannel {
    pub sender: Option<String>,
    pub receiver: Option<String>,
    pub argument_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tee {
    pub common: ComputationCommon,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mpc {
    pub common: ComputationCommon,
    // TODO: Add the correct type and usage for this field
    // pub in_protect_pre_sent: <Type>,
    // pub in_protect_post_received: <Type>,
    // pub in_channel: <Type>,
    // pub out_channel: <Type>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ComputationCommon {
    pub pebpmn_type: PeBpmnSubType,
    pub in_protect: Vec<DataFlowAnnotationLexer>,
    pub in_unprotect: Vec<DataFlowAnnotationLexer>,
    pub out_protect: Vec<DataFlowAnnotationLexer>,
    pub out_unprotect: Vec<DataFlowAnnotationLexer>,
    pub data_without_protection: Vec<String>,
    pub data_already_protected: Vec<String>,
    pub admin: Option<String>,
    pub external_root_access: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PeBpmnSubType {
    Pool(String),
    Tasks(Vec<String>),
}

impl Default for PeBpmnSubType {
    fn default() -> Self {
        PeBpmnSubType::Pool(String::new())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataFlowAnnotationLexer {
    pub node: String,
    pub rv: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct PeBpmnMeta {
    pub stroke_color: Option<String>,
    pub fill_color: Option<String>,
}

enum BlockState {
    Closed,
    Opened,
}

fn assemble_secure_channel(mut tokens: Tokens, mut tc: TokenCoordinate) -> AResult {
    let mut secure_channel = None;
    let mut pe_bpmn_meta = PeBpmnMeta {
        stroke_color: None,
        fill_color: None,
    };

    let sender = parse_id_or_placeholder(tokens.next(), tc, "pre-sent", "sender")?;
    tc.end += sender.len() + 1;
    secure_channel
        .get_or_insert_with(SecureChannel::default)
        .sender = Some(sender).filter(|s| !s.is_empty());

    let receiver = parse_id_or_placeholder(tokens.next(), tc, "post-received", "receiver")?;
    secure_channel
        .get_or_insert_with(SecureChannel::default)
        .receiver = Some(receiver).filter(|s| !s.is_empty());

    let args = parse_optional_ids(&mut tokens)?;
    secure_channel
        .get_or_insert_with(SecureChannel::default)
        .argument_ids = args;

    while let Some(it) = tokens.next() {
        tc.end = it.0.end;
        match it.1.clone() {
            Token::ExtensionArgument(val) => match val.as_str() {
                "secure-channel" => {
                    return Err((it.0, "There is already a secure-channel defined".to_string()));
                }
                "stroke-color" => {
                    if pe_bpmn_meta.stroke_color.is_some() {
                        return Err((it.0, "There is already a stroke-color defined".to_string()));
                    }
                    let nt = tokens.next();
                    let color = check_color_format(&it, nt)?;
                    pe_bpmn_meta.stroke_color = Some(color);
                    check_end_block(tokens.next())?;
                }
                "fill-color" => {
                    if pe_bpmn_meta.fill_color.is_some() {
                        return Err((it.0, "There is already a fill-color defined".to_string()));
                    }
                    let nt = tokens.next();
                    let color = check_color_format(&it, nt)?;
                    pe_bpmn_meta.fill_color = Some(color);
                    check_end_block(tokens.next())?;
                }
                _ => return Err((it.0, "secure-channel does not allow this extension argument type. Allowed are stroke-color and fill-color.".to_string())),
            },
            _ => return Err((it.0, "Expected an extension argument".to_string())),
        }
    }

    Ok(Statement::PeBpmn(PeBpmn {
        r#type: PeBpmnType::SecureChannel(
            secure_channel.expect("Programming error: an secure channel should always be created."),
        ),
        meta: pe_bpmn_meta,
    }))
}

fn assemble_tee_or_mpc(
    mut tokens: Tokens,
    mut tc: TokenCoordinate,
    pe_bpmn_type: PeBpmnType,
    is_pool: bool,
) -> AResult {
    let ids = parse_optional_ids(&mut tokens)?;
    let pebpmn_type = if !ids.is_empty() {
        match (is_pool, ids.as_slice()) {
            (true, [single_id]) => PeBpmnSubType::Pool(single_id.to_string()),
            (true, _) => {
                return Err((
                    tc,
                    "Expected exactly one ID for pool-based TEE or MPC".to_string(),
                ));
            }
            (false, _) => PeBpmnSubType::Tasks(ids),
        }
    } else {
        return Err((tc, "Missing an ID. Please add it.".to_string()));
    };

    let tee_or_mpc = match pe_bpmn_type {
        PeBpmnType::Tee(_) => "tee",
        PeBpmnType::Mpc(_) => "mpc",
        _ => return Err((tc, "Invalid pe-bpmn type for tee or mpc".to_string())),
    };

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
    let mut admin: Option<String> = None;

    let mut seen_external_root_access = false;

    while let Some(it) = tokens.next() {
        tc.end = it.0.end;
        // Check if the token is an extension argument
        if let Token::ExtensionArgument(val) = it.1.clone() {
            // Check for common arguments for tee and mpc
            if let Some(suffix) = val.strip_prefix(&format!("{tee_or_mpc}-")) {
                match suffix {
                    "data-without-protection" => {
                        data_without_protection.append(&mut parse_optional_ids(&mut tokens)?);
                        continue;
                    }
                    "already-protected" => {
                        data_already_protected.append(&mut parse_optional_ids(&mut tokens)?);
                        continue;
                    }
                    "in-protect" => {
                        in_protect.push(parse_data_flow_annotation(&mut tokens, tc, true, true)?);
                        continue;
                    }
                    "in-unprotect" => {
                        if !is_pool {
                            return Err((
                                it.0,
                                format!(
                                    "{tee_or_mpc}-tasks doesn't allow {tee_or_mpc}-in-unprotect statements. Allowed are {tee_or_mpc}-in-protect and {tee_or_mpc}-out-unprotect."
                                ),
                            ));
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
                        if !is_pool {
                            return Err((
                                it.0,
                                format!(
                                    "{tee_or_mpc}-tasks doesn't allow {tee_or_mpc}-out-protect statements. Allowed are {tee_or_mpc}-in-protect and {tee_or_mpc}-out-unprotect."
                                ),
                            ));
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
                    "admin" => {
                        if admin.is_some() {
                            return Err((
                                it.0,
                                format!(
                                    "There is already a {tee_or_mpc}-admin defined. Multiple {tee_or_mpc}-admin entries are not allowed."
                                ),
                            ));
                        }

                        admin = match tokens.next() {
                            Some((_, Token::Id(id))) => Ok(Some(id)),
                            Some((tc, _)) => {
                                Err((tc, "Unexpected argument. Only accepting IDs".to_string()))
                            }
                            None => return Err((tc, "Missing ID. Add an ID.".to_string())),
                        }?;

                        check_end_block(tokens.next())?;
                        continue;
                    }
                    "external-root-access" => {
                        if !external_root_access.is_empty() {
                            return Err((
                                it.0,
                                format!(
                                    "{tee_or_mpc}-external-root-access is already defined. To add multiple pools, append them in one declaration, E.g.: ({tee_or_mpc}-external-root-access @pool1 @pool2 ...)."
                                ),
                            ));
                        }
                        seen_external_root_access = true;
                        external_root_access = parse_optional_ids(&mut tokens)?;
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
                            return Err((
                                it.0,
                                "There is already a stroke-color defined".to_string(),
                            ));
                        }
                        let nt = tokens.next();
                        let color = check_color_format(&it, nt)?;
                        pe_bpmn_meta.stroke_color = Some(color);
                        check_end_block(tokens.next())?;
                        continue;
                    }
                    "fill-color" => {
                        if pe_bpmn_meta.fill_color.is_some() {
                            return Err((
                                it.0,
                                "There is already a fill-color defined".to_string(),
                            ));
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
            return Err((
                it.0,
                format!("{tee_or_mpc} does not allow this extension argument type"),
            ));
        } else {
            return Err((it.0, "Expected an extension argument".to_string()));
        }
    }

    if !seen_external_root_access {
        return Err((
            tc,
            "Missing required 'tee-external-root-access' statement (even if empty)".to_string(),
        ));
    }

    let computation_common = ComputationCommon {
        pebpmn_type,
        in_protect,
        in_unprotect,
        out_protect,
        out_unprotect,
        data_without_protection,
        data_already_protected,
        admin,
        external_root_access,
    };

    return match pe_bpmn_type {
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
        _ => return Err((tc, "Invalid pe-bpmn type for tee or mpc".to_string())),
    };
}

pub fn to_pe_bpmn(mut tokens: Tokens) -> AResult {
    // Use first token is determine extension type
    let first_token = tokens.next();

    if let Some((tc, Token::ExtensionArgument(extension_type))) = first_token {
        // Implement extension types
        match extension_type.as_str() {
            "secure-channel" => assemble_secure_channel(tokens, tc),
            "tee-pool" => assemble_tee_or_mpc(tokens, tc, PeBpmnType::Tee(Tee::default()), true),
            "tee-tasks" => assemble_tee_or_mpc(tokens, tc, PeBpmnType::Tee(Tee::default()), false),
            "mpc-pool" => assemble_tee_or_mpc(tokens, tc, PeBpmnType::Mpc(Mpc::default()), true),
            "mpc-tasks" => assemble_tee_or_mpc(tokens, tc, PeBpmnType::Mpc(Mpc::default()), false),
            _ => Err((tc, "Unknown pe-bpmn extension type".to_string())),
        }
    } else if let Some((tc, Token::Text(text))) = first_token {
        Err((
            tc,
            format!(
                "Found freeform text ({text}) which is not allowed in this position. Did you intend to start a new (...) block?"
            ),
        ))
    } else {
        Err((
            first_token
                .expect("Programming error: first token should be of type ExtensionArgument")
                .0,
            "Found wrong format text which is not allowed in this position. Did you intend to start a new (...) block?".to_string(),
        ))
    }
}

impl<'a> Lexer<'a> {
    pub fn start_extension(
        &mut self,
        tc: TokenCoordinate,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
                    return Err(self.sas.conv_err((tc, "Empty extension block. Make sure you complete the full \"[...]\" statement".to_string())));
                }
                Some(_) => {
                    // Read extension
                    let mut tc = self.current_coord();
                    let (tc_end, extension_type) =
                        self.read_label().map_err(|e| self.sas.conv_err(e))?;
                    // Check extension type
                    if extension_type == "pe-bpmn" {
                        self.sas.next_statement(tc, self.position, to_pe_bpmn)?;
                        tc = TokenCoordinate {
                            start: tc.start,
                            end: tc_end.end,
                        };
                        self.run_pe_bpmn(tc)?;
                        // Empty [pe-bpmn]
                        if self.sas.fragments.is_empty() {
                            return Err(self.sas.conv_err((tc, "Empty extension block. Make sure you complete the full \"[pe-bpmn...]\" statement".to_string())));
                        }
                        break;
                    }
                    return Err(self.sas.conv_err((
                        TokenCoordinate {
                            start: tc.start,
                            end: tc_end.end,
                        },
                        "Invalid extension".to_string(),
                    )));
                }
                None => {
                    return Err(self.sas.conv_err((tc, "Unfinished extension block. Make sure you complete the full \"[...]\" statement.".to_string())));
                }
            }
        }
        Ok(())
    }

    fn run_pe_bpmn(&mut self, mut tc: TokenCoordinate) -> Result<(), Box<dyn std::error::Error>> {
        self.skip_whitespace();

        let mut block_state = BlockState::Closed;
        loop {
            match self.current_char {
                Some('(') => {
                    let tc = self.current_coord();
                    if matches!(block_state, BlockState::Opened) {
                        return Err(self
                            .sas
                            .conv_err((tc, "Already inside an active block".to_string())));
                    }
                    block_state = BlockState::Opened;
                    self.advance();
                    let (tc_end, target) = self
                        .read_argument(false)
                        .map_err(|e| self.sas.conv_err(e))?;
                    self.sas
                        .add_fragment(tc, tc_end.end, Token::ExtensionArgument(target))?;
                }
                Some(')') => {
                    tc = self.current_coord();
                    if matches!(block_state, BlockState::Closed) {
                        return Err(self
                            .sas
                            .conv_err((tc, "There is no active block to close".to_string())));
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
                    let (tc_end, id) = self.read_label().map_err(|e| self.sas.conv_err(e))?;
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
                        return Err(self.sas.conv_err((tc, "Unfinished (...) block. Add a \")\" first to close the block before you can close the \"[...]\" statement.".to_string())));
                    }
                    self.advance();
                    break;
                }
                Some(_) => {
                    let tc = self.current_coord();
                    let (tc_end, argument) =
                        self.read_argument(true).map_err(|e| self.sas.conv_err(e))?;
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
                        };
                    }
                    if !matches!(block_state, BlockState::Closed) {
                        return Err(self.sas.conv_err((
                            tc,
                            "Unfinished (...) block. Add a \")\" to close the block.".to_string(),
                        )));
                    }
                    return Err(self.sas.conv_err((tc, "Unfinished extension block. Make sure you complete the full \"[...]\" statement.".to_string())));
                }
            }
        }
        Ok(())
    }

    pub fn read_argument(
        &mut self,
        is_text: bool,
    ) -> Result<(TokenCoordinate, String), LexerError> {
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
                Err((
                    coord_start,
                    "Forbidden text/symbol in this context".to_string(),
                ))
            } else {
                Err((
                    coord_start,
                    "This extension seems to be missing arguments. Consider adding a text, IDs (@id) etc".to_string(),
                ))
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
) -> Result<DataFlowAnnotationLexer, (TokenCoordinate, String)> {
    let id = match tokens.next() {
        Some((_, Token::Id(id))) => Ok(id),
        Some((tc, Token::Separator)) => Err((tc, "Missing ID. Add an ID.".to_string())),
        Some((tc, _)) => Err((tc, "Only accepting IDs.".to_string())),
        None => Err((tc, "Missing ID. Add an ID.".to_string())),
    }?;

    let arg = if (is_in && is_protected) || (!is_in && !is_protected) {
        Some(match tokens.next() {
            Some((_, Token::Id(id))) => Ok(Some(id)),
            Some((_, Token::Text(text))) if text == "no-rv" => Ok(None),
            Some((tc, Token::Separator)) => Err((
                tc,
                "Reference value missing. Add an ID or 'no-rv' before closing the block."
                    .to_string(),
            )),
            Some((tc, _)) => Err((
                tc,
                "Unknown argument. Expected an ID or 'no-rv'.".to_string(),
            )),
            None => Err((tc, "Missing second argument.".to_string())),
        }?)
    } else {
        None
    };

    check_end_block(tokens.next())?;

    let result = DataFlowAnnotationLexer {
        node: id,
        rv: arg.flatten(),
    };

    Ok(result)
}

fn check_end_block(
    token: Option<(TokenCoordinate, Token)>,
) -> Result<(), (TokenCoordinate, String)> {
    match token {
        Some((_, Token::Separator)) => Ok(()),
        Some((tc, _)) => Err((
            tc,
            "The block doesn't take any more arguments. Please remove it.".to_string(),
        )),
        None => Ok(()),
    }?;
    Ok(())
}

fn parse_id_or_placeholder(
    token: Option<(TokenCoordinate, Token)>,
    prev_tc: TokenCoordinate,
    placeholder: &str,
    actor: &str,
) -> Result<String, (TokenCoordinate, String)> {
    if let Some((tc, token)) = token {
        match token {
            Token::Id(id) => Ok(id),
            Token::Text(text) => {
                if text == placeholder {
                    Ok(String::new())
                } else {
                    Err((
                        tc,
                        format!("Unexpected argument. Did you mean \"{placeholder}\"?",),
                    ))
                }
            }
            Token::Separator => Err((
                tc,
                format!("Expected more arguments. Add a {actor} ID or \"{placeholder}\""),
            )),
            _ => Err((
                tc,
                format!("Unexpected argument. Did you mean \"{placeholder}\"?",),
            )),
        }
    } else {
        Err((
            prev_tc,
            format!("Missing {actor}. Add a {actor} ID or \"{placeholder}\"",),
        ))
    }
}

fn parse_optional_ids(
    tokens: &mut impl Iterator<Item = (TokenCoordinate, Token)>,
) -> Result<Vec<String>, (TokenCoordinate, String)> {
    let mut arguments_ids = Vec::new();
    for (tc, t) in tokens.by_ref() {
        match t {
            Token::Separator => {
                break;
            }
            Token::Id(id) => {
                arguments_ids.push(id);
            }
            _ => {
                return Err((tc, "Unexpected argument. Only accepting IDs".to_string()));
            }
        }
    }

    Ok(arguments_ids)
}

fn check_color_format(
    it: &(TokenCoordinate, Token),
    nt: Option<(TokenCoordinate, Token)>,
) -> Result<String, (TokenCoordinate, String)> {
    if let Some(meta) = nt {
        if let (tc2, Token::Text(text)) = meta {
            if is_valid_hex_color(&text) {
                Ok(text)
            } else {
                Err((
                tc2,
                "Invalid hex color format. Expected # followed by 3 or 6 hex digits (e.g., #ABC or #FFFFFF)"
                    .to_string(),
            ))
            }
        } else if let (_, Token::Separator) = meta {
            Err((
                it.0,
                "This extension seems to be missing a hex color argument (e.g #FFFFFF)."
                    .to_string(),
            ))
        } else {
            Err((
                TokenCoordinate {
                    start: it.0.start,
                    end: meta.0.end,
                },
                "This extension argument only accepts hex colors (e.g #FFFFFF)".to_string(),
            ))
        }
    } else {
        Err((
            it.0,
            "This extension seems to be missing a hex color argument (e.g #FFFFFF).".to_string(),
        ))
    }
}

pub fn is_allowed_symbol_in_args(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '#')
}

fn is_valid_hex_color(s: &str) -> bool {
    s.len() == 4 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
        || s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

use alloc::string::String;
use bpmd_parse::lexer::is_allowed_symbol_in_label_or_id;
use core::fmt::Write as _;
const STMT_START_SHORT: &[(&str, &str)] = &[
    (".", "end"),
    ("!", "end"),
    ("#", "event"),
    ("-", "activity"),
    ("X", "gateway"),
    ("+", "gateway"),
    ("O", "gateway"),
    ("*", "gateway"),
    ("&", "data continuation"),
];

const STMT_START_LONG: &[(&str, &str)] = &[
    (".-", "activity"),
    ("XG", "gateway"),
    ("+G", "gateway"),
    ("OG", "gateway"),
    ("*G", "gateway"),
    (".#", "event"),
    ("M#", "event"),
    ("T#", "event"),
    ("C#", "event"),
    (">#", "event"),
    ("S#", "event"),
    ("E#", "event"),
    ("^#", "event"),
    ("<#", "event"),
    ("X#", "event"),
    ("##", "event"),
    ("+#", "event"),
    ("..", "event"),
    ("M.", "event"),
    ("S.", "event"),
    ("E.", "event"),
    ("^.", "event"),
    ("<.", "event"),
    ("X.", "event"),
    ("#.", "event"),
    ("MF", "mf"),
    ("OD", "data"),
    ("SD", "data"),
    ("<D", "data"),
    (">D", "data"),
    ("O&", "data continuation"),
    ("S&", "data continuation"),
    ("<&", "data continuation"),
    (">&", "data continuation"),
    ("==", "header"),
    ("=", "pool"),
];

const BOUNDARY: &[(&str, &str)] = &[
    ("M!", "interrupt-boundary"),
    ("T!", "interrupt-boundary"),
    ("C!", "interrupt-boundary"),
    ("S!", "interrupt-boundary"),
    ("E!", "interrupt-boundary"),
    ("^!", "interrupt-boundary"),
    ("<!", "interrupt-boundary"),
    ("X!", "interrupt-boundary"),
    ("#!", "interrupt-boundary"),
    ("+!", "interrupt-boundary"),
    ("M!", "non-interrupt-boundary"),
    ("T!", "non-interrupt-boundary"),
    ("C!", "non-interrupt-boundary"),
    ("S!", "non-interrupt-boundary"),
    ("^!", "non-interrupt-boundary"),
    ("#!", "non-interrupt-boundary"),
    ("+!", "non-interrupt-boundary"),
];

const ATTRIBUTE_TYPE: &[(&str, &str)] = &[
    ("~throw", "throw"),
    ("~catch", "catch"),
    ("~send", "send"),
    ("~receive", "receive"),
    ("~manual", "manual"),
    ("~user", "user"),
    ("~script", "script"),
    ("~service", "service"),
    ("~businessrule", "businessrule"),
    ("~multiple", "multiple"),
    ("~loop", "loop"),
    ("~adhoc", "adhoc"),
    ("~compensation", "compensation"),
    ("~blackbox", "blackbox"),
];

const ATTRIBUTE_ARROW: &[(&str, &str)] = &[("<-", "arrowleft"), ("->", "arrowright")];

const ATTRIBUTE_ID: &[(&str, &str)] = &[("@", "id")];

const COMMENT: &[(&str, &str)] = &[("//", "comment_start")];

const EXTENSION_START: &[(&str, &str)] = &[("[", "extension-start")];
const EXTENSION_END: &[(&str, &str)] = &[("]", "extension-end")];
const GROUPING: &[(&str, &str)] = &[("(", "grouping-start"), (")", "grouping-end")];
const EXTENSION_KEYWORDS1: &[(&str, &str)] = &[
    ("pe-bpmd", "pe-bpmd"),
    ("import", "import"),
    ("place", "place"),
    ("blackbox", "blackbox"),
    ("unblackbox", "unblackbox"),
    ("secure-channel", "secure-channel"),
    ("tee-tasks", "tee-tasks"),
    ("tee-lane", "tee-lane"),
    ("tee-pool", "tee-pool"),
    ("mpc-tasks", "mpc-tasks"),
    ("mpc-lane", "mpc-lane"),
    ("mpc-pool", "mpc-pool"),
    ("tee-external-root-access", "tee-external-root-access"),
    ("tee-in-protect", "tee-in-protect"),
    ("tee-out-unprotect", "tee-out-unprotect"),
    ("tee-in-unprotect", "tee-in-unprotect"),
    ("tee-out-protect", "tee-out-protect"),
    ("mpc-in-protect", "mpc-in-protect"),
    ("mpc-out-unprotect", "mpc-out-unprotect"),
    ("mpc-in-unprotect", "mpc-in-unprotect"),
    ("mpc-out-protect", "mpc-out-protect"),
    ("tee-hardware-operators", "tee-hardware-operators"),
    ("tee-software-operators", "tee-software-operators"),
    ("stroke-color", "stroke-color"),
    ("fill-color", "fill-color"),
    ("tee-data-without-protection", "tee-data-without-protection"),
    ("tee-already-protected", "tee-already-protected"),
    ("tee-already-protected", "tee-already-protected"),
    ("mpc-data-without-protection", "mpc-data-without-protection"),
    ("mpc-already-protected", "mpc-already-protected"),
    ("mpc-already-protected", "mpc-already-protected"),
];
const EXTENSION_KEYWORDS2: &[(&str, &str)] = &[
    ("blocked", "blocked"),
    ("containing-pool-only", "containing-pool-only"),
    ("before", "before"),
    ("or", "or"),
    ("above", "above"),
    ("below", "below"),
    ("of", "of"),
];
const HASH_FOR_COLORS: &[(&str, &str)] = &[("#", "hash")];

pub fn highlight_text_for_inner_html(mut text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 8);
    let mut allow_new_statement = true;
    let mut allow_display_text = true;
    let mut allow_attributes = false;
    let mut old_len = None;
    let mut within_extension = false;
    for i in 0..10000 {
        eat_whitespace(&mut text, &mut out, &mut allow_new_statement);
        if let Some(old_len) = old_len
            && old_len == text.len()
        {
            eat_next_unexpected_word(&mut text, &mut out, "illegal_word");
            eat_whitespace(&mut text, &mut out, &mut allow_new_statement);
            if old_len == text.len() {
                log::error!("Syntax Highlighting failed - no progress made. Iterations: {i}");
                out.push_str(text);
                return out;
            }
        }
        old_len = Some(text.len());

        if allow_new_statement {
            log::error!("before stmt");
            if try_match(STMT_START_LONG, &mut text, &mut out, "stmt stmt-long")
                || try_match(BOUNDARY, &mut text, &mut out, "boundary")
                || try_match(STMT_START_SHORT, &mut text, &mut out, "stmt stmt-short")
            {
                log::error!("in stmt");
                allow_new_statement = false;
                allow_display_text = true;
                allow_attributes = true;
                // Could be that the extension is currently written. To avoid breaking syntax in
                // funny ways,
                within_extension = false;
            } else if try_match(EXTENSION_START, &mut text, &mut out, "") {
                within_extension = true;
                allow_attributes = true;
                allow_display_text = false;
                allow_new_statement = false;
            }
        }

        log::error!("before with ext");
        if within_extension {
            if try_match(HASH_FOR_COLORS, &mut text, &mut out, "") {
                allow_new_statement = false;
                eat_next_word(&mut text, &mut out, "hash-label");
                eat_whitespace(&mut text, &mut out, &mut allow_new_statement);
            }
            if try_match(GROUPING, &mut text, &mut out, "grouping")
                || try_match(
                    EXTENSION_KEYWORDS1,
                    &mut text,
                    &mut out,
                    "extension-keyword1",
                )
                || try_match(
                    EXTENSION_KEYWORDS2,
                    &mut text,
                    &mut out,
                    "extension-keyword2",
                )
            {
                allow_new_statement = false;
                eat_whitespace(&mut text, &mut out, &mut allow_new_statement);
            }
            if try_match(EXTENSION_END, &mut text, &mut out, "") {
                within_extension = false;
                eat_whitespace(&mut text, &mut out, &mut allow_new_statement);
            }
        }

        if allow_attributes && try_match(ATTRIBUTE_TYPE, &mut text, &mut out, "attribute type") {
            allow_display_text = false;
            allow_new_statement = false;
            eat_whitespace(&mut text, &mut out, &mut allow_new_statement);
        }

        if allow_attributes && try_match(ATTRIBUTE_ID, &mut text, &mut out, "attribute id") {
            allow_display_text = false;
            allow_new_statement = false;
            eat_next_word(&mut text, &mut out, "id-label");
            eat_whitespace(&mut text, &mut out, &mut allow_new_statement);
        }

        // Often arrows are next to each other, so loop this one.
        while allow_attributes && try_match(ATTRIBUTE_ARROW, &mut text, &mut out, "attribute arrow")
        {
            eat_next_word(&mut text, &mut out, "label");
            eat_next_quoted_words(&mut text, &mut out);
            allow_display_text = false;
            allow_new_statement = false;
            eat_whitespace(&mut text, &mut out, &mut allow_new_statement);
        }

        if try_match(COMMENT, &mut text, &mut out, "comment") {
            eat_rest_of_the_line(&mut text, &mut out, "comment");
            eat_whitespace(&mut text, &mut out, &mut allow_new_statement);
            // Need not process from here any further, just go to the new line.
            continue;
        }

        // Must come at the end to not eat any attributes or the comment.
        if allow_display_text {
            eat_next_word(&mut text, &mut out, "display_text");
            allow_attributes = true;
            allow_new_statement = false;
            eat_whitespace(&mut text, &mut out, &mut allow_new_statement);
        }

        if text.is_empty() {
            return out;
        }
        log::error!("at end");
    }

    log::error!("Syntax Highlighting failed - loop finished");
    out.push_str(text);
    out
}

// Written by ChatGPT 2026-10-09.
fn eat_whitespace(text: &mut &str, output: &mut String, allow_new_statement: &mut bool) {
    let trimmed = text.trim_start_matches(char::is_whitespace);
    let whitespace_len = text.len() - trimmed.len();
    let whitespace = &text[..whitespace_len];

    if whitespace.contains(['\n', '\r']) {
        *allow_new_statement = true;
    }

    log::error!(
        "eat_whitespace: text len: {}, trimmed len: {}",
        text.len(),
        trimmed.len()
    );
    output.push_str(whitespace);
    *text = trimmed;
}

fn try_match(group: &[(&str, &str)], text: &mut &str, out: &mut String, css_class: &str) -> bool {
    for (statement, additional_css_classes) in group {
        if let Some(rest) = text.strip_prefix(statement) {
            write!(
                out,
                "<span class=\"{css_class} {additional_css_classes}\">{statement}</span>"
            )
            .unwrap();
            *text = rest;
            return true;
        }
    }
    false
}

fn eat_rest_of_the_line(text: &mut &str, out: &mut String, css_class: &str) {
    let trimmed = text.trim_start_matches(|c: char| c != '\n' && c != '\r');
    let line_len = text.len() - trimmed.len();
    if line_len > 0 {
        let line = &text[..line_len];
        write!(out, "<span class=\"{css_class}\">{line}</span>").unwrap();
        *text = trimmed;
    }
}

fn eat_next_unexpected_word(text: &mut &str, out: &mut String, css_class: &str) -> bool {
    let trimmed = text.trim_start_matches(|c: char| !c.is_whitespace());
    let word_len = text.len() - trimmed.len();
    if word_len > 0 {
        let word = &text[..word_len];
        write!(out, "<span class=\"{css_class}\">{word}</span>").unwrap();
        *text = trimmed;
        true
    } else {
        false
    }
}

fn eat_next_word(text: &mut &str, out: &mut String, css_class: &str) -> bool {
    let trimmed = text.trim_start_matches(is_allowed_symbol_in_label_or_id);
    let word_len = text.len() - trimmed.len();
    if word_len > 0 {
        let word = &text[..word_len];
        write!(out, "<span class=\"{css_class}\">{word}</span>").unwrap();
        *text = trimmed;
        true
    } else {
        false
    }
}

fn eat_next_quoted_words(text: &mut &str, out: &mut String) -> bool {
    let Some(rest) = text.strip_prefix('"') else {
        return false;
    };
    write!(out, "<span class=\"quote_symbol quote_start\">\"</span>").unwrap();
    let rest2 = rest.trim_start_matches(|c: char| c != '\n' && c != '\r' && c != '"');
    let quoted_words_len = rest.len() - rest2.len();
    if quoted_words_len > 0 {
        let quoted_words = &rest[..quoted_words_len];
        write!(out, "<span class=\"quoted_words\">{quoted_words}</span>").unwrap();
    }
    if let Some(rest3) = rest2.strip_prefix('"') {
        write!(out, "<span class=\"quote_symbol quote_end\">\"</span>").unwrap();
        *text = rest3;
    } else {
        *text = rest2;
    }
    true
}

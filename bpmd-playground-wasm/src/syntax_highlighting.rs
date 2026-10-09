use alloc::format;
use alloc::string::String;
use core::fmt::Write as _;
const STMT_START: &[(&str, &str)] = &[
    (".", "end short"),
    ("!", "end short"),
    ("#", "event short"),
    ("-", "activity short"),
    ("MF", "mf"),
    ("OD", "data"),
    ("SD", "data"),
    ("<D", "data"),
    (">D", "data"),
    ("=", "pool"),
    ("==", "header"),
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
];

const ARROW: &[(&str, &str)] = &[("<-", "arrowleft"), ("->", "arrowright")];

pub fn highlight_text_for_inner_html(mut text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 8);
    let mut allow_new_statement = true;
    loop {
        if allow_new_statement {
            if try_match(STMT_START, &mut text, &mut out, "stmt") {}
        }
        allow_new_statement = false;

        'attributes: loop {
            eat_whitespace(&mut text, &mut out, &mut allow_new_statement);
            if allow_new_statement {
                // Basically a `continue`.
                break;
            }
            if try_match(ATTRIBUTE_TYPE, &mut text, &mut out, "attribute type") {
                continue;
            }
            if try_match(ARROW, &mut text, &mut out, "arrow") {
                try match the label
                continue;
            }

            break 'attributes;
        }
        if text.is_empty() {
            break;
        }
    }

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

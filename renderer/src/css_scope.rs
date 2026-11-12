//! Scope a guest stylesheet to its own tab.
//!
//! Every SDK styles its app with one injected `<style>` element. Left as-is,
//! that sheet applies to the whole document, including the host's tab strip,
//! address bar and console, so a guest could hide or restyle (spoof) the
//! browser chrome. The host therefore never lets a guest `<style>` become a
//! live stylesheet (see abi.rs): it reads the text, runs it through
//! [`scope_stylesheet`], and loads the result into a host-owned sheet.
//!
//! The rewrite:
//! * prefixes every selector with `prefix` (which matches only the tab's mount
//!   node). `:root`, `html`, `body`, `:scope` and `&` at the start of a
//!   selector become the mount itself, so `body { margin: 0 }` keeps working;
//! * recurses into `@media`, `@supports`, `@container`, `@layer` and
//!   `@starting-style` blocks;
//! * keeps `@keyframes`, `@font-face`, `@property`, `@counter-style`, `@page`,
//!   `@font-feature-values` and `@font-palette-values` verbatim (no selectors);
//! * drops everything else (`@import`, `@namespace`, `@scope`, unknown rules)
//!   and any style rule whose body nests one of those.
//!
//! Tokenizing uses `cssparser`, the same tokenizer Stylo uses, so block
//! structure here agrees with the engine. As a second line of defense the
//! output is re-parsed and every selector is checked to start with `prefix`;
//! if any doesn't, the whole sheet is discarded (fail closed).

use cssparser::{ParseError, Parser, ParserInput, Token};

type PResult<'i, T> = Result<T, ParseError<'i, ()>>;

const RECURSE_AT_RULES: &[&str] = &["media", "supports", "container", "layer", "starting-style"];
const VERBATIM_AT_RULES: &[&str] = &[
    "keyframes",
    "-webkit-keyframes",
    "font-face",
    "property",
    "counter-style",
    "page",
    "font-feature-values",
    "font-palette-values",
];
const FORBIDDEN_NESTED_AT_RULES: &[&str] = &["import", "namespace", "scope"];
const ROOT_ALIASES: &[&str] = &[":root", "html", "body", ":scope", "&"];

/// Turn a guest `<style>`'s raw text into the text of the host's shadow
/// `<style>`. Blitz HTML-entity-decodes `<style>` text before parsing it
/// (`process_style_element`), so we decode first (keeping guest semantics),
/// scope, then escape every `&` so Blitz's decode reproduces our output
/// byte-for-byte. Without that, `&#125;` would be read as `}` *after*
/// scoping and could close a rule early, letting an unscoped rule through.
pub fn scoped_style_text(raw: &str, prefix: &str) -> String {
    let decoded = html_escape::decode_html_entities(raw);
    scope_stylesheet(&decoded, prefix).replace('&', "&amp;")
}

/// Rewrite `css` so it can only match inside the element `prefix` selects.
pub fn scope_stylesheet(css: &str, prefix: &str) -> String {
    let mut input = ParserInput::new(css);
    let mut p = Parser::new(&mut input);
    let mut out = String::with_capacity(css.len() + css.len() / 4);
    rule_list(&mut p, prefix, &mut out);
    if verify(&out, prefix) {
        out
    } else {
        crate::logger::log("css", "guest stylesheet failed scope verification; discarded");
        String::new()
    }
}

/// Next token, raw: a returned `(`/`[`/function block is NOT consumed yet.
fn next_raw(p: &mut Parser<'_, '_>) -> Option<Token<'static>> {
    p.next_including_whitespace_and_comments()
        .ok()
        .map(|t| t.clone().into_owned_token())
}

/// Next token, with any `(`/`[`/function block consumed through its closing
/// bracket. cssparser only skips an unparsed block on the *following* call,
/// so without this `position()` would still point just past the opener and
/// slices ending at a block (`:not(.a)`, `[href]`, `@media (...)`) would be
/// cut short. `{}` blocks are left to the caller.
fn next(p: &mut Parser<'_, '_>) -> Option<Token<'static>> {
    let tok = next_raw(p)?;
    if matches!(tok, Token::ParenthesisBlock | Token::SquareBracketBlock | Token::Function(_)) {
        let _ = p.parse_nested_block(|b| -> PResult<()> {
            while b.next_including_whitespace_and_comments().is_ok() {}
            Ok(())
        });
    }
    Some(tok)
}

trait IntoOwned {
    fn into_owned_token(self) -> Token<'static>;
}

impl IntoOwned for Token<'_> {
    // Only the variants we inspect need their payload; everything else is
    // matched structurally, so a cheap stand-in keeps lifetimes simple.
    fn into_owned_token(self) -> Token<'static> {
        match self {
            Token::AtKeyword(s) => Token::AtKeyword(s.as_ref().to_string().into()),
            Token::WhiteSpace(_) => Token::WhiteSpace(" "),
            Token::Comment(_) => Token::Comment(""),
            Token::Semicolon => Token::Semicolon,
            Token::Comma => Token::Comma,
            Token::CurlyBracketBlock => Token::CurlyBracketBlock,
            Token::ParenthesisBlock => Token::ParenthesisBlock,
            Token::SquareBracketBlock => Token::SquareBracketBlock,
            Token::Function(s) => Token::Function(s.as_ref().to_string().into()),
            Token::CloseCurlyBracket => Token::CloseCurlyBracket,
            Token::CDO => Token::CDO,
            Token::CDC => Token::CDC,
            _ => Token::Delim('?'),
        }
    }
}

/// Scope a list of rules (top level, or the body of @media and friends).
fn rule_list(p: &mut Parser<'_, '_>, prefix: &str, out: &mut String) {
    loop {
        let start = p.position();
        let Some(tok) = next(p) else { return };
        match tok {
            Token::WhiteSpace(_) | Token::Comment(_) | Token::CDO | Token::CDC => {}
            // Stray `;` / `}` at rule-list level: drop them.
            Token::Semicolon | Token::CloseCurlyBracket => {}
            Token::AtKeyword(name) => at_rule(p, &name.to_ascii_lowercase(), prefix, out),
            first => qualified_rule(p, start, first, prefix, out),
        }
    }
}

fn at_rule(p: &mut Parser<'_, '_>, name: &str, prefix: &str, out: &mut String) {
    let prelude_start = p.position();
    let prelude_end;
    let mut has_block = false;
    loop {
        let before = p.position();
        match next(p) {
            None => {
                prelude_end = before;
                break;
            }
            Some(Token::Semicolon) => {
                prelude_end = before;
                break;
            }
            Some(Token::CurlyBracketBlock) => {
                prelude_end = before;
                has_block = true;
                break;
            }
            Some(_) => {}
        }
    }
    let prelude = p.slice(prelude_start..prelude_end).to_string();

    if !has_block {
        // Only the `@layer a, b;` ordering statement is harmless.
        if name == "layer" {
            out.push_str("@layer");
            out.push_str(&prelude);
            out.push(';');
        }
        return;
    }

    if RECURSE_AT_RULES.contains(&name) {
        let mut inner = String::new();
        let _ = p.parse_nested_block(|b| -> PResult<()> {
            rule_list(b, prefix, &mut inner);
            Ok(())
        });
        out.push('@');
        out.push_str(name);
        out.push_str(&prelude);
        out.push('{');
        out.push_str(&inner);
        out.push('}');
    } else if VERBATIM_AT_RULES.contains(&name) {
        let mut forbidden = false;
        let body = verbatim_block(p, &mut forbidden);
        if !forbidden {
            out.push('@');
            out.push_str(name);
            out.push_str(&prelude);
            out.push('{');
            out.push_str(&body);
            out.push('}');
        }
    } else {
        // Unknown or dangerous (@import, @namespace, @scope, ...): consume and drop.
        let mut forbidden = false;
        let _ = verbatim_block(p, &mut forbidden);
    }
}

/// Consume the block just opened and return its source text, flagging any
/// nested at-rule we refuse to pass through.
fn verbatim_block(p: &mut Parser<'_, '_>, forbidden: &mut bool) -> String {
    p.parse_nested_block(|b| -> PResult<String> {
        let start = b.position();
        scan(b, forbidden);
        Ok(b.slice_from(start).to_string())
    })
    .unwrap_or_default()
}

fn scan(p: &mut Parser<'_, '_>, forbidden: &mut bool) {
    while let Some(tok) = next_raw(p) {
        match tok {
            Token::AtKeyword(name) => {
                if FORBIDDEN_NESTED_AT_RULES.contains(&name.to_ascii_lowercase().as_str()) {
                    *forbidden = true;
                }
            }
            Token::CurlyBracketBlock
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::Function(_) => {
                let _ = p.parse_nested_block(|b| -> PResult<()> {
                    scan(b, forbidden);
                    Ok(())
                });
            }
            _ => {}
        }
    }
}

fn qualified_rule(
    p: &mut Parser<'_, '_>,
    start: cssparser::SourcePosition,
    first: Token<'static>,
    prefix: &str,
    out: &mut String,
) {
    let mut selectors: Vec<String> = Vec::new();
    let mut seg_start = start;
    let mut tok = Some(first);
    let mut before = start;
    loop {
        match tok {
            None => return, // EOF before `{`: not a rule
            Some(Token::Comma) => {
                selectors.push(p.slice(seg_start..before).to_string());
                seg_start = p.position();
            }
            Some(Token::CurlyBracketBlock) => {
                selectors.push(p.slice(seg_start..before).to_string());
                break;
            }
            Some(_) => {}
        }
        before = p.position();
        tok = next(p);
    }

    let mut forbidden = false;
    let body = verbatim_block(p, &mut forbidden);
    if forbidden {
        return;
    }
    let scoped: Vec<String> = selectors
        .iter()
        .filter_map(|s| scope_selector(s, prefix))
        .collect();
    if scoped.is_empty() {
        return;
    }
    out.push_str(&scoped.join(","));
    out.push('{');
    out.push_str(&body);
    out.push('}');
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '\\' || !c.is_ascii()
}

/// Prefix one complex selector. Returns None for an empty selector.
fn scope_selector(sel: &str, prefix: &str) -> Option<String> {
    let mut rest = sel.trim_start_matches(|c: char| c.is_ascii_whitespace());
    if rest.trim().is_empty() {
        return None;
    }
    let mut matched_root = false;
    loop {
        let alias = ROOT_ALIASES.iter().find(|a| {
            rest.len() >= a.len()
                && rest.is_char_boundary(a.len())
                && rest[..a.len()].eq_ignore_ascii_case(a)
                && !rest[a.len()..].starts_with(|c: char| a.starts_with(|f: char| f.is_ascii_alphabetic()) && is_ident_char(c))
        });
        let Some(alias) = alias else { break };
        matched_root = true;
        rest = &rest[alias.len()..];
        // `body.dark`, `html[lang]`, `:root:hover` -> compound on the mount.
        if rest.starts_with(['.', '#', '[', ':']) {
            return Some(format!("{prefix}{rest}"));
        }
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace());
        if let Some(r) = rest.strip_prefix('>') {
            rest = r.trim_start_matches(|c: char| c.is_ascii_whitespace());
        }
    }
    if rest.trim().is_empty() {
        return if matched_root { Some(prefix.to_string()) } else { None };
    }
    Some(format!("{prefix} {rest}"))
}

/// Re-parse scoped output; every selector at every rule-list level must begin
/// with `prefix`, and only allowed at-rules may appear.
fn verify(css: &str, prefix: &str) -> bool {
    let mut input = ParserInput::new(css);
    let mut p = Parser::new(&mut input);
    verify_list(&mut p, prefix)
}

fn verify_list(p: &mut Parser<'_, '_>, prefix: &str) -> bool {
    loop {
        let start = p.position();
        let Some(tok) = next(p) else { return true };
        match tok {
            Token::WhiteSpace(_) | Token::Comment(_) => {}
            Token::AtKeyword(name) => {
                let name = name.to_ascii_lowercase();
                let mut ok = true;
                loop {
                    match next(p) {
                        None | Some(Token::Semicolon) => {
                            if name != "layer" {
                                return false;
                            }
                            break;
                        }
                        Some(Token::CurlyBracketBlock) => {
                            if RECURSE_AT_RULES.contains(&name.as_str()) {
                                ok = p
                                    .parse_nested_block(|b| -> PResult<bool> { Ok(verify_list(b, prefix)) })
                                    .unwrap_or(false);
                            } else if VERBATIM_AT_RULES.contains(&name.as_str()) {
                                let mut forbidden = false;
                                let _ = verbatim_block(p, &mut forbidden);
                                ok = !forbidden;
                            } else {
                                ok = false;
                            }
                            break;
                        }
                        Some(_) => {}
                    }
                }
                if !ok {
                    return false;
                }
            }
            _ => {
                // A qualified rule: check each selector before the `{`.
                let mut seg_start = start;
                let mut before = start;
                let mut t = Some(tok);
                loop {
                    match t {
                        None => return false,
                        Some(Token::Comma) | Some(Token::CurlyBracketBlock) => {
                            let seg = p.slice(seg_start..before).trim_start();
                            if !seg.starts_with(prefix) {
                                return false;
                            }
                            if matches!(t, Some(Token::CurlyBracketBlock)) {
                                break;
                            }
                            seg_start = p.position();
                        }
                        Some(_) => {}
                    }
                    before = p.position();
                    t = next(p);
                }
                let mut forbidden = false;
                let _ = verbatim_block(p, &mut forbidden);
                if forbidden {
                    return false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = ".gwb-scope-t3>*";

    fn s(css: &str) -> String {
        scope_stylesheet(css, P)
    }

    #[test]
    fn prefixes_plain_selectors() {
        assert_eq!(s(".a{color:red}"), ".gwb-scope-t3>* .a{color:red}");
        assert_eq!(s(".a, #b > c {x:y}"), ".gwb-scope-t3>* .a,.gwb-scope-t3>* #b > c {x:y}");
        assert_eq!(s("* {box-sizing:border-box}"), ".gwb-scope-t3>* * {box-sizing:border-box}");
    }

    #[test]
    fn root_aliases_map_to_mount() {
        assert_eq!(s("body{margin:0}"), ".gwb-scope-t3>*{margin:0}");
        assert_eq!(s(":root{--x:1}"), ".gwb-scope-t3>*{--x:1}");
        assert_eq!(s("html body .x{a:b}"), ".gwb-scope-t3>* .x{a:b}");
        assert_eq!(s("html > body{a:b}"), ".gwb-scope-t3>*{a:b}");
        assert_eq!(s("body.dark .x{a:b}"), ".gwb-scope-t3>*.dark .x{a:b}");
        assert_eq!(s("HTML{a:b}"), ".gwb-scope-t3>*{a:b}");
        // Not aliases: element names that merely start with one.
        assert_eq!(s("bodyx{a:b}"), ".gwb-scope-t3>* bodyx{a:b}");
        assert_eq!(s("html-thing{a:b}"), ".gwb-scope-t3>* html-thing{a:b}");
    }

    #[test]
    fn recurses_into_conditional_rules() {
        assert_eq!(
            s("@media (max-width: 600px){.a{x:y} body{m:0}}"),
            "@media (max-width: 600px){.gwb-scope-t3>* .a{x:y}.gwb-scope-t3>*{m:0}}"
        );
        assert_eq!(
            s("@supports (display:grid){@media screen{.a{x:y}}}"),
            "@supports (display:grid){@media screen{.gwb-scope-t3>* .a{x:y}}}"
        );
    }

    #[test]
    fn real_world_selectors_survive() {
        // Selectors/preludes ending in (), [] or functions must not be cut
        // short (and then discarded by the verifier).
        assert_eq!(
            s("a[href^='x']:not(.b), :is(.c,.d) > e{f:g}"),
            ".gwb-scope-t3>* a[href^='x']:not(.b),.gwb-scope-t3>* :is(.c,.d) > e{f:g}"
        );
        assert_eq!(s("[data-x]{y:z}"), ".gwb-scope-t3>* [data-x]{y:z}");
        assert_eq!(s(":is(.a, #ab-input){display:none}"), ".gwb-scope-t3>* :is(.a, #ab-input){display:none}");
        assert_eq!(
            s("@media (min-width: 1px) and (max-width: 9px){li:nth-child(2n+1){a:b}}"),
            "@media (min-width: 1px) and (max-width: 9px){.gwb-scope-t3>* li:nth-child(2n+1){a:b}}"
        );
        assert_eq!(s("body:not(.x) .y{a:b}"), ".gwb-scope-t3>*:not(.x) .y{a:b}");
    }

    #[test]
    fn keeps_selectorless_at_rules_verbatim() {
        assert_eq!(s("@keyframes spin{from{a:b}to{c:d}}"), "@keyframes spin{from{a:b}to{c:d}}");
        assert_eq!(s("@font-face{font-family:x;src:url(a.woff)}"), "@font-face{font-family:x;src:url(a.woff)}");
        assert_eq!(s("@layer base, ui;"), "@layer base, ui;");
    }

    #[test]
    fn drops_dangerous_at_rules() {
        assert_eq!(s("@import url(http://evil/x.css);.a{x:y}"), ".gwb-scope-t3>* .a{x:y}");
        assert_eq!(s("@namespace svg url(x);"), "");
        assert_eq!(s("@scope (:root) { #ab-input{display:none} }"), "");
        assert_eq!(s("@unknown { #ab-input{display:none} }"), "");
        assert_eq!(s(".a{ @scope (:root){ #x{y:z} } }"), "");
    }

    #[test]
    fn keeps_nested_rules_inside_scoped_parent() {
        assert_eq!(s(".a{color:red; &:hover{color:blue}}"), ".gwb-scope-t3>* .a{color:red; &:hover{color:blue}}");
    }

    #[test]
    fn brace_and_string_tricks_stay_scoped() {
        for css in [
            "} #ab-input{display:none}",
            ".x{background:url(a}b)} #ab-input{display:none}",
            ".x{content:\"}\"} #ab-input{display:none}",
            ".x{content:'\\'}'} #ab-input{display:none}",
            "/* } */ #ab-input{display:none}",
            ".x{a:b",
            ".x{a:\"unterminated",
            ".x\\{ #ab-input{display:none}",
            "a\\ , #ab-input{display:none}",
            "<!-- #ab-input{display:none} -->",
            ".x{} ;;; #ab-input{display:none}",
            "@media screen{} } #ab-input{display:none}",
            "@media screen{ } #ab-input{display:none}",
            ":is(.a, #ab-input){display:none}",
        ] {
            let out = s(css);
            // Every selector we emitted must be scoped (re-parsed with the
            // engine's tokenizer), and no rule may start with the target.
            assert!(verify(&out, P), "unscoped output for {css:?}: {out:?}");
            for bad in ["}#ab-input", "} #ab-input", ";#ab-input"] {
                assert!(!out.contains(bad), "{css:?} -> {out:?}");
            }
            assert!(!out.trim_start().starts_with("#ab-input"), "{css:?} -> {out:?}");
        }
    }

    #[test]
    fn entities_cannot_smuggle_braces() {
        for raw in [
            ".x{a:b&#125; #ab-input{display:none}",
            ".x{a:b&amp;#125; #ab-input{display:none}",
            ".x{a:b&rcub; #ab-input{display:none}",
            ".a{color:red; &:hover{color:blue}}",
        ] {
            let shadow = scoped_style_text(raw, P);
            // What the engine will actually parse:
            let engine_sees = html_escape::decode_html_entities(&shadow).into_owned();
            assert!(verify(&engine_sees, P), "{raw:?} -> engine sees {engine_sees:?}");
            assert_eq!(engine_sees, scope_stylesheet(&html_escape::decode_html_entities(raw), P));
        }
    }

    #[test]
    fn verify_rejects_unscoped() {
        assert!(!verify("#ab-input{display:none}", P));
        assert!(!verify("@import url(x);", P));
        assert!(verify(".gwb-scope-t3>* .a{b:c}", P));
    }
}

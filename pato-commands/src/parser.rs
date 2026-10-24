//! Argument parsing for chat commands, ported from the TypeScript `parseArgs`
//! this design came from.
//!
//! A command's arguments are described by one template string of
//! space-separated placeholders: `<name:type>` is required, `[name:type]` is
//! optional, `type` is `string`, `number` or `boolean`, and a trailing `...`
//! on a *string* placeholder swallows the rest of the line. For example
//! `<user:string> [amount:number] <reason:string...>`.
//!
//! `validate_template` runs when a command is saved or registered (so bad
//! templates never reach dispatch); `parse_args` runs on each invocation.
//! Both report problems as human-readable strings, which are shown to the
//! streamer (template errors) or sent to chat (argument errors) as-is.
//!
//! Deliberately basic, like the original: positional only, no quoting, no
//! flags. Differences from the TypeScript version are all tightenings:
//! empty input yields no arguments (JS turned `""` into one empty arg, and
//! `Number("")` is 0), numbers must be plain finite decimals (no `0x10`),
//! the spread argument keeps the user's original spacing, and templates
//! with a required placeholder after an optional one, or a duplicate name,
//! are rejected.

use std::collections::HashSet;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgType {
    String,
    Number,
    Boolean,
}

impl ArgType {
    fn name(self) -> &'static str {
        match self {
            ArgType::String => "string",
            ArgType::Number => "number",
            ArgType::Boolean => "boolean",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub name: String,
    pub ty: ArgType,
    pub required: bool,
    /// Only ever on the last part, and only for strings.
    pub spread: bool,
}

impl Part {
    /// How the placeholder is written in a template, e.g. `<name:string>`.
    pub fn literal(&self) -> String {
        let (open, close) = if self.required { ('<', '>') } else { ('[', ']') };
        let dots = if self.spread { "..." } else { "" };
        format!("{open}{}:{}{dots}{close}", self.name, self.ty.name())
    }

    /// `<name>` / `[name]`, used in error messages.
    fn placeholder(&self) -> String {
        if self.required {
            format!("<{}>", self.name)
        } else {
            format!("[{}]", self.name)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Num(f64),
    Flag(bool),
}

/// Parses and checks a template. An empty template is valid: the command
/// takes no arguments.
pub fn validate_template(template: &str) -> Result<Vec<Part>, String> {
    let mut parts: Vec<Part> = Vec::new();
    for token in template.split_whitespace() {
        parts.push(parse_part(token)?);
    }
    if let Some(i) = parts.iter().position(|p| p.spread) {
        if i != parts.len() - 1 {
            return Err("Invalid template: spread only valid in last position".into());
        }
    }
    let mut seen = HashSet::new();
    let mut optional_seen = false;
    for part in &parts {
        if !seen.insert(part.name.as_str()) {
            return Err(format!("Invalid template: duplicate name {}", part.name));
        }
        if part.required && optional_seen {
            return Err(format!(
                "Invalid template: required {} cannot follow an optional argument",
                part.placeholder()
            ));
        }
        optional_seen |= !part.required;
    }
    Ok(parts)
}

/// `(<|[)\w+:(string|number|boolean)(...)?(>|])`, matched against the whole
/// token.
fn parse_part(token: &str) -> Result<Part, String> {
    let invalid = || "Invalid template".to_string();
    let mut chars = token.chars();
    let open = chars.next().ok_or_else(invalid)?;
    let close = chars.next_back().ok_or_else(invalid)?;
    if !matches!(open, '<' | '[') || !matches!(close, '>' | ']') {
        return Err(invalid());
    }
    if (open == '<') != (close == '>') {
        return Err("Invalid template: brackets must match".into());
    }
    let inner = chars.as_str();
    let (name, ty) = inner.split_once(':').ok_or_else(invalid)?;
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(invalid());
    }
    let (ty, spread) = match ty.strip_suffix("...") {
        Some(ty) => (ty, true),
        None => (ty, false),
    };
    let ty = match ty {
        "string" => ArgType::String,
        "number" => ArgType::Number,
        "boolean" => ArgType::Boolean,
        _ => return Err(invalid()),
    };
    if spread && ty != ArgType::String {
        return Err("Invalid template: spread only valid for strings".into());
    }
    Ok(Part {
        name: name.to_string(),
        ty,
        required: open == '<',
        spread,
    })
}

/// Parses the text after the command name against already-validated `parts`.
/// Returns the values in template order; omitted optional arguments are
/// absent.
pub fn parse_args(parts: &[Part], raw: &str) -> Result<Vec<(String, Value)>, String> {
    let mut result = Vec::new();
    let mut used = 0;
    for (idx, (start, token)) in tokens(raw).enumerate() {
        let Some(part) = parts.get(idx) else {
            return Err("Expected fewer arguments".into());
        };
        used = idx + 1;
        let value = match part.ty {
            ArgType::String if part.spread => {
                // Rest of the line, original spacing preserved.
                result.push((part.name.clone(), Value::Text(raw[start..].trim_end().to_string())));
                break;
            }
            ArgType::String => Value::Text(token.to_string()),
            ArgType::Number => match f64::from_str(token) {
                Ok(n) if n.is_finite() => Value::Num(n),
                _ => return Err(format!("Expected {} to be a number", part.placeholder())),
            },
            ArgType::Boolean => match token {
                "true" => Value::Flag(true),
                "false" => Value::Flag(false),
                _ => return Err(format!("Expected {} to be a boolean", part.placeholder())),
            },
        };
        result.push((part.name.clone(), value));
    }
    if let Some(missing) = parts.iter().skip(used).find(|p| p.required) {
        return Err(format!("Missing {}", missing.placeholder()));
    }
    Ok(result)
}

/// Whitespace-separated tokens with their byte offsets into `raw`.
fn tokens(raw: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut rest = raw;
    let mut offset = 0;
    std::iter::from_fn(move || {
        let trimmed = rest.trim_start();
        offset += rest.len() - trimmed.len();
        if trimmed.is_empty() {
            return None;
        }
        let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        let start = offset;
        offset += end;
        rest = &trimmed[end..];
        Some((start, &trimmed[..end]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(template: &str, raw: &str) -> Result<Vec<(String, Value)>, String> {
        parse_args(&validate_template(template)?, raw)
    }
    fn text(s: &str) -> Value {
        Value::Text(s.into())
    }
    fn pair(name: &str, v: Value) -> (String, Value) {
        (name.to_string(), v)
    }

    // --- the original's documented examples -----------------------------

    #[test]
    fn single_string() {
        assert_eq!(run("<name:string>", "Snare"), Ok(vec![pair("name", text("Snare"))]));
    }

    #[test]
    fn string_and_number() {
        assert_eq!(
            run("<name:string> <amount:number>", "Snare 10"),
            Ok(vec![pair("name", text("Snare")), pair("amount", Value::Num(10.0))])
        );
    }

    #[test]
    fn optional_may_be_omitted() {
        assert_eq!(
            run("<name:string> [amount:number]", "Snare"),
            Ok(vec![pair("name", text("Snare"))])
        );
    }

    #[test]
    fn bad_number_names_the_placeholder() {
        assert_eq!(
            run("<name:string> <amount:number>", "Snare hello"),
            Err("Expected <amount> to be a number".into())
        );
        assert_eq!(run("[n:number]", "x"), Err("Expected [n] to be a number".into()));
    }

    // --- argument errors --------------------------------------------------

    #[test]
    fn missing_required_reports_first_missing() {
        assert_eq!(run("<a:string> <b:string> <c:string>", "x"), Err("Missing <b>".into()));
        assert_eq!(run("<a:string>", ""), Err("Missing <a>".into()));
    }

    #[test]
    fn too_many_arguments() {
        assert_eq!(run("<a:string>", "x y"), Err("Expected fewer arguments".into()));
        assert_eq!(run("", "x"), Err("Expected fewer arguments".into()));
    }

    #[test]
    fn booleans_are_exactly_true_or_false() {
        assert_eq!(run("<on:boolean>", "true"), Ok(vec![pair("on", Value::Flag(true))]));
        assert_eq!(run("<on:boolean>", "false"), Ok(vec![pair("on", Value::Flag(false))]));
        assert_eq!(run("<on:boolean>", "yes"), Err("Expected <on> to be a boolean".into()));
        assert_eq!(run("<on:boolean>", "True"), Err("Expected <on> to be a boolean".into()));
    }

    #[test]
    fn empty_input_is_no_arguments_not_one_empty_argument() {
        assert_eq!(run("", ""), Ok(vec![]));
        assert_eq!(run("", "   "), Ok(vec![]));
        // The JS version turned "" into [""] and Number("") into 0.
        assert_eq!(run("<n:number>", ""), Err("Missing <n>".into()));
        assert_eq!(run("[n:number]", ""), Ok(vec![]));
    }

    #[test]
    fn numbers_are_plain_finite_decimals() {
        assert_eq!(run("<n:number>", "-2.5"), Ok(vec![pair("n", Value::Num(-2.5))]));
        assert_eq!(run("<n:number>", "1e3"), Ok(vec![pair("n", Value::Num(1000.0))]));
        for bad in ["0x10", "inf", "NaN", "Infinity", "1,5", "--1"] {
            assert_eq!(run("<n:number>", bad), Err("Expected <n> to be a number".into()), "{bad}");
        }
    }

    #[test]
    fn extra_whitespace_between_arguments_is_fine() {
        assert_eq!(
            run("<a:string> <b:number>", "  x \t  3  "),
            Ok(vec![pair("a", text("x")), pair("b", Value::Num(3.0))])
        );
    }

    // --- spread -----------------------------------------------------------

    #[test]
    fn spread_takes_the_rest_of_the_line_with_original_spacing() {
        assert_eq!(
            run("<user:string> <msg:string...>", "bob  hello   there world  "),
            Ok(vec![pair("user", text("bob")), pair("msg", text("hello   there world"))])
        );
    }

    #[test]
    fn spread_may_be_one_word_or_omitted_when_optional() {
        assert_eq!(run("<msg:string...>", "hi"), Ok(vec![pair("msg", text("hi"))]));
        assert_eq!(run("<a:string> [rest:string...]", "x"), Ok(vec![pair("a", text("x"))]));
        assert_eq!(run("<msg:string...>", ""), Err("Missing <msg>".into()));
    }

    // --- template validation ---------------------------------------------

    #[test]
    fn template_literals_round_trip() {
        let parts = validate_template("<a:string> [b:number] [c:string...]").unwrap();
        let literals: Vec<_> = parts.iter().map(Part::literal).collect();
        assert_eq!(literals, ["<a:string>", "[b:number]", "[c:string...]"]);
    }

    #[test]
    fn empty_template_is_valid() {
        assert_eq!(validate_template(""), Ok(vec![]));
        assert_eq!(validate_template("   "), Ok(vec![]));
    }

    #[test]
    fn rejects_malformed_placeholders() {
        for bad in [
            "name", "<name>", "<:string>", "<na-me:string>", "<name:float>", "<name:string",
            "name:string>", "<name:string>x", "x<name:string>", "<name:>", "<>", "<name:string..>",
        ] {
            assert_eq!(validate_template(bad), Err("Invalid template".into()), "{bad}");
        }
    }

    #[test]
    fn rejects_mismatched_brackets() {
        for bad in ["<a:string]", "[a:string>"] {
            assert_eq!(
                validate_template(bad),
                Err("Invalid template: brackets must match".into()),
                "{bad}"
            );
        }
    }

    #[test]
    fn spread_rules() {
        assert_eq!(
            validate_template("<n:number...>"),
            Err("Invalid template: spread only valid for strings".into())
        );
        assert_eq!(
            validate_template("<a:string...> <b:string>"),
            Err("Invalid template: spread only valid in last position".into())
        );
    }

    #[test]
    fn rejects_required_after_optional_and_duplicates() {
        assert_eq!(
            validate_template("[a:string] <b:string>"),
            Err("Invalid template: required <b> cannot follow an optional argument".into())
        );
        assert_eq!(
            validate_template("<a:string> <a:number>"),
            Err("Invalid template: duplicate name a".into())
        );
    }
}

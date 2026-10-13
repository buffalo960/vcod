//! The `.menu` grammar's tokenizer and the item cvar test, shared by the
//! menu parser in [`crate::ui_menu`].

#[derive(Debug, Clone)]
pub struct CvarGate {
    pub cvar: String,
    /// `true` for `showCvar` (visible when the value is listed), `false` for
    /// `hideCvar` (visible when it is not).
    pub show: bool,
    pub values: Vec<String>,
}

impl CvarGate {
    /// A cvar the client has never heard of reads as the empty string, same
    /// as retail's `Cvar_VariableString` on an unknown name.
    pub fn passes(&self, value: Option<&str>) -> bool {
        let v = value.unwrap_or("");
        self.values.iter().any(|x| x == v) == self.show
    }
}

/// `//` and `/* */` comments dropped, `#`-lines skipped, quoted strings kept
/// as one token, `{`, `}` and `;` as their own tokens.
pub(crate) fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Byte tests only, all ASCII, so every slice boundary is a char boundary.
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'#' || (c == b'/' && bytes.get(i + 1) == Some(&b'/')) {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else if c == b'"' {
            let start = i + 1;
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            tokens.push(text[start..i].to_string());
            i += 1;
        } else if c == b'{' || c == b'}' || c == b';' {
            tokens.push((c as char).to_string());
            i += 1;
        } else {
            let start = i;
            while i < bytes.len() {
                let c = bytes[i];
                if c.is_ascii_whitespace() || matches!(c, b'{' | b'}' | b';' | b'"') {
                    break;
                }
                i += 1;
            }
            tokens.push(text[start..i].to_string());
        }
    }
    tokens
}

/// The tokens strictly between the `{` at `tokens[*i]` and its matching `}`,
/// advancing `*i` past the closing brace. Empty and `*i` unchanged if
/// `tokens[*i]` is not `{`.
pub(crate) fn block<'a>(tokens: &'a [String], i: &mut usize) -> &'a [String] {
    if tokens.get(*i).map(String::as_str) != Some("{") {
        return &[];
    }
    let start = *i + 1;
    let mut depth = 1;
    let mut j = start;
    while j < tokens.len() && depth > 0 {
        match tokens[j].as_str() {
            "{" => depth += 1,
            "}" => depth -= 1,
            _ => {}
        }
        j += 1;
    }
    // Unbalanced input (a brace with no match) takes everything left rather
    // than panicking: a menu cut off mid-block still parses what it has.
    let end = if depth == 0 { j - 1 } else { j };
    *i = j;
    &tokens[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cvar_gate_hides_disallowed_weapon() {
        let gate = CvarGate {
            cvar: "scr_allow_m1carbine".into(),
            show: false,
            values: vec!["0".into()],
        };
        assert!(!gate.passes(Some("0")));
        assert!(gate.passes(Some("1")));
        assert!(gate.passes(None));
    }

    #[test]
    fn a_block_cut_off_takes_what_is_left() {
        let t = tokenize("{ menuDef { name \"x\"");
        let mut i = 0;
        assert_eq!(block(&t, &mut i), ["menuDef", "{", "name", "x"]);
        assert_eq!(i, t.len());
    }

    #[test]
    fn non_ascii_text_tokenizes_without_panicking() {
        // 0x85 and 0xA0 are UTF-8 continuation bytes that read as whitespace as a char.
        assert_eq!(tokenize("name à"), ["name", "à"]);
        tokenize("{ menuDef { itemDef { text Å } } }");
        assert_eq!(tokenize("name\u{a0}\"x\""), ["name\u{a0}", "x"]);
    }
}

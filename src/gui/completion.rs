//! Context-aware completion for the Lua editor — a small, self-contained
//! "IntelliSense" over the `nem` API (egui has no language server). The pure
//! [`complete`] function decides what to suggest; the editor renders it.

use std::ops::Range;

/// One completion suggestion.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// Text shown and inserted.
    pub label: String,
    /// Short signature/type shown next to the label.
    pub detail: &'static str,
}

/// The result of a completion query: which characters to replace and the
/// matching candidates.
#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    /// Char range of the token being completed (to be replaced on accept).
    pub range: Range<usize>,
    /// Matching candidates, sorted by label.
    pub candidates: Vec<Candidate>,
}

// Members of `nem.` ------------------------------------------------------------
const NEM: &[(&str, &str)] = &[
    ("processes", "() -> {id,name,parent_id}[]"),
    ("attach", "{pid=|name=|plugin=} -> Target"),
    ("open", "(pid) -> Target"),
    ("pattern", "(sig, style?) -> Pattern"),
    ("pattern_code", "(bytes, mask) -> Pattern"),
    ("class", "(name) -> ClassBuilder"),
    ("project", "() -> Project"),
    ("load_project", "(ron) -> Project"),
    ("generate", "(project, lang) -> string"),
    ("kinds", "field-kind table"),
    ("classes", "() -> string[] (GUI)"),
    ("set_class_address", "(name, addr) (GUI)"),
    ("class_address", "(name) -> integer? (GUI)"),
];

// Members of `nem.kinds.` ------------------------------------------------------
const KINDS: &[(&str, &str)] = &[
    ("i8", "Kind"),
    ("i16", "Kind"),
    ("i32", "Kind"),
    ("i64", "Kind"),
    ("u8", "Kind"),
    ("u16", "Kind"),
    ("u32", "Kind"),
    ("u64", "Kind"),
    ("f32", "Kind"),
    ("f64", "Kind"),
    ("bool", "Kind"),
    ("ptr", "Kind"),
    ("strptr", "Kind"),
    ("hex8", "Kind"),
    ("hex16", "Kind"),
    ("hex32", "Kind"),
    ("hex64", "Kind"),
    ("vec", "(components, \"f32\"|\"f64\") -> Kind"),
    ("mat", "(rows, cols, \"f32\"|\"f64\") -> Kind"),
];

// Method names after `:` (union across userdata types) -------------------------
const METHODS: &[(&str, &str)] = &[
    ("read_i8", "(addr) -> integer"),
    ("read_i16", "(addr) -> integer"),
    ("read_i32", "(addr) -> integer"),
    ("read_i64", "(addr) -> integer"),
    ("read_u8", "(addr) -> integer"),
    ("read_u16", "(addr) -> integer"),
    ("read_u32", "(addr) -> integer"),
    ("read_u64", "(addr) -> integer"),
    ("read_f32", "(addr) -> number"),
    ("read_f64", "(addr) -> number"),
    ("read_ptr", "(addr) -> integer"),
    ("read_bytes", "(addr, len) -> string"),
    ("read_string", "(addr) -> string"),
    ("write_i8", "(addr, v)"),
    ("write_i16", "(addr, v)"),
    ("write_i32", "(addr, v)"),
    ("write_i64", "(addr, v)"),
    ("write_u8", "(addr, v)"),
    ("write_u16", "(addr, v)"),
    ("write_u32", "(addr, v)"),
    ("write_u64", "(addr, v)"),
    ("write_f32", "(addr, v)"),
    ("write_f64", "(addr, v)"),
    ("write_bytes", "(addr, data)"),
    ("can_read", "(addr) -> boolean"),
    ("id", "() -> integer"),
    ("name", "() -> string"),
    ("pointer_size", "() -> integer"),
    ("is_wine", "() -> boolean"),
    ("modules", "() -> {base,size,name}[]"),
    ("module", "(name) -> {base,size,name}"),
    ("scan", "(pat, opts) -> integer[]"),
    ("scan_module", "(pat, module) -> integer[]"),
    ("scan_range", "(pat, start, len) -> integer[]"),
    ("resolve", "(base, offsets) -> integer"),
    ("rip", "(rel32, next) -> integer"),
    ("eval", "(expr) -> integer"),
    ("infer", "(addr) -> string?"),
    ("len", "() -> integer"),
    ("is_empty", "() -> boolean"),
    ("size", "() -> integer"),
    ("field", "(name, kind)"),
    ("field_at", "(name, kind, offset)"),
    ("pad", "(bytes)"),
    ("build", "() -> Type"),
    ("to_ron", "() -> string"),
    ("generate", "(lang) -> string"),
    ("add", "(type)"),
];

// Top-level identifiers --------------------------------------------------------
const GLOBALS: &[(&str, &str)] = &[
    ("nem", "the nem API"),
    ("PID", "integer? (attached pid)"),
    ("PNAME", "string?"),
    ("PROJECT", "string? (project dir)"),
    ("EXPORT", "string (set to import)"),
    ("print", "(...)"),
    ("pcall", "(f, ...)"),
    ("ipairs", "(t)"),
    ("pairs", "(t)"),
    ("tostring", "(v)"),
    ("tonumber", "(v)"),
    ("string", "stdlib"),
    ("math", "stdlib"),
    ("table", "stdlib"),
];

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn to_candidates(table: &[(&str, &'static str)]) -> Vec<Candidate> {
    table
        .iter()
        .map(|(label, detail)| Candidate {
            label: (*label).to_owned(),
            detail,
        })
        .collect()
}

/// Computes completions for `text` with the caret at char index `caret`.
///
/// Returns `None` when there is nothing to suggest (unknown member base, no
/// matches, or a bare identifier with an empty prefix).
pub fn complete(text: &str, caret: usize) -> Option<Completion> {
    let chars: Vec<char> = text.chars().collect();
    let caret = caret.min(chars.len());

    // The identifier token currently under the caret.
    let mut start = caret;
    while start > 0 && is_ident(chars[start - 1]) {
        start -= 1;
    }
    let token: String = chars[start..caret].iter().collect();

    // The character just before the token selects the context.
    let pool = if start > 0 && chars[start - 1] == '.' {
        // Member access: read the identifier immediately before the '.'.
        let dot = start - 1;
        let mut b = dot;
        while b > 0 && is_ident(chars[b - 1]) {
            b -= 1;
        }
        let base: String = chars[b..dot].iter().collect();
        match base.as_str() {
            "nem" => to_candidates(NEM),
            "kinds" => to_candidates(KINDS),
            _ => return None,
        }
    } else if start > 0 && chars[start - 1] == ':' {
        to_candidates(METHODS)
    } else {
        // Bare identifier: only suggest once something has been typed.
        if token.is_empty() {
            return None;
        }
        to_candidates(GLOBALS)
    };

    let prefix = token.to_ascii_lowercase();
    let mut candidates: Vec<Candidate> = pool
        .into_iter()
        .filter(|c| c.label.to_ascii_lowercase().starts_with(&prefix))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|a, b| a.label.cmp(&b.label));
    candidates.dedup_by(|a, b| a.label == b.label);

    Some(Completion {
        range: start..caret,
        candidates,
    })
}

#[cfg(test)]
mod tests {
    use super::complete;

    fn labels(text: &str, caret: usize) -> Vec<String> {
        complete(text, caret)
            .map(|c| c.candidates.into_iter().map(|c| c.label).collect())
            .unwrap_or_default()
    }

    #[test]
    fn nem_members() {
        let l = labels("nem.", 4);
        assert!(l.contains(&"attach".to_string()));
        assert!(l.contains(&"kinds".to_string()));
        assert!(l.contains(&"set_class_address".to_string()));
        // Prefix narrows the set.
        assert_eq!(labels("nem.pa", 6), vec!["pattern", "pattern_code"]);
    }

    #[test]
    fn kinds_members() {
        let l = labels("nem.kinds.", 10);
        assert!(l.contains(&"i32".to_string()));
        assert!(l.contains(&"vec".to_string()));
        assert_eq!(labels("nem.kinds.ve", 12), vec!["vec"]);
    }

    #[test]
    fn methods_after_colon() {
        let c = complete("local a = proc:read_i", 21).unwrap();
        let l: Vec<_> = c.candidates.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(l, vec!["read_i16", "read_i32", "read_i64", "read_i8"]);
        // Replaces just the "read_i" token.
        assert_eq!(&"local a = proc:read_i"[c.range.clone()], "read_i");
    }

    #[test]
    fn top_level_identifiers() {
        assert_eq!(labels("PI", 2), vec!["PID"]);
        assert!(labels("pr", 2).contains(&"print".to_string()));
    }

    #[test]
    fn no_suggestions_cases() {
        assert!(complete("local x = ", 10).is_none()); // empty bare token
        assert!(complete("nem.zzz", 7).is_none()); // no matching member
        assert!(complete("foo.bar", 7).is_none()); // unknown object member
    }
}

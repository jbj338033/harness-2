use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSkill {
    pub name: String,
    pub description: String,
    pub license: Option<String>,
    pub compatibility: Option<String>,
    pub allowed_tools: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub body: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("missing YAML frontmatter delimiters")]
    MissingFrontmatter,
    #[error("invalid YAML frontmatter: {0}")]
    InvalidYaml(String),
    #[error("description is required")]
    MissingDescription,
}

pub fn parse_skill_md(raw: &str) -> Result<ParsedSkill, ParseError> {
    let (frontmatter, body) = split_frontmatter(raw)?;
    let raw_map = parse_yaml_lenient(frontmatter)?;

    let mut warnings = Vec::new();

    let Some(description) = raw_map
        .get("description")
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
    else {
        return Err(ParseError::MissingDescription);
    };

    let name = raw_map
        .get("name")
        .map(String::as_str)
        .map_or("", str::trim)
        .to_owned();
    if name.is_empty() {
        warnings.push("missing `name` — directory name will be used".into());
    }
    if name.len() > 64 {
        warnings.push(format!(
            "name exceeds spec max of 64 characters ({})",
            name.len()
        ));
    }
    if !name.is_empty() && !name_looks_valid(&name) {
        warnings.push(format!(
            "name `{name}` contains characters outside a-z 0-9 -"
        ));
    }

    let license = raw_map.get("license").cloned();
    let compatibility = raw_map.get("compatibility").cloned();
    let allowed_tools = raw_map.get("allowed-tools").cloned();

    let mut metadata = BTreeMap::new();
    for (k, v) in raw_map {
        if matches!(
            k.as_str(),
            "name" | "description" | "license" | "compatibility" | "allowed-tools"
        ) {
            continue;
        }
        metadata.insert(k, v);
    }

    Ok(ParsedSkill {
        name,
        description,
        license,
        compatibility,
        allowed_tools,
        metadata,
        body,
        warnings,
    })
}

fn split_frontmatter(raw: &str) -> Result<(&str, String), ParseError> {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let after_open = raw
        .strip_prefix("---\n")
        .or_else(|| raw.strip_prefix("---\r\n"))
        .ok_or(ParseError::MissingFrontmatter)?;

    let mut cursor = 0usize;
    for line in after_open.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" {
            let frontmatter = &after_open[..cursor];
            let body_start = cursor + line.len();
            let body = after_open[body_start..].trim_start_matches(['\r', '\n']);
            return Ok((frontmatter, body.to_string()));
        }
        cursor += line.len();
    }
    Err(ParseError::MissingFrontmatter)
}

fn parse_yaml_lenient(frontmatter: &str) -> Result<BTreeMap<String, String>, ParseError> {
    match yaml_to_string_map(frontmatter) {
        Ok(m) => Ok(m),
        Err(first) => {
            let wrapped = quote_wrap_values(frontmatter);
            yaml_to_string_map(&wrapped).map_err(|_| ParseError::InvalidYaml(first))
        }
    }
}

fn yaml_to_string_map(src: &str) -> Result<BTreeMap<String, String>, String> {
    let value: serde_yaml::Value = serde_yaml::from_str(src).map_err(|e| e.to_string())?;
    let mapping = match value {
        serde_yaml::Value::Mapping(m) => m,
        serde_yaml::Value::Null => serde_yaml::Mapping::new(),
        _ => return Err("frontmatter is not a YAML mapping".into()),
    };
    let mut out = BTreeMap::new();
    for (k, v) in mapping {
        let key = match k {
            serde_yaml::Value::String(s) => s,
            other => serde_yaml::to_string(&other).map_err(|e| e.to_string())?,
        };
        let value = yaml_scalar_to_string(&v);
        out.insert(key, value);
    }
    Ok(out)
}

fn yaml_scalar_to_string(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Null => String::new(),
        serde_yaml::Value::Mapping(_) | serde_yaml::Value::Sequence(_) => serde_yaml::to_string(v)
            .unwrap_or_default()
            .trim()
            .to_string(),
        serde_yaml::Value::Tagged(t) => yaml_scalar_to_string(&t.value),
    }
}

fn quote_wrap_values(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 16);
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if let Some(idx) = line.find(':') {
            let (key, rest) = line.split_at(idx);
            let value = rest[1..].trim();
            if value.is_empty()
                || value.starts_with('"')
                || value.starts_with('\'')
                || value.starts_with('|')
                || value.starts_with('>')
                || value.starts_with('[')
                || value.starts_with('{')
            {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
            out.push_str(key);
            out.push_str(": \"");
            out.push_str(&escaped);
            out.push_str("\"\n");
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn name_looks_valid(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first == '-' {
        return false;
    }
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    let mut prev_hyphen = false;
    for c in chars {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return false;
        }
        if c == '-' && prev_hyphen {
            return false;
        }
        prev_hyphen = c == '-';
    }
    !prev_hyphen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_skill() {
        let src = "---\nname: pdf-processing\ndescription: Extract PDF text.\n---\nBody here.\n";
        let p = parse_skill_md(src).unwrap();
        assert_eq!(p.name, "pdf-processing");
        assert_eq!(p.description, "Extract PDF text.");
        assert!(p.body.starts_with("Body here."));
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn rejects_missing_description() {
        let src = "---\nname: foo\n---\n";
        assert!(matches!(
            parse_skill_md(src),
            Err(ParseError::MissingDescription)
        ));
    }

    #[test]
    fn rejects_missing_frontmatter() {
        let src = "no frontmatter here\njust body\n";
        assert!(matches!(
            parse_skill_md(src),
            Err(ParseError::MissingFrontmatter)
        ));
    }

    #[test]
    fn warns_on_missing_name() {
        let src = "---\ndescription: does things\n---\nbody\n";
        let p = parse_skill_md(src).unwrap();
        assert!(p.name.is_empty());
        assert!(p.warnings.iter().any(|w| w.contains("missing `name`")));
    }

    #[test]
    fn warns_on_uppercase_name() {
        let src = "---\nname: PDF-Thing\ndescription: d\n---\nbody\n";
        let p = parse_skill_md(src).unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("a-z 0-9")));
    }

    #[test]
    fn quote_wrap_fallback_handles_colon_in_value() {
        let src = "---\nname: x\ndescription: Use when: the user mentions PDFs\n---\n";
        let p = parse_skill_md(src).unwrap();
        assert_eq!(p.description, "Use when: the user mentions PDFs");
    }

    #[test]
    fn captures_optional_fields() {
        let src = "---\nname: x\ndescription: d\nlicense: MIT\ncompatibility: needs git\nallowed-tools: Bash Read\nauthor: acme\n---\nbody\n";
        let p = parse_skill_md(src).unwrap();
        assert_eq!(p.license.as_deref(), Some("MIT"));
        assert_eq!(p.compatibility.as_deref(), Some("needs git"));
        assert_eq!(p.allowed_tools.as_deref(), Some("Bash Read"));
        assert_eq!(p.metadata.get("author").map(String::as_str), Some("acme"));
    }

    #[test]
    fn strips_bom() {
        let src = "\u{feff}---\nname: x\ndescription: d\n---\nb";
        let p = parse_skill_md(src).unwrap();
        assert_eq!(p.name, "x");
    }

    #[test]
    fn name_validation() {
        assert!(name_looks_valid("pdf-processing"));
        assert!(name_looks_valid("echo2"));
        assert!(!name_looks_valid("PDF-Thing"));
        assert!(!name_looks_valid("-leading"));
        assert!(!name_looks_valid("trailing-"));
        assert!(!name_looks_valid("double--hyphen"));
    }
}

#[must_use]
pub fn format_tool_description(raw: &str) -> String {
    if raw.contains("USE:") || raw.contains("DO NOT USE:") {
        return raw.trim().to_string();
    }
    let trimmed = raw.trim();
    format!(
        "{trimmed}\nUSE: whenever this tool's name and schema match the task.\nDO NOT USE: for tasks another tool is specialized for."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_hints_present() {
        let raw = "Search files.\nUSE: grep-like.\nDO NOT USE: for globbing.";
        assert_eq!(format_tool_description(raw), raw);
    }

    #[test]
    fn appends_generic_hints_when_missing() {
        let out = format_tool_description("Do the thing.");
        assert!(out.contains("Do the thing."));
        assert!(out.contains("USE:"));
        assert!(out.contains("DO NOT USE:"));
    }
}

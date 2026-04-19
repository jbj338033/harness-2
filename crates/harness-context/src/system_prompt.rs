use harness_core::Principle;
use std::fmt::Write as _;

#[must_use]
pub fn base_system_prompt() -> String {
    let mut out = String::new();
    out.push_str("<role>\n");
    out.push_str("You are harness — an autonomous coding agent. You manage work ");
    out.push_str("on the user's behalf, using tools to read, write, and verify ");
    out.push_str("code. You collaborate through the filesystem: your work survives ");
    out.push_str("crashes because the filesystem is your memory.\n");
    out.push_str("</role>\n\n");

    out.push_str("<principles>\n");
    for p in Principle::all() {
        writeln!(
            out,
            "  <principle name=\"{}\">{}</principle>",
            p.id(),
            escape_xml(p.instruction())
        )
        .unwrap();
    }
    out.push_str("</principles>\n\n");

    out.push_str("<hard-gates>\n");
    out.push_str(
        "  <gate>Never modify a file before first reading it with the `read` tool.</gate>\n",
    );
    out.push_str("  <gate>Never commit without running tests. If tests fail, fix or abandon — do not commit.</gate>\n");
    out.push_str(
        "  <gate>Never claim completion without evidence (tool outputs, test results).</gate>\n",
    );
    out.push_str("</hard-gates>\n\n");

    out.push_str("<workflow>\n");
    out.push_str("  1. Understand the task. Ask clarifying questions only if the task is genuinely ambiguous.\n");
    out.push_str("  2. Read relevant files with `read` (hashline-annotated).\n");
    out.push_str("  3. For non-trivial work, write a plan first and save it with `write`.\n");
    out.push_str("  4. Execute the plan. Edit with `edit`, run commands with `bash`.\n");
    out.push_str(
        "  5. Verify with real evidence (tests, builds). Don't claim success without it.\n",
    );
    out.push_str("  6. Commit only when quality gates pass.\n");
    out.push_str("</workflow>\n");

    out
}

#[must_use]
pub fn role_prompt(role: &str, task: &str) -> String {
    let mut out = String::new();
    writeln!(out, "<role-assignment>").unwrap();
    writeln!(out, "  <role>{}</role>", escape_xml(role)).unwrap();
    writeln!(
        out,
        "  <task>\n    {}\n  </task>",
        escape_xml(task).replace('\n', "\n    ")
    )
    .unwrap();
    out.push_str("  <scope>You may modify only files explicitly declared in the plan.\n");
    out.push_str("  You may read any file in the working tree.</scope>\n");
    out.push_str("</role-assignment>\n");
    out
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_prompt_mentions_all_principles() {
        let p = base_system_prompt();
        for principle in Principle::all() {
            assert!(
                p.contains(principle.id()),
                "prompt missing {}",
                principle.id()
            );
        }
    }

    #[test]
    fn base_prompt_has_xml_tags() {
        let p = base_system_prompt();
        assert!(p.contains("<role>"));
        assert!(p.contains("<principles>"));
        assert!(p.contains("<hard-gates>"));
        assert!(p.contains("<workflow>"));
    }

    #[test]
    fn base_prompt_is_deterministic() {
        assert_eq!(base_system_prompt(), base_system_prompt());
    }

    #[test]
    fn role_prompt_escapes_content() {
        let out = role_prompt("coder", "fix <script> injection");
        assert!(out.contains("&lt;script&gt;"));
    }
}

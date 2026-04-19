use crate::{base_system_prompt, tool_description};
use harness_llm_types::{ChatRequest, Message, ToolDef};
use harness_memory::MemoryRecord;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheBreakpoint {
    AfterBase,
    AfterTools,
    AfterMemory,
    AfterRole,
}

impl CacheBreakpoint {
    #[must_use]
    pub const fn all() -> [CacheBreakpoint; 4] {
        [
            Self::AfterBase,
            Self::AfterTools,
            Self::AfterMemory,
            Self::AfterRole,
        ]
    }
}

pub struct AssemblyInputs<'a> {
    pub role: &'a str,
    pub task: &'a str,
    pub tools: &'a [ToolDef],
    pub memories: &'a [MemoryRecord],
    pub skills: &'a [SkillSummary<'a>],
    pub messages: &'a [Message],
}

#[derive(Debug, Clone, Copy)]
pub struct SkillSummary<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub location: &'a str,
}

#[derive(Debug, Clone)]
pub struct AssembledPrompt {
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub breakpoint_offsets: [usize; 4],
}

impl AssembledPrompt {
    #[must_use]
    pub fn into_chat_request(self) -> ChatRequest {
        ChatRequest {
            system: Some(self.system),
            messages: self.messages,
            tools: self.tools,
        }
    }
}

#[must_use]
pub fn assemble(inputs: &AssemblyInputs<'_>) -> AssembledPrompt {
    use std::fmt::Write;
    let mut sys = base_system_prompt();
    let bp_base = sys.len();

    sys.push_str("\n<tools>\n");
    for t in inputs.tools {
        writeln!(
            sys,
            "  <tool name=\"{}\">\n    {}\n  </tool>",
            escape(&t.name),
            escape(&tool_description::format_tool_description(&t.description))
                .replace('\n', "\n    ")
        )
        .unwrap();
    }
    sys.push_str("</tools>\n");
    let bp_tools = sys.len();

    if !inputs.memories.is_empty() {
        sys.push('\n');
        sys.push_str(&harness_memory::selection::render_xml(inputs.memories));
    }
    if !inputs.skills.is_empty() {
        sys.push_str("\n<available_skills>\n");
        for s in inputs.skills {
            writeln!(
                sys,
                "  <skill>\n    <name>{}</name>\n    <description>{}</description>\n    <location>{}</location>\n  </skill>",
                escape(s.name),
                escape(s.description),
                escape(s.location),
            )
            .unwrap();
        }
        sys.push_str("</available_skills>\n");
        sys.push_str(
            "The `<available_skills>` list above enumerates skills installed on this machine. \
             When a task matches a skill's description, call the `activate_skill` tool with the skill's name to load \
             its full instructions into the conversation. Do not invent skills that are not listed.\n",
        );
    }
    let bp_memory = sys.len();

    sys.push('\n');
    sys.push_str(&crate::system_prompt::role_prompt(inputs.role, inputs.task));
    let bp_role = sys.len();

    AssembledPrompt {
        system: sys,
        messages: inputs.messages.to_vec(),
        tools: inputs.tools.to_vec(),
        breakpoint_offsets: [bp_base, bp_tools, bp_memory, bp_role],
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_llm_types::{Message, ToolDef};
    use serde_json::json;

    fn tool(name: &str) -> ToolDef {
        ToolDef {
            name: name.into(),
            description: format!("desc of {name}"),
            input_schema: json!({}),
        }
    }

    #[test]
    fn offsets_are_monotonic() {
        let tools = vec![tool("read"), tool("write")];
        let memories = vec![MemoryRecord {
            id: "x".into(),
            scope: harness_memory::Scope::Global,
            content: "use pnpm".into(),
            created_at: 0,
        }];
        let messages = vec![Message::user_text("hi")];

        let out = assemble(&AssemblyInputs {
            role: "root",
            task: "do the thing",
            tools: &tools,
            memories: &memories,
            skills: &[],
            messages: &messages,
        });

        let offsets = out.breakpoint_offsets;
        assert!(offsets[0] < offsets[1]);
        assert!(offsets[1] < offsets[2]);
        assert!(offsets[2] < offsets[3]);
        assert_eq!(offsets[3], out.system.len());
    }

    #[test]
    fn system_prompt_contains_all_sections() {
        let tools = vec![tool("bash")];
        let memories = vec![];
        let messages = vec![];

        let out = assemble(&AssemblyInputs {
            role: "coder",
            task: "implement auth",
            tools: &tools,
            memories: &memories,
            skills: &[],
            messages: &messages,
        });
        assert!(out.system.contains("<role>"));
        assert!(out.system.contains("<tools>"));
        assert!(out.system.contains("<tool name=\"bash\">"));
        assert!(out.system.contains("<role-assignment>"));
        assert!(out.system.contains("implement auth"));
    }

    #[test]
    fn empty_memories_collapse_cleanly() {
        let tools = vec![];
        let memories = vec![];
        let messages = vec![];
        let out = assemble(&AssemblyInputs {
            role: "root",
            task: "",
            tools: &tools,
            memories: &memories,
            skills: &[],
            messages: &messages,
        });
        assert!(!out.system.contains("<memory>"));
        assert_eq!(out.breakpoint_offsets[1], out.breakpoint_offsets[2]);
    }

    #[test]
    fn assembly_is_deterministic_across_calls() {
        let tools = vec![tool("read")];
        let memories = vec![];
        let messages = vec![];
        let a = assemble(&AssemblyInputs {
            role: "r",
            task: "t",
            tools: &tools,
            memories: &memories,
            skills: &[],
            messages: &messages,
        });
        let b = assemble(&AssemblyInputs {
            role: "r",
            task: "t",
            tools: &tools,
            memories: &memories,
            skills: &[],
            messages: &messages,
        });
        assert_eq!(a.system, b.system);
        assert_eq!(a.breakpoint_offsets, b.breakpoint_offsets);
    }

    #[test]
    fn skill_catalog_renders_in_system_prompt() {
        let tools = vec![];
        let memories = vec![];
        let messages = vec![];
        let skills = vec![
            SkillSummary {
                name: "pdf-processing",
                description: "extract PDF text",
                location: "/home/u/.agents/skills/pdf-processing/SKILL.md",
            },
            SkillSummary {
                name: "echo-test",
                description: "prints hello",
                location: "/home/u/.agents/skills/echo-test/SKILL.md",
            },
        ];
        let out = assemble(&AssemblyInputs {
            role: "root",
            task: "t",
            tools: &tools,
            memories: &memories,
            skills: &skills,
            messages: &messages,
        });
        assert!(out.system.contains("<available_skills>"));
        assert!(out.system.contains("<name>pdf-processing</name>"));
        assert!(
            out.system
                .contains("<description>extract PDF text</description>")
        );
        assert!(out.system.contains("activate_skill"));
    }

    #[test]
    fn empty_skill_catalog_emits_nothing() {
        let tools = vec![];
        let memories = vec![];
        let messages = vec![];
        let out = assemble(&AssemblyInputs {
            role: "root",
            task: "t",
            tools: &tools,
            memories: &memories,
            skills: &[],
            messages: &messages,
        });
        assert!(!out.system.contains("<available_skills>"));
    }

    #[test]
    fn into_chat_request_preserves_fields() {
        let tools = vec![tool("bash")];
        let memories = vec![];
        let messages = vec![Message::user_text("go")];
        let out = assemble(&AssemblyInputs {
            role: "root",
            task: "x",
            tools: &tools,
            memories: &memories,
            skills: &[],
            messages: &messages,
        });
        let req = out.into_chat_request();
        assert!(req.system.as_deref().unwrap().contains("<principles>"));
        assert_eq!(req.tools.len(), 1);
        assert_eq!(req.messages.len(), 1);
    }
}

use async_trait::async_trait;
use harness_session::message::{self, MessageKind, MessageRole, NewMessage};
use harness_skills::{Catalog, activate};
use harness_storage::WriterHandle;
use harness_tools::{Tool, ToolContext, ToolError, ToolOutput};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub const TOOL_NAME: &str = "activate_skill";

pub struct ActivateSkill {
    catalog: Arc<RwLock<Catalog>>,
    writer: WriterHandle,
    db_path: PathBuf,
}

impl ActivateSkill {
    #[must_use]
    pub fn new(catalog: Arc<RwLock<Catalog>>, writer: WriterHandle, db_path: PathBuf) -> Self {
        Self {
            catalog,
            writer,
            db_path,
        }
    }
}

#[async_trait]
impl Tool for ActivateSkill {
    fn name(&self) -> &str {
        TOOL_NAME
    }

    fn description(&self) -> &'static str {
        "Load a skill's SKILL.md body into the conversation so you can follow its instructions.\n\
         USE: when a task matches a skill's <description> in the <available_skills> catalog.\n\
         DO NOT USE: to discover what skills exist (the catalog in the system prompt already lists them), \
         or to activate a skill that is not in the catalog. Activating the same skill twice in one agent is a no-op."
    }

    fn input_schema(&self) -> Value {
        let names: Vec<String> = self.catalog.read().map(|c| c.names()).unwrap_or_default();
        let name_schema = if names.is_empty() {
            json!({ "type": "string" })
        } else {
            json!({ "type": "string", "enum": names })
        };
        json!({
            "type": "object",
            "properties": {
                "name": name_schema,
            },
            "required": ["name"],
            "additionalProperties": false,
        })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        #[derive(serde::Deserialize)]
        struct Args {
            name: String,
        }
        let Args { name } = serde_json::from_value(input)
            .map_err(|e| ToolError::Input(format!("invalid input: {e}")))?;

        let skill = {
            let catalog = self
                .catalog
                .read()
                .map_err(|_| ToolError::Other("skills lock poisoned".into()))?;
            let Some(s) = catalog.get(&name) else {
                return Ok(ToolOutput::err(format!("skill not found: {name}")));
            };
            s.clone()
        };

        if already_activated(&self.db_path, ctx.agent, &name)
            .await
            .unwrap_or(false)
        {
            return Ok(ToolOutput::ok(format!(
                "skill `{name}` is already active in this agent"
            )));
        }

        let activation =
            activate(&skill).map_err(|e| ToolError::Other(format!("activate skill: {e}")))?;

        let wrapped = render_skill_attachment(&activation);

        message::insert(
            &self.writer,
            NewMessage {
                agent_id: ctx.agent,
                role: MessageRole::System,
                content: Some(wrapped),
                model: None,
                kind: MessageKind::SkillAttachment,
            },
        )
        .await
        .map_err(|e| ToolError::Other(format!("persist skill message: {e}")))?;

        Ok(ToolOutput::ok(format!(
            "skill activated: {} ({} resources)",
            activation.name,
            activation.resources.len()
        )))
    }
}

fn render_skill_attachment(a: &harness_skills::Activation) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    writeln!(s, "<skill_content name=\"{}\">", xml_escape(&a.name)).unwrap();
    s.push_str(a.body.trim_end());
    s.push_str("\n\nSkill directory: ");
    s.push_str(&a.directory.display().to_string());
    s.push('\n');
    if !a.resources.is_empty() {
        s.push_str("<skill_resources>\n");
        for r in &a.resources {
            writeln!(s, "  <file>{}</file>", xml_escape(r)).unwrap();
        }
        s.push_str("</skill_resources>\n");
    }
    s.push_str("</skill_content>");
    s
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

async fn already_activated(
    db_path: &std::path::Path,
    agent: harness_core::AgentId,
    skill: &str,
) -> rusqlite::Result<bool> {
    let db_path = db_path.to_path_buf();
    let agent_s = agent.as_uuid().to_string();
    let needle = format!("%<skill_content name=\"{}\">%", xml_escape(skill));
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&db_path)?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM messages
             WHERE agent_id = ?1
               AND kind = 'skill_attachment'
               AND content LIKE ?2",
            rusqlite::params![agent_s, needle],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    })
    .await
    .unwrap_or(Ok(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_session::agent::{self, NewAgent};
    use harness_session::manager::SessionManager;
    use harness_skills::{Skill, SkillLayout, SkillScope};
    use harness_storage::{Database, Writer};
    use std::collections::BTreeMap;
    use std::fs;
    use tempfile::NamedTempFile;

    async fn test_setup() -> (NamedTempFile, WriterHandle, harness_core::AgentId, PathBuf) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let writer = Writer::spawn(f.path()).unwrap();
        let mgr = SessionManager::new(&writer);
        let s = mgr.create("/tmp", None).await.unwrap();
        let aid = agent::insert(
            &writer,
            NewAgent {
                session_id: s.id,
                parent_id: None,
                role: "root".into(),
                model: "m".into(),
                system_prompt: None,
                worktree_path: None,
                wave: None,
            },
        )
        .await
        .unwrap();
        let path = f.path().to_path_buf();
        (f, writer, aid, path)
    }

    fn sample_skill(dir: &std::path::Path, name: &str) -> Skill {
        fs::create_dir_all(dir).unwrap();
        let skill_md = dir.join("SKILL.md");
        fs::write(
            &skill_md,
            format!("---\nname: {name}\ndescription: d\n---\nBody of {name}.\n"),
        )
        .unwrap();
        Skill {
            name: name.into(),
            description: "d".into(),
            location: skill_md,
            license: None,
            compatibility: None,
            allowed_tools: None,
            metadata: BTreeMap::new(),
            scope: SkillScope::User,
            layout: SkillLayout::Std,
        }
    }

    #[tokio::test]
    async fn input_schema_enumerates_discovered_names() {
        let (_f, writer, _aid, db_path) = test_setup().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut catalog = Catalog::new();
        catalog.insert(sample_skill(&tmp.path().join("alpha"), "alpha"));
        catalog.insert(sample_skill(&tmp.path().join("beta"), "beta"));
        let tool = ActivateSkill::new(Arc::new(RwLock::new(catalog)), writer, db_path);
        let schema = tool.input_schema();
        let enum_val = &schema["properties"]["name"]["enum"];
        assert!(enum_val.is_array());
        let names: Vec<String> = enum_val
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"alpha".to_string()));
        assert!(names.contains(&"beta".to_string()));
    }

    #[tokio::test]
    async fn activates_and_persists_as_skill_attachment() {
        let (_f, writer, aid, db_path) = test_setup().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut catalog = Catalog::new();
        catalog.insert(sample_skill(&tmp.path().join("echo"), "echo"));
        let tool = ActivateSkill::new(
            Arc::new(RwLock::new(catalog)),
            writer.clone(),
            db_path.clone(),
        );

        let ctx = ToolContext {
            session: harness_core::SessionId::new(),
            agent: aid,
            cwd: tmp.path().to_path_buf(),
            allowed_writes: None,
            is_root: true,
            approval: None,
        };
        let out = tool.execute(json!({ "name": "echo" }), &ctx).await.unwrap();
        assert!(out.content.starts_with("skill activated:"));
        assert!(!out.is_error);

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE agent_id = ?1 AND kind = 'skill_attachment'",
                rusqlite::params![aid.as_uuid().to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn second_activation_is_a_noop() {
        let (_f, writer, aid, db_path) = test_setup().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut catalog = Catalog::new();
        catalog.insert(sample_skill(&tmp.path().join("echo"), "echo"));
        let tool = ActivateSkill::new(
            Arc::new(RwLock::new(catalog)),
            writer.clone(),
            db_path.clone(),
        );
        let ctx = ToolContext {
            session: harness_core::SessionId::new(),
            agent: aid,
            cwd: tmp.path().to_path_buf(),
            allowed_writes: None,
            is_root: true,
            approval: None,
        };
        tool.execute(json!({ "name": "echo" }), &ctx).await.unwrap();
        let out2 = tool.execute(json!({ "name": "echo" }), &ctx).await.unwrap();
        assert!(out2.content.contains("already active"));

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE agent_id = ?1 AND kind = 'skill_attachment'",
                rusqlite::params![aid.as_uuid().to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "dedupe should prevent a second row");
    }

    #[tokio::test]
    async fn unknown_skill_returns_error_output() {
        let (_f, writer, aid, db_path) = test_setup().await;
        let catalog = Catalog::new();
        let tool = ActivateSkill::new(Arc::new(RwLock::new(catalog)), writer, db_path);
        let ctx = ToolContext {
            session: harness_core::SessionId::new(),
            agent: aid,
            cwd: PathBuf::from("/tmp"),
            allowed_writes: None,
            is_root: true,
            approval: None,
        };
        let out = tool
            .execute(json!({ "name": "ghost" }), &ctx)
            .await
            .unwrap();
        assert!(out.is_error);
        assert!(out.content.contains("not found"));
    }
}

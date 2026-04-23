// IMPLEMENTS: D-158, D-204, D-293, D-294, D-295, D-296, D-297, D-298, D-299
mod activate;
mod catalog;
mod discover;
mod parse;
pub mod skills_sh;

use std::collections::BTreeMap;
use std::path::PathBuf;

pub use activate::{Activation, activate};
pub use catalog::Catalog;
pub use discover::{DiscoveryConfig, discover};
pub use parse::{ParseError, parse_skill_md};
pub use skills_sh::{ImportError, ImportReport, ParsedScript, import_from_dir, render_skill_md};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub location: PathBuf,
    pub license: Option<String>,
    pub compatibility: Option<String>,
    pub allowed_tools: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub scope: SkillScope,
    pub layout: SkillLayout,
}

impl Skill {
    #[must_use]
    pub fn directory(&self) -> &std::path::Path {
        self.location
            .parent()
            .expect("SKILL.md always has a parent directory")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillScope {
    Project,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillLayout {
    Native,
    Std,
    Claude,
}

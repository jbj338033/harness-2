use crate::Skill;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    skills: BTreeMap<String, Skill>,
}

impl Catalog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, skill: Skill) {
        if let Some(existing) = self.skills.get(&skill.name) {
            tracing::warn!(
                name = %skill.name,
                kept_location = %existing.location.display(),
                dropped_location = %skill.location.display(),
                "skill: name collision — keeping the earlier match"
            );
            return;
        }
        self.skills.insert(skill.name.clone(), skill);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Skill> {
        self.skills.values()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.skills.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SkillLayout, SkillScope};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn skill(name: &str, desc: &str) -> Skill {
        Skill {
            name: name.into(),
            description: desc.into(),
            location: PathBuf::from(format!("/tmp/{name}/SKILL.md")),
            license: None,
            compatibility: None,
            allowed_tools: None,
            metadata: BTreeMap::new(),
            scope: SkillScope::User,
            layout: SkillLayout::Std,
        }
    }

    #[test]
    fn first_insert_wins() {
        let mut c = Catalog::new();
        c.insert(skill("x", "first"));
        c.insert(skill("x", "second"));
        assert_eq!(c.get("x").unwrap().description, "first");
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn names_is_sorted() {
        let mut c = Catalog::new();
        c.insert(skill("b", "d"));
        c.insert(skill("a", "d"));
        c.insert(skill("c", "d"));
        assert_eq!(c.names(), vec!["a".to_string(), "b".into(), "c".into()]);
    }
}

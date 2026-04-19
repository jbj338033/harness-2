use crate::tool::Tool;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Default, Clone)]
pub struct Registry {
    tools: Arc<RwLock<HashMap<String, Arc<dyn Tool>>>>,
}

impl Registry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, tool: Arc<dyn Tool>) {
        self.tools
            .write()
            .expect("registry poisoned")
            .insert(tool.name().to_string(), tool);
    }

    pub fn unregister(&self, name: &str) {
        self.tools.write().expect("registry poisoned").remove(name);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools
            .read()
            .expect("registry poisoned")
            .get(name)
            .cloned()
    }

    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let mut v: Vec<_> = self
            .tools
            .read()
            .expect("registry poisoned")
            .keys()
            .cloned()
            .collect();
        v.sort();
        v
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tools.read().expect("registry poisoned").len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.read().expect("registry poisoned").is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{ToolContext, ToolError, ToolOutput};
    use async_trait::async_trait;
    use serde_json::{Value, json};

    struct DummyTool(&'static str);

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            self.0
        }
        fn description(&self) -> &'static str {
            "dummy"
        }
        fn input_schema(&self) -> Value {
            json!({"type": "object"})
        }
        async fn execute(
            &self,
            _input: Value,
            _ctx: &ToolContext,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::ok("ok"))
        }
    }

    #[test]
    fn register_and_lookup() {
        let r = Registry::new();
        r.register(Arc::new(DummyTool("foo")));
        r.register(Arc::new(DummyTool("bar")));
        assert_eq!(r.len(), 2);
        assert!(r.get("foo").is_some());
        assert!(r.get("missing").is_none());
        assert_eq!(r.names(), vec!["bar".to_string(), "foo".to_string()]);
    }

    #[test]
    fn overwrite_by_name() {
        let r = Registry::new();
        r.register(Arc::new(DummyTool("foo")));
        r.register(Arc::new(DummyTool("foo")));
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn unregister_removes_tool() {
        let r = Registry::new();
        r.register(Arc::new(DummyTool("foo")));
        assert_eq!(r.len(), 1);
        r.unregister("foo");
        assert_eq!(r.len(), 0);
        r.unregister("foo");
        assert_eq!(r.len(), 0);
    }
}

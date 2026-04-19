use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Write;

#[derive(Debug, Deserialize, Clone)]
pub struct AxNode {
    #[serde(rename = "nodeId")]
    pub node_id: String,
    #[serde(rename = "backendDOMNodeId", default)]
    pub backend_node_id: Option<i64>,
    #[serde(default)]
    pub role: AxValue,
    #[serde(default)]
    pub name: AxValue,
    #[serde(default, rename = "childIds")]
    pub child_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AxValue {
    #[serde(default)]
    pub value: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub rendered: String,
    pub refs: HashMap<String, i64>,
}

#[must_use]
pub fn render_snapshot(nodes: &[AxNode]) -> Snapshot {
    let mut by_id: HashMap<&str, &AxNode> = HashMap::new();
    for n in nodes {
        by_id.insert(n.node_id.as_str(), n);
    }
    let root = nodes.first();
    let Some(root) = root else {
        return Snapshot {
            rendered: String::new(),
            refs: HashMap::new(),
        };
    };
    let mut rendered = String::new();
    let mut refs: HashMap<String, i64> = HashMap::new();
    let mut counter: u32 = 0;
    walk(root, &by_id, &mut rendered, &mut refs, &mut counter, 0);
    Snapshot { rendered, refs }
}

fn walk(
    n: &AxNode,
    by_id: &HashMap<&str, &AxNode>,
    out: &mut String,
    refs: &mut HashMap<String, i64>,
    counter: &mut u32,
    depth: usize,
) {
    let role = n.role.value.as_str().unwrap_or("").to_string();
    let name = n.name.value.as_str().unwrap_or("").to_string();

    if is_renderable(&role) {
        *counter += 1;
        let r = format!("e{counter}");
        if let Some(node_id) = n.backend_node_id {
            refs.insert(r.clone(), node_id);
        }
        for _ in 0..depth {
            out.push_str("  ");
        }
        if name.is_empty() {
            writeln!(out, "{role} [ref={r}]").unwrap();
        } else {
            writeln!(out, "{role} \"{name}\" [ref={r}]").unwrap();
        }
    }

    for child_id in &n.child_ids {
        if let Some(child) = by_id.get(child_id.as_str()) {
            walk(child, by_id, out, refs, counter, depth + 1);
        }
    }
}

fn is_renderable(role: &str) -> bool {
    matches!(
        role,
        "button"
            | "link"
            | "textbox"
            | "searchbox"
            | "combobox"
            | "checkbox"
            | "radio"
            | "heading"
            | "image"
            | "tab"
            | "menuitem"
            | "listbox"
            | "option"
            | "main"
            | "navigation"
            | "form"
            | "article"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make(nodes: serde_json::Value) -> Vec<AxNode> {
        serde_json::from_value(nodes).unwrap()
    }

    #[test]
    fn renders_button_with_ref() {
        let nodes = make(json!([
            {"nodeId":"1","backendDOMNodeId":100,"role":{"value":"WebArea"},"name":{"value":""},"childIds":["2"]},
            {"nodeId":"2","backendDOMNodeId":101,"role":{"value":"button"},"name":{"value":"Sign in"},"childIds":[]}
        ]));
        let snap = render_snapshot(&nodes);
        assert!(snap.rendered.contains("button \"Sign in\""));
        assert!(snap.rendered.contains("[ref=e1]"));
        assert_eq!(snap.refs.get("e1").copied(), Some(101));
    }

    #[test]
    fn ignores_non_renderable_roles() {
        let nodes = make(json!([
            {"nodeId":"1","role":{"value":"none"},"name":{"value":""},"childIds":["2"]},
            {"nodeId":"2","role":{"value":"div"},"name":{"value":""},"childIds":[]}
        ]));
        let snap = render_snapshot(&nodes);
        assert!(snap.rendered.is_empty());
    }

    #[test]
    fn handles_empty_tree() {
        let snap = render_snapshot(&[]);
        assert!(snap.rendered.is_empty());
        assert!(snap.refs.is_empty());
    }
}

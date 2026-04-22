// IMPLEMENTS: D-392
//! ROS 2 MCP bridge scope. The decision is *not* to write native ROS
//! Rust code; an MCP adapter expresses topics, services, and actions
//! as MCP tools so the daemon stays platform-neutral.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ros2BridgeKind {
    Topic,
    Service,
    Action,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ros2McpScope {
    pub kind: Ros2BridgeKind,
    pub ros_name: String,
    pub mcp_tool_name: String,
    pub read_only: bool,
}

impl Ros2McpScope {
    /// A topic is read-only by default (subscribe). Mutating ROS calls
    /// (publish, service call, action goal) must explicitly set
    /// `read_only = false`.
    #[must_use]
    pub fn is_dangerous(&self) -> bool {
        !self.read_only && !matches!(self.kind, Ros2BridgeKind::Topic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_subscribe_is_safe() {
        let s = Ros2McpScope {
            kind: Ros2BridgeKind::Topic,
            ros_name: "/odom".into(),
            mcp_tool_name: "ros.odom.read".into(),
            read_only: true,
        };
        assert!(!s.is_dangerous());
    }

    #[test]
    fn action_call_is_dangerous() {
        let s = Ros2McpScope {
            kind: Ros2BridgeKind::Action,
            ros_name: "/move_base".into(),
            mcp_tool_name: "ros.move_base.call".into(),
            read_only: false,
        };
        assert!(s.is_dangerous());
    }
}

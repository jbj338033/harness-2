// IMPLEMENTS: D-065, D-102
use tree_sitter::{Node, Parser, Tree};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BashVerdict {
    Allow,
    Confirm(String),
    Deny(String),
}

#[must_use]
pub fn evaluate(command: &str) -> BashVerdict {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return BashVerdict::Allow;
    }
    if let Some(reason) = fork_bomb_text_match(trimmed) {
        return BashVerdict::Deny(reason);
    }
    let Some(tree) = parse(trimmed) else {
        return BashVerdict::Deny("could not parse bash input as a syntactic program".into());
    };
    walk(tree.root_node(), trimmed.as_bytes())
}

fn fork_bomb_text_match(input: &str) -> Option<String> {
    let condensed: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if condensed.contains(":(){:|:&};:") || condensed.contains(":(){:|:&};:") {
        return Some("fork bomb".into());
    }
    None
}

fn parse(source: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    let lang: tree_sitter::Language = tree_sitter_bash::LANGUAGE.into();
    parser.set_language(&lang).ok()?;
    parser.parse(source.as_bytes(), None)
}

fn walk(root: Node<'_>, source: &[u8]) -> BashVerdict {
    let mut deny: Option<String> = None;
    let mut confirm: Option<String> = None;

    let mut stack: Vec<Node<'_>> = vec![root];
    while let Some(node) = stack.pop() {
        if let Some(reason) = node_deny_reason(node, source) {
            deny.get_or_insert(reason);
        } else if let Some(reason) = node_confirm_reason(node, source)
            && confirm.is_none()
        {
            confirm = Some(reason);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    if let Some(r) = deny {
        BashVerdict::Deny(r)
    } else if let Some(r) = confirm {
        BashVerdict::Confirm(r)
    } else {
        BashVerdict::Allow
    }
}

fn node_text<'s>(node: Node<'_>, source: &'s [u8]) -> &'s str {
    std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).unwrap_or("")
}

fn node_deny_reason(node: Node<'_>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "ansi_c_string" => Some("$'…' ANSI-C strings can encode arbitrary bytes".into()),
        "process_substitution" => Some("process substitution <(…) / >(…) can hide commands".into()),
        "command" => command_deny_reason(node, source),
        "pipeline" => pipeline_deny_reason(node, source),
        "file_redirect" => redirect_to_disk_text(node_text(node, source)),
        "heredoc_body" => heredoc_recurse_reason(node_text(node, source)),
        _ => None,
    }
}

fn redirect_to_disk_text(text: &str) -> Option<String> {
    if text.contains("/dev/sd")
        || text.contains("/dev/nvme")
        || text.contains("/dev/hd")
        || text.contains("/dev/disk")
    {
        return Some("redirect to raw disk device".into());
    }
    None
}

fn heredoc_recurse_reason(body: &str) -> Option<String> {
    match evaluate(body) {
        BashVerdict::Deny(reason) => Some(format!("heredoc body refused: {reason}")),
        _ => None,
    }
}

fn node_confirm_reason(node: Node<'_>, source: &[u8]) -> Option<String> {
    if node.kind() == "command" {
        return command_confirm_reason(node, source);
    }
    None
}

fn command_name<'a>(node: Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name("name")
        .map(|n| node_text(n, source))
}

fn command_args<'a>(node: Node<'_>, source: &'a [u8]) -> Vec<&'a str> {
    let mut args = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "command_name") {
            continue;
        }
        if child.is_named() && child.kind() != "comment" {
            args.push(node_text(child, source));
        }
    }
    args
}

fn command_deny_reason(node: Node<'_>, source: &[u8]) -> Option<String> {
    let name = command_name(node, source)?;
    let args = command_args(node, source);
    let bare = bare_program(name);

    match bare {
        "rm" if args_target_root(&args) => Some("destructive wipe of root filesystem".into()),
        "mkfs" => Some("filesystem creation".into()),
        n if n.starts_with("mkfs.") => Some("filesystem creation".into()),
        "dd" if dd_writes_disk_device(&args) => Some("overwrite raw disk device".into()),
        "chmod" if chmod_777_root(&args) => Some("chmod 777 on filesystem root".into()),
        "xargs" if xargs_runs_shell(&args) => {
            Some("xargs piping into bash/sh evades validation".into())
        }
        "sudo" => sudo_inner_deny(&args),
        _ => None,
    }
}

fn sudo_inner_deny(args: &[&str]) -> Option<String> {
    // Skip sudo flags then re-evaluate the inner program as if it were the
    // outer command — this catches `sudo rm -rf /`, `sudo dd …`, etc.
    let mut i = 0;
    while i < args.len() && args[i].starts_with('-') {
        i += 1;
    }
    if i >= args.len() {
        return None;
    }
    let rest = args[i..].join(" ");
    match evaluate(&rest) {
        BashVerdict::Deny(reason) => Some(reason),
        _ => None,
    }
}

fn command_confirm_reason(node: Node<'_>, source: &[u8]) -> Option<String> {
    let name = command_name(node, source)?;
    let args = command_args(node, source);
    let bare = bare_program(name);
    match bare {
        "sudo" => Some("sudo execution".into()),
        "kill" if args.contains(&"-9") => Some("force kill".into()),
        "git" => git_confirm(&args),
        "rm" if rm_recursive(&args) && !args_target_root(&args) => Some("recursive rm".into()),
        _ => None,
    }
}

fn bare_program(raw: &str) -> &str {
    raw.rsplit('/').next().unwrap_or(raw)
}

fn rm_recursive(args: &[&str]) -> bool {
    args.iter().any(|a| {
        if !a.starts_with('-') {
            return false;
        }
        let body = a.trim_start_matches('-');
        body.contains('r') || body.contains('R')
    })
}

fn args_target_root(args: &[&str]) -> bool {
    args.iter().any(|a| {
        let stripped = a.trim();
        stripped == "/" || stripped == "/*" || stripped == "$HOME" || stripped == "~"
    })
}

fn dd_writes_disk_device(args: &[&str]) -> bool {
    args.iter().any(|a| {
        let v = a.strip_prefix("of=").unwrap_or("");
        v.starts_with("/dev/sd")
            || v.starts_with("/dev/nvme")
            || v.starts_with("/dev/hd")
            || v.starts_with("/dev/disk")
    })
}

fn chmod_777_root(args: &[&str]) -> bool {
    let mut recursive = false;
    let mut mode_777 = false;
    let mut on_root = false;
    for a in args {
        if a.starts_with('-') {
            let body = a.trim_start_matches('-');
            if body.contains('R') {
                recursive = true;
            }
        } else if *a == "777" || *a == "0777" {
            mode_777 = true;
        } else if *a == "/" {
            on_root = true;
        }
    }
    recursive && mode_777 && on_root
}

fn xargs_runs_shell(args: &[&str]) -> bool {
    args.iter()
        .any(|a| matches!(*a, "bash" | "sh" | "/bin/bash" | "/bin/sh"))
}

fn git_confirm(args: &[&str]) -> Option<String> {
    let joined = args.join(" ");
    if joined.contains("push") && (joined.contains("--force") || joined.contains("-f ")) {
        return Some("git force push".into());
    }
    if joined.contains("reset") && joined.contains("--hard") {
        return Some("git reset --hard".into());
    }
    None
}

fn pipeline_deny_reason(node: Node<'_>, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node
        .children(&mut cursor)
        .filter(|c| c.kind() == "command")
        .collect();
    if children.len() < 2 {
        return None;
    }
    for cmd in children.iter().skip(1) {
        if let Some(name) = command_name(*cmd, source) {
            let bare = bare_program(name);
            if matches!(bare, "bash" | "sh") {
                return Some(format!("piping into {bare} evades validation"));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allow(cmd: &str) {
        match evaluate(cmd) {
            BashVerdict::Allow => {}
            other => panic!("expected allow for {cmd:?}, got {other:?}"),
        }
    }

    fn deny(cmd: &str) {
        match evaluate(cmd) {
            BashVerdict::Deny(_) => {}
            other => panic!("expected deny for {cmd:?}, got {other:?}"),
        }
    }

    fn confirm(cmd: &str) {
        match evaluate(cmd) {
            BashVerdict::Confirm(_) => {}
            other => panic!("expected confirm for {cmd:?}, got {other:?}"),
        }
    }

    #[test]
    fn allows_basic_commands() {
        allow("");
        allow("ls -la");
        allow("cargo test");
        allow("git status");
        allow("echo hello world");
    }

    #[test]
    fn denies_destructive_root() {
        deny("rm -rf /");
        deny("rm -rf / # bye");
    }

    #[test]
    fn denies_disk_writes() {
        deny("mkfs.ext4 /dev/sda1");
        deny("dd if=/dev/zero of=/dev/sda");
        deny("echo hi > /dev/sda");
    }

    #[test]
    fn denies_fork_bomb() {
        deny(":(){ :|: & };:");
    }

    #[test]
    fn denies_chmod_777_root() {
        deny("chmod -R 777 /");
    }

    #[test]
    fn denies_ansi_c_string() {
        deny("echo $'\\x72\\x6d -rf /'");
    }

    #[test]
    fn denies_process_substitution() {
        deny("diff <(ls) <(ls -a)");
        deny("tee >(cat)");
    }

    #[test]
    fn denies_pipe_into_bash() {
        deny("printf 'rm -rf /' | bash");
        deny("echo whoami | sh");
    }

    #[test]
    fn denies_xargs_into_bash() {
        deny("echo whoami | xargs bash");
        deny("echo whoami | xargs sh");
    }

    #[test]
    fn denies_destructive_inside_heredoc() {
        // Heredoc body is parsed by tree-sitter, which surfaces the inner
        // command as a regular `command` node — so AST walk catches it.
        deny("cat <<EOF\nrm -rf /\nEOF");
    }

    #[test]
    fn confirms_intentional_actions() {
        confirm("git push origin main --force");
        confirm("git reset --hard HEAD~5");
        confirm("sudo apt install foo");
        confirm("rm -rf build");
        confirm("kill -9 12345");
    }

    #[test]
    fn allows_safe_pipelines() {
        allow("ls | sort | uniq");
        allow("echo hi | wc -c");
    }

    #[test]
    fn allows_safe_dd_invocations() {
        allow("dd if=input.bin of=output.bin bs=1M");
    }
}

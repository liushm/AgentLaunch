use crate::config::AgentKind;
use crate::discovery::AgentDescriptor;

pub fn builtin_descriptors() -> Vec<AgentDescriptor> {
    vec![
        AgentDescriptor {
            id: "antigravity",
            name: "Antigravity",
            kind: AgentKind::Electron,
            win_appx_prefixes: &[],
            win_appx_rel_exes: &[],
            win_squirrel_dir: None,
            win_program_rel_paths: &[
                "antigravity\\Antigravity.exe",
                "Antigravity\\Antigravity.exe",
            ],
            win_uninstall_keywords: &["Antigravity"],
            win_exe_name: "Antigravity.exe",
            mac_app_bundle: Some("Antigravity.app"),
            mac_bundle_id: None,
            cli_names: &["antigravity"],
        },
        AgentDescriptor {
            id: "codex",
            name: "Codex Desktop",
            kind: AgentKind::Electron,
            win_appx_prefixes: &["OpenAI.Codex"],
            win_appx_rel_exes: &["app\\ChatGPT.exe", "app\\Codex.exe"],
            win_squirrel_dir: Some("Codex"),
            win_program_rel_paths: &["Codex\\ChatGPT.exe", "Codex\\Codex.exe", "OpenAI\\Codex\\Codex.exe"],
            win_uninstall_keywords: &["OpenAI Codex", "Codex"],
            win_exe_name: "ChatGPT.exe",
            mac_app_bundle: Some("Codex.app"),
            mac_bundle_id: Some("com.openai.codex"),
            cli_names: &[],
        },
        AgentDescriptor {
            id: "claude-desktop",
            name: "Claude Desktop",
            kind: AgentKind::Electron,
            win_appx_prefixes: &["AnthropicPBC.Claude", "Claude"],
            win_appx_rel_exes: &["app\\claude.exe", "Claude.exe"],
            win_squirrel_dir: Some("AnthropicClaude"),
            win_program_rel_paths: &[
                "Claude\\Claude.exe",
                "AnthropicClaude\\Claude.exe",
            ],
            win_uninstall_keywords: &["Claude"],
            win_exe_name: "Claude.exe",
            mac_app_bundle: Some("Claude.app"),
            mac_bundle_id: None,
            cli_names: &[],
        },
        AgentDescriptor {
            id: "cursor",
            name: "Cursor",
            kind: AgentKind::Electron,
            win_appx_prefixes: &[],
            win_appx_rel_exes: &[],
            win_squirrel_dir: None,
            win_program_rel_paths: &["cursor\\Cursor.exe", "Cursor\\Cursor.exe"],
            win_uninstall_keywords: &["Cursor"],
            win_exe_name: "Cursor.exe",
            mac_app_bundle: Some("Cursor.app"),
            mac_bundle_id: None,
            cli_names: &["cursor"],
        },
        AgentDescriptor {
            id: "vscode",
            name: "VS Code",
            kind: AgentKind::Electron,
            win_appx_prefixes: &[],
            win_appx_rel_exes: &[],
            win_squirrel_dir: None,
            win_program_rel_paths: &["Microsoft VS Code\\Code.exe"],
            win_uninstall_keywords: &["Microsoft Visual Studio Code"],
            win_exe_name: "Code.exe",
            mac_app_bundle: Some("Visual Studio Code.app"),
            mac_bundle_id: None,
            cli_names: &[],
        },
    ]
}

pub fn find_descriptor(id: &str) -> Option<AgentDescriptor> {
    builtin_descriptors()
        .into_iter()
        .find(|d| d.id.eq_ignore_ascii_case(id))
}

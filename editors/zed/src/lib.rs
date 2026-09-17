use zed_extension_api as zed;

struct CssLabExtension;

impl zed::Extension for CssLabExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        let node = worktree
            .shell_env()
            .into_iter()
            .find(|(name, _)| name == "CSS_LAB_NODE")
            .map(|(_, value)| value)
            .or_else(|| worktree.which("node"))
            .ok_or_else(|| {
                "CSS Lab LSP requires node on Zed's worktree PATH (or CSS_LAB_NODE)".to_string()
            })?;

        let root = worktree.root_path();
        let server = format!(
            "{}/packages/css-lsp/dist/server.js",
            root.trim_end_matches('/')
        );

        Ok(zed::Command {
            command: node,
            args: vec![server, "--stdio".to_string()],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(CssLabExtension);

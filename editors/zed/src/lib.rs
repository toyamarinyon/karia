use std::env;
use zed::settings::LspSettings;
use zed_extension_api::{self as zed, LanguageServerId, Result, Worktree};

const SERVER_NAME: &str = "karia";
const PACKAGE_NAME: &str = "karia-lsp";
const SERVER_PATH: &str = "node_modules/karia-lsp/dist/server.js";
const PACKAGE_JSON_PATH: &str = "node_modules/karia-lsp/package.json";

struct KariaExtension {
    cached_server_path: Option<String>,
}

impl KariaExtension {
    /// server.js from a project-local install, or install the package into the
    /// extension work dir and use that.
    fn server_path(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<String> {
        let root = worktree.root_path();
        if worktree.read_text_file(PACKAGE_JSON_PATH).is_ok() {
            return Ok(format!("{}/{}", root.trim_end_matches('/'), SERVER_PATH));
        }

        let server_exists = std::fs::metadata(SERVER_PATH).is_ok_and(|m| m.is_file());
        if self.cached_server_path.is_some() && server_exists {
            return Ok(self.cached_server_path.clone().unwrap());
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let version = zed::npm_package_latest_version(PACKAGE_NAME)?;
        let installed = zed::npm_package_installed_version(PACKAGE_NAME)?;
        if !server_exists || installed.as_deref() != Some(version.as_str()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );
            let result = zed::npm_install_package(PACKAGE_NAME, &version);
            match result {
                Ok(()) => {
                    if !std::fs::metadata(SERVER_PATH).is_ok_and(|m| m.is_file()) {
                        Err(format!(
                            "installed package '{PACKAGE_NAME}' did not contain expected path '{SERVER_PATH}'"
                        ))?;
                    }
                }
                Err(error) => {
                    if !server_exists {
                        Err(error)?;
                    }
                }
            }
        }
        let server_path = env::current_dir()
            .map_err(|e| e.to_string())?
            .join(SERVER_PATH)
            .to_string_lossy()
            .to_string();
        self.cached_server_path = Some(server_path.clone());
        Ok(server_path)
    }
}

impl zed::Extension for KariaExtension {
    fn new() -> Self {
        Self {
            cached_server_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        // 1. Explicit override: lsp.karia.binary in settings.json
        let settings = LspSettings::for_worktree(SERVER_NAME, worktree).ok();
        if let Some(binary) = settings.and_then(|s| s.binary) {
            if let Some(path) = binary.path {
                return Ok(zed::Command {
                    command: path,
                    args: binary.arguments.unwrap_or_default(),
                    env: worktree.shell_env(),
                });
            }
        }

        // 2. Project-local node_modules, else the extension work dir.
        let server_path = self.server_path(language_server_id, worktree)?;

        let node = zed::node_binary_path()
            .or_else(|_| worktree.which("node").ok_or("node not found".to_string()))
            .or_else(|_| {
                worktree
                    .shell_env()
                    .into_iter()
                    .find(|(name, _)| name == "CSS_LAB_NODE")
                    .map(|(_, value)| value)
                    .ok_or_else(|| {
                        "karia requires node on PATH (or CSS_LAB_NODE)".to_string()
                    })
            })?;

        Ok(zed::Command {
            command: node,
            args: vec![server_path, "--stdio".to_string()],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(KariaExtension);

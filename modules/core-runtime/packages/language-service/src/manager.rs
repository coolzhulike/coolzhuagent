use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use lsp_types::Position;
use tokio::sync::Mutex;

use crate::client::LspClient;
use crate::error::LspError;
use crate::types::{
    normalize_extension, FileDiagnostics, LspContextEnrichment, LspServerConfig, SymbolLocation,
    WorkspaceDiagnostics,
};

pub struct LspManager {
    server_configs: BTreeMap<String, LspServerConfig>,
    extension_map: BTreeMap<String, String>,
    clients: Mutex<BTreeMap<String, Arc<LspClient>>>,
}

impl LspManager {
    pub fn new(server_configs: Vec<LspServerConfig>) -> Result<Self, LspError> {
        let mut configs_by_name = BTreeMap::new();
        let mut extension_map = BTreeMap::new();

        for mut config in server_configs {
            config.workspace_root = config.workspace_root.canonicalize()?;
            if config.name.trim().is_empty() || config.command.trim().is_empty() {
                return Err(LspError::Protocol(
                    "LSP server name and command must be nonempty".to_string(),
                ));
            }
            for extension in config.extension_to_language.keys() {
                let normalized = normalize_extension(extension);
                if let Some(existing_server) =
                    extension_map.insert(normalized.clone(), config.name.clone())
                {
                    return Err(LspError::DuplicateExtension {
                        extension: normalized,
                        existing_server,
                        new_server: config.name.clone(),
                    });
                }
            }
            configs_by_name.insert(config.name.clone(), config);
        }

        Ok(Self {
            server_configs: configs_by_name,
            extension_map,
            clients: Mutex::new(BTreeMap::new()),
        })
    }

    #[must_use]
    pub fn supports_path(&self, path: &Path) -> bool {
        path.extension().is_some_and(|extension| {
            let normalized = normalize_extension(extension.to_string_lossy().as_ref());
            self.extension_map.contains_key(&normalized)
        })
    }

    pub async fn open_document(&self, path: &Path, text: &str) -> Result<(), LspError> {
        let (client, path) = self.client_for_path(path).await?;
        client.open_document(&path, text).await
    }

    pub async fn sync_document_from_disk(&self, path: &Path) -> Result<(), LspError> {
        let (client, path) = self.client_for_path(path).await?;
        let contents = std::fs::read_to_string(&path)?;
        client.change_document(&path, &contents).await?;
        client.save_document(&path).await
    }

    pub async fn change_document(&self, path: &Path, text: &str) -> Result<(), LspError> {
        let (client, path) = self.client_for_path(path).await?;
        client.change_document(&path, text).await
    }

    pub async fn save_document(&self, path: &Path) -> Result<(), LspError> {
        let (client, path) = self.client_for_path(path).await?;
        client.save_document(&path).await
    }

    pub async fn close_document(&self, path: &Path) -> Result<(), LspError> {
        let (client, path) = self.client_for_path(path).await?;
        client.close_document(&path).await
    }

    pub async fn go_to_definition(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Vec<SymbolLocation>, LspError> {
        let (client, path) = self.client_for_path(path).await?;
        let mut locations = client.go_to_definition(&path, position).await?;
        locations
            .retain(|location| location_within_workspace(&location.path, client.workspace_root()));
        dedupe_locations(&mut locations);
        Ok(locations)
    }

    pub async fn find_references(
        &self,
        path: &Path,
        position: Position,
        include_declaration: bool,
    ) -> Result<Vec<SymbolLocation>, LspError> {
        let (client, path) = self.client_for_path(path).await?;
        let mut locations = client
            .find_references(&path, position, include_declaration)
            .await?;
        locations
            .retain(|location| location_within_workspace(&location.path, client.workspace_root()));
        dedupe_locations(&mut locations);
        Ok(locations)
    }

    pub async fn collect_workspace_diagnostics(&self) -> Result<WorkspaceDiagnostics, LspError> {
        let clients = self
            .clients
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut files = Vec::new();

        for client in clients {
            for (uri, diagnostics) in client.diagnostics_snapshot().await {
                let Ok(uri_path) = url::Url::parse(&uri).and_then(|url| {
                    url.to_file_path()
                        .map_err(|()| url::ParseError::RelativeUrlWithoutBase)
                }) else {
                    continue;
                };
                let Ok(path) = uri_path.canonicalize() else {
                    continue;
                };
                if diagnostics.is_empty() || !path.starts_with(client.workspace_root()) {
                    continue;
                }
                files.push(FileDiagnostics {
                    path,
                    uri,
                    diagnostics,
                });
            }
        }

        files.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(WorkspaceDiagnostics { files })
    }

    /// 仅报告已实际启动且读循环仍存活的服务；不因配置存在而暗示服务可用。
    pub async fn is_running(&self) -> bool {
        let clients = self.clients.lock().await;
        !clients.is_empty() && clients.values().all(|client| client.is_alive())
    }

    pub async fn context_enrichment(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<LspContextEnrichment, LspError> {
        Ok(LspContextEnrichment {
            file_path: path.to_path_buf(),
            diagnostics: self.collect_workspace_diagnostics().await?,
            definitions: self.go_to_definition(path, position).await?,
            references: self.find_references(path, position, true).await?,
        })
    }

    pub async fn shutdown(&self) -> Result<(), LspError> {
        let mut clients = self.clients.lock().await;
        let drained = clients.values().cloned().collect::<Vec<_>>();
        clients.clear();
        drop(clients);

        for client in drained {
            client.shutdown().await?;
        }
        Ok(())
    }

    async fn client_for_path(
        &self,
        path: &Path,
    ) -> Result<(Arc<LspClient>, std::path::PathBuf), LspError> {
        let path = path.canonicalize()?;
        let extension = path
            .extension()
            .map(|extension| normalize_extension(extension.to_string_lossy().as_ref()))
            .ok_or_else(|| LspError::UnsupportedDocument(path.to_path_buf()))?;
        let server_name = self
            .extension_map
            .get(&extension)
            .cloned()
            .ok_or_else(|| LspError::UnsupportedDocument(path.to_path_buf()))?;

        let config = self
            .server_configs
            .get(&server_name)
            .cloned()
            .ok_or_else(|| LspError::UnknownServer(server_name.clone()))?;
        if !path.starts_with(&config.workspace_root) {
            return Err(LspError::Protocol(format!(
                "document is outside configured workspace: {}",
                path.display()
            )));
        }

        let mut clients = self.clients.lock().await;
        if let Some(client) = clients.get(&server_name) {
            if !client.is_alive() {
                return Err(LspError::Protocol(format!(
                    "language server `{server_name}` stopped; close and reopen the workspace to retry"
                )));
            }
            return Ok((client.clone(), path));
        }

        let client = Arc::new(LspClient::connect(config).await?);
        clients.insert(server_name, client.clone());
        Ok((client, path))
    }
}

fn location_within_workspace(path: &Path, workspace: &Path) -> bool {
    path.canonicalize()
        .is_ok_and(|canonical| canonical.starts_with(workspace))
}

fn dedupe_locations(locations: &mut Vec<SymbolLocation>) {
    let mut seen = BTreeSet::new();
    locations.retain(|location| {
        seen.insert((
            location.path.clone(),
            location.range.start.line,
            location.range.start.character,
            location.range.end.line,
            location.range.end.character,
        ))
    });
}

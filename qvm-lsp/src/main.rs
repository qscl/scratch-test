use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::path::Path as FilePath;
use std::sync::{Arc, RwLock};
use tokio::task;

use regex::Regex;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Configuration {
    pub has_configuration_capability: bool,
    pub has_workspace_folder_capability: bool,
    pub has_diagnostic_related_information_capability: bool,
}

#[derive(Debug)]
struct ExampleSettings {
    pub max_number_of_problems: u32,
}

// XXX Remove this or add real settings in
const DEFAULT_SETTINGS: ExampleSettings = ExampleSettings {
    max_number_of_problems: 1000,
};

#[derive(Debug)]
struct Backend {
    client: Client,
    configuration: Arc<RwLock<Configuration>>,
    settings: Arc<RwLock<BTreeMap<String, ExampleSettings>>>,
    documents: Arc<RwLock<BTreeMap<String, String>>>,
    compiler: Arc<RwLock<Option<qvm::compile::Compiler>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let capabilities = params.capabilities;

        let mut config = self.configuration.write().unwrap();

        config.has_configuration_capability = match &capabilities.workspace {
            Some(workspace) => workspace.configuration.map_or(false, |x| x),
            None => false,
        };

        config.has_workspace_folder_capability = match &capabilities.workspace {
            Some(workspace) => workspace.workspace_folders.map_or(false, |x| x),
            None => false,
        };

        config.has_diagnostic_related_information_capability = match &capabilities.text_document {
            Some(text_document) => text_document
                .publish_diagnostics
                .as_ref()
                .map_or(false, |publish_diagnostics| {
                    publish_diagnostics.related_information.map_or(false, |x| x)
                }),
            None => false,
        };

        let result = InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    // Although the tutorial uses INCREMENTAL, we have to use FULL because from
                    // what I can tell, TowerLSP doesn't help you apply INCREMENTAL updates.
                    // TextDocumentSyncKind::INCREMENTAL,
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(true),
                    ..Default::default()
                }),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(true),
                }),
                ..Default::default()
            },
            ..Default::default()
        };

        Ok(result)
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;

        let (has_configuration_capability, has_workspace_folder_capability) = {
            let config = self.configuration.read().unwrap();
            (
                config.has_configuration_capability,
                config.has_workspace_folder_capability,
            )
        };
        if has_configuration_capability {
            self.client
                .register_capability(vec![Registration {
                    id: "config".to_string(),
                    method: "workspace/didChangeConfiguration".to_string(),
                    register_options: None,
                }])
                .await
                .unwrap();
        }

        if has_workspace_folder_capability {
            self.client
                .register_capability(vec![Registration {
                    id: "workspace".to_string(),
                    method: "workspace/didChangeWorkspaceFolders".to_string(),
                    register_options: None,
                }])
                .await
                .unwrap();
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    // XXX REMOVE
    async fn did_change_configuration(&self, _params: DidChangeConfigurationParams) {
        if self
            .configuration
            .read()
            .unwrap()
            .has_configuration_capability
        {
            self.settings.write().unwrap().clear();
        }

        let mut futures = Vec::new();
        for (uri, document) in self.documents.read().unwrap().iter() {
            futures.push(self.on_change(Url::parse(uri).unwrap(), document.clone(), None));
        }

        for future in futures {
            future.await;
        }
    }

    async fn did_change_workspace_folders(&self, _params: DidChangeWorkspaceFoldersParams) {
        eprintln!("Workspace folder change event received.");
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        eprintln!("FILE CONTENTS CHANGED");
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        // NOTE: If we change this to INCREMENTAL we'll need to recompute the new document text
        let text = params.content_changes.swap_remove(0).text;

        self.on_change(uri, text, Some(version)).await
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let text = params.text_document.text;

        self.on_change(uri, text, Some(version)).await
    }

    async fn did_change_watched_files(&self, _params: DidChangeWatchedFilesParams) {
        eprintln!("We receved a file change event.");
    }

    // TODO: Implement real completion logic
    async fn completion(&self, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let completions = vec![
            CompletionItem {
                label: "TypeScript".to_string(),
                kind: Some(CompletionItemKind::TEXT),
                data: Some(1.into()),
                ..Default::default()
            },
            CompletionItem {
                label: "JavaScript".to_string(),
                kind: Some(CompletionItemKind::TEXT),
                data: Some(2.into()),
                ..Default::default()
            },
        ];
        Ok(Some(CompletionResponse::Array(completions)))
    }

    // TODO: Implement real completion logic
    async fn completion_resolve(&self, item: CompletionItem) -> Result<CompletionItem> {
        if let Some(data) = &item.data {
            if let serde_json::Value::Number(n) = data {
                if n.as_u64() == Some(1) {
                    return Ok(CompletionItem {
                        detail: Some("TypeScript details".to_string()),
                        documentation: Some(Documentation::String(
                            "TypeScript documentation".to_string(),
                        )),
                        ..item
                    });
                } else if n.as_u64() == Some(2) {
                    return Ok(CompletionItem {
                        detail: Some("JavaScript details".to_string()),
                        documentation: Some(Documentation::String(
                            "JavaScript documentation".to_string(),
                        )),
                        ..item
                    });
                }
            }
        }
        Ok(item)
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri;
        let documents = self.documents.read().unwrap();

        let text = documents
            .get(&uri.to_string())
            .ok_or(Error::invalid_params("Invalid document URI.".to_string()))?
            .clone();

        let mut lenses = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let range = Range {
                start: Position {
                    line: i as u32,
                    character: 0,
                },
                end: Position {
                    line: i as u32,
                    character: line.len() as u32,
                },
            };
            let command = Command {
                title: "Show references".to_string(),
                command: "showReferences".to_string(),

                // TODO: I think we'll want to turn the arguments into a struct
                // of some sort
                arguments: Some(vec![
                    uri.to_string().into(), /* , range.clone().into() */
                ]),
            };
            lenses.push(CodeLens {
                range,
                command: Some(command),
                data: None,
            });
        }
        Ok(Some(lenses))
    }
}

impl Backend {
    async fn on_change(&self, uri: Url, text: String, version: Option<i32>) {
        {
            let mut documents = self.documents.write().unwrap();
            documents.insert(uri.to_string(), text.clone());
        }

        let has_diagnostic_related_information_capability = {
            let config = self.configuration.read().unwrap();
            config.has_diagnostic_related_information_capability
        };

        let file = uri.path().to_string();

        // XXX Refactor this into a separate function
        let folder = FilePath::new(uri.path())
            .parent()
            .map(|p| p.to_str().unwrap())
            .map(String::from);
        let ast = match qvm::parser::parse_schema(&file, &text) {
            Ok(ast) => ast,
            Err(e) => {
                eprintln!("ERROR (that should be sent as a diagnostic): {:?}", e);
                return;
            }
        };

        let schema = qvm::compile::schema::Schema::new(file, folder);
        let compiler = self.compiler.clone();

        eprintln!("GOING TO COMPILE");
        task::spawn_blocking(move || {
            eprintln!("IN COMPILER");
            let compiler = maybe_init_compiler(compiler);
            // let compiler = qvm::compile::Compiler::new().unwrap();
            eprintln!("COMPILNG!");
            let compile_result = compiler.compile_schema_ast(schema.clone(), &ast);
            eprintln!("COMPILE RESULT: {:?}", compile_result);
        })
        .await
        .expect("compiler thread failed");

        // XXX send the compile errors as diagnostics up to the client

        let pattern = Regex::new(r"\b[A-Z]{2,}\b").unwrap();

        let mut problems = 0;
        let mut diagnostics = Vec::new();

        let allowed_problems = 10;
        for m in pattern.find_iter(&text) {
            if problems >= allowed_problems {
                break;
            }

            let mut diagnostic = Diagnostic {
                severity: Some(DiagnosticSeverity::WARNING),
                range: Range {
                    start: position_of_index(&text, m.start()),
                    end: position_of_index(&text, m.end()),
                },
                message: format!("{} is all uppercase.", m.as_str()),
                source: Some("ex".to_string()),
                ..Default::default()
            };
            if has_diagnostic_related_information_capability {
                diagnostic.related_information = Some(vec![
                    DiagnosticRelatedInformation {
                        location: Location {
                            uri: uri.clone(),
                            range: Range {
                                start: position_of_index(&text, m.start()),
                                end: position_of_index(&text, m.end()),
                            },
                        },
                        message: "Spelling matters".to_string(),
                    },
                    DiagnosticRelatedInformation {
                        location: Location {
                            uri: uri.clone(),
                            range: Range {
                                start: position_of_index(&text, m.start()),
                                end: position_of_index(&text, m.end()),
                            },
                        },
                        message: "Particularly for names".to_string(),
                    },
                ]);
            }
            diagnostics.push(diagnostic);

            problems += 1;
        }

        self.client
            .publish_diagnostics(uri.clone(), diagnostics, version)
            .await
    }

    async fn run_query(&self, params: serde_json::Value) -> Result<String> {
        eprintln!("PARAMS: {:?}", params);
        Ok("foo".to_string())
    }
}

fn maybe_init_compiler(
    compiler_ref: Arc<RwLock<Option<qvm::compile::Compiler>>>,
) -> qvm::compile::Compiler {
    {
        let compiler_read = &*(compiler_ref.read().unwrap());
        if let Some(compiler) = compiler_read {
            return compiler.clone();
        }
    }

    let mut locked_compiler = compiler_ref.write().unwrap();
    if let Some(compiler) = &*locked_compiler {
        return compiler.clone();
    }

    let compiler = qvm::compile::Compiler::new().unwrap();
    locked_compiler.replace(compiler.clone());
    compiler
}

fn position_of_index(text: &str, index: usize) -> Position {
    let mut line = 0;
    let mut column = 0;
    for (i, c) in text.char_indices() {
        if i == index {
            break;
        }
        if c == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    Position {
        line,
        character: column,
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| Backend {
        client,
        configuration: Arc::new(RwLock::new(Configuration {
            has_configuration_capability: false,
            has_workspace_folder_capability: false,
            has_diagnostic_related_information_capability: false,
        })),
        settings: Arc::new(RwLock::new(BTreeMap::new())),
        documents: Arc::new(RwLock::new(BTreeMap::new())),
        compiler: Arc::new(RwLock::new(None)),
    })
    .custom_method("qvm/runQuery", Backend::run_query)
    .finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}

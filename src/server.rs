use crate::{
    compat::Document,
    document::{KEYWORDS, TOKEN_NAMES},
    fnd::{Database, Entry},
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
};
use tower_lsp::{jsonrpc::Result, lsp_types::*, Client, LanguageServer};

fn markup(value: String) -> MarkupContent {
    MarkupContent {
        kind: MarkupKind::Markdown,
        value,
    }
}
fn entry_item(e: &Entry) -> CompletionItem {
    if let Some(value) = crate::car_cdr::hover(&e.name) {
        return CompletionItem {
            label: e.name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(format!("{}(lst) — car/cdr selector", e.name)),
            documentation: Some(Documentation::MarkupContent(markup(value))),
            ..Default::default()
        };
    }
    let mut params: Vec<String> = e
        .params
        .iter()
        .filter(|p| !p.optional)
        .map(|p| p.name.clone())
        .collect();
    params.extend(
        e.params
            .iter()
            .filter(|p| p.optional)
            .map(|p| format!("[{}]", p.name)),
    );
    let label = if params.len() > 6 {
        format!("{} ...", params[..5].join(" "))
    } else {
        params.join(" ")
    };
    let ret = e
        .return_type
        .as_ref()
        .map(|r| format!(" => {r}"))
        .unwrap_or_default();
    CompletionItem {
        label: e.name.clone(),
        kind: Some(CompletionItemKind::FUNCTION),
        detail: Some(format!("({label}){ret}")),
        documentation: Some(Documentation::MarkupContent(markup(e.markdown()))),
        ..Default::default()
    }
}
pub struct Backend {
    client: Client,
    db: Arc<Database>,
    documents: RwLock<HashMap<Url, Arc<Document>>>,
    completions: Vec<CompletionItem>,
    max_completions: usize,
    completion_index: Vec<(String, usize)>,
    hover_text: HashMap<String, String>,
    utf32: AtomicBool,
}
impl Backend {
    pub fn new(client: Client, db: Arc<Database>, max_completions: usize) -> Self {
        let mut completions: Vec<_> = KEYWORDS
            .iter()
            .filter(|kw| db.get(kw).is_none())
            .map(|kw| CompletionItem {
                label: (*kw).into(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("SKILL keyword".into()),
                ..Default::default()
            })
            .collect();
        completions.extend(db.entries.iter().map(entry_item));
        let mut completion_index: Vec<_> = completions
            .iter()
            .enumerate()
            .map(|(i, c)| (c.label.to_lowercase(), i))
            .collect();
        completion_index.sort();
        let hover_text = db
            .entries
            .iter()
            .map(|e| (e.name.clone(), e.markdown()))
            .collect();
        Self {
            client,
            db,
            documents: RwLock::new(HashMap::new()),
            completions,
            max_completions,
            completion_index,
            hover_text,
            utf32: AtomicBool::new(false),
        }
    }
    fn document(&self, uri: &Url) -> Arc<Document> {
        self.documents
            .read()
            .unwrap()
            .get(uri)
            .cloned()
            .unwrap_or_else(|| Arc::new(Document::new(String::new(), &self.db)))
    }
    fn lookup(&self, name: &str) -> Option<String> {
        crate::car_cdr::hover(name)
            .or_else(|| self.hover_text.get(name).cloned())
            .or_else(|| {
                KEYWORDS
                    .contains(&name)
                    .then(|| format!("**{name}** — SKILL syntax keyword"))
            })
    }
    fn position(&self, doc: &Document, position: Position) -> Position {
        if self.utf32.load(Ordering::Relaxed) {
            position
        } else {
            doc.from_utf16(position)
        }
    }
    fn range(&self, doc: &Document, range: Range) -> Range {
        if self.utf32.load(Ordering::Relaxed) {
            range
        } else {
            Range::new(doc.to_utf16(range.start), doc.to_utf16(range.end))
        }
    }
    async fn update(&self, uri: Url, text: String) {
        let doc = Arc::new(Document::new(text, &self.db));
        self.documents
            .write()
            .unwrap()
            .insert(uri.clone(), doc.clone());
        self.client
            .publish_diagnostics(
                uri,
                doc.diagnostics
                    .iter()
                    .map(|diagnostic| {
                        let mut diagnostic = diagnostic.clone();
                        diagnostic.range = self.range(&doc, diagnostic.range);
                        diagnostic
                    })
                    .collect(),
                None,
            )
            .await;
    }
}
#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let utf32 = params
            .capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_ref())
            .is_some_and(|encodings| encodings.contains(&PositionEncodingKind::UTF32));
        self.utf32.store(utf32, Ordering::Relaxed);
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "skill-lsp".into(),
                version: Some("v3.0.0".into()),
            }),
            capabilities: ServerCapabilities {
                position_encoding: Some(if utf32 {
                    PositionEncodingKind::UTF32
                } else {
                    PositionEncodingKind::UTF16
                }),
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        ..Default::default()
                    },
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["(".into(), " ".into()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".into(), ",".into()]),
                    ..Default::default()
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: TOKEN_NAMES
                                    .iter()
                                    .map(|&s| SemanticTokenType::new(s))
                                    .collect(),
                                token_modifiers: vec![],
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
        })
    }
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
    async fn did_open(&self, p: DidOpenTextDocumentParams) {
        self.update(p.text_document.uri, p.text_document.text).await;
    }
    async fn did_change(&self, p: DidChangeTextDocumentParams) {
        if let Some(change) = p.content_changes.into_iter().last() {
            self.update(p.text_document.uri, change.text).await;
        }
    }
    async fn did_close(&self, p: DidCloseTextDocumentParams) {
        self.documents.write().unwrap().remove(&p.text_document.uri);
    }
    async fn completion(&self, p: CompletionParams) -> Result<Option<CompletionResponse>> {
        let pos = p.text_document_position;
        let d = self.document(&pos.text_document.uri);
        let prefix = d.prefix(self.position(&d, pos.position)).to_lowercase();
        let start = self
            .completion_index
            .partition_point(|(name, _)| name.as_str() < prefix.as_str());
        let mut indices: Vec<_> = self.completion_index[start..]
            .iter()
            .take_while(|(name, _)| name.starts_with(&prefix))
            .map(|(_, i)| *i)
            .collect();
        indices.sort_unstable();
        let mut items: Vec<_> = indices
            .into_iter()
            .map(|i| self.completions[i].clone())
            .collect();
        let mut seen = std::collections::HashSet::new();
        for s in &d.symbols {
            if seen.insert(&s.name) && s.name.to_lowercase().starts_with(&prefix) {
                items.push(CompletionItem {
                    label: s.name.clone(),
                    kind: Some(if s.function {
                        CompletionItemKind::FUNCTION
                    } else {
                        CompletionItemKind::VARIABLE
                    }),
                    detail: Some("user defined".into()),
                    ..Default::default()
                });
            }
        }
        let incomplete = items.len() > self.max_completions;
        items.truncate(self.max_completions);
        Ok(Some(CompletionResponse::List(CompletionList {
            is_incomplete: incomplete,
            items,
        })))
    }
    async fn hover(&self, p: HoverParams) -> Result<Option<Hover>> {
        let pos = p.text_document_position_params;
        let d = self.document(&pos.text_document.uri);
        let name = d.word(self.position(&d, pos.position));
        let value = self.lookup(&name).or_else(|| {
            d.symbol(&name)
                .map(|_| format!("**{name}** — user-defined symbol"))
        });
        Ok(value.map(|value| Hover {
            contents: HoverContents::Markup(markup(value)),
            range: None,
        }))
    }
    async fn goto_definition(
        &self,
        p: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let pos = p.text_document_position_params;
        let d = self.document(&pos.text_document.uri);
        let position = self.position(&d, pos.position);
        if crate::car_cdr::is_accessor(&d.word(position)) {
            return Ok(None);
        }
        Ok(d.symbol(&d.word(position)).map(|s| {
            GotoDefinitionResponse::Scalar(Location {
                uri: pos.text_document.uri,
                range: self.range(&d, s.range),
            })
        }))
    }
    async fn references(&self, p: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let pos = p.text_document_position;
        let d = self.document(&pos.text_document.uri);
        let name = d.word(self.position(&d, pos.position));
        Ok(Some(
            d.references(&name)
                .into_iter()
                .map(|range| Location {
                    uri: pos.text_document.uri.clone(),
                    range: self.range(&d, range),
                })
                .collect(),
        ))
    }
    async fn document_symbol(
        &self,
        p: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let d = self.document(&p.text_document.uri);
        Ok(Some(DocumentSymbolResponse::Nested(
            d.document_symbols()
                .into_iter()
                .map(|mut symbol| {
                    symbol.range = self.range(&d, symbol.range);
                    symbol.selection_range = self.range(&d, symbol.selection_range);
                    symbol
                })
                .collect(),
        )))
    }
    async fn semantic_tokens_full(
        &self,
        p: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let d = self.document(&p.text_document.uri);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: if self.utf32.load(Ordering::Relaxed) {
                d.semantic.clone()
            } else {
                d.semantic_utf16.clone()
            },
        })))
    }
    async fn formatting(&self, p: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        Ok(Some(self.document(&p.text_document.uri).format(&p.options)))
    }
    async fn signature_help(&self, p: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let pos = p.text_document_position_params;
        let d = self.document(&pos.text_document.uri);
        let Some((name, active)) = d.call(self.position(&d, pos.position)) else {
            return Ok(None);
        };
        let (signature, active) = if let Some(e) = self.db.get(&name) {
            (
                SignatureInformation {
                    label: e.signature_label(),
                    documentation: Some(Documentation::MarkupContent(markup(e.markdown()))),
                    parameters: Some(
                        e.params
                            .iter()
                            .map(|p| ParameterInformation {
                                label: ParameterLabel::Simple(p.name.clone()),
                                documentation: Some(Documentation::MarkupContent(markup(format!(
                                    "`{}` — {}{}",
                                    p.name,
                                    p.type_str,
                                    if p.optional { " *(optional)*" } else { "" }
                                )))),
                            })
                            .collect(),
                    ),
                    active_parameter: None,
                },
                active.min(e.params.len().saturating_sub(1)),
            )
        } else {
            (
                SignatureInformation {
                    label: name.clone(),
                    documentation: Some(Documentation::MarkupContent(markup(
                        self.lookup(&name)
                            .unwrap_or_else(|| format!("**{name}** — SKILL function")),
                    ))),
                    parameters: None,
                    active_parameter: None,
                },
                0,
            )
        };
        Ok(Some(SignatureHelp {
            signatures: vec![signature],
            active_signature: Some(0),
            active_parameter: Some(active as u32),
        }))
    }
}

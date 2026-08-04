// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub mod code_action;
pub mod code_lens;
mod compilation;
pub mod completion;
pub mod definition;
pub mod format;
pub mod hover;
mod name_locator;
mod openqasm;
pub mod protocol;
mod qsc_utils;
pub mod references;
pub mod rename;
pub mod signature_help;
mod state;
#[cfg(test)]
mod test_utils;
#[cfg(test)]
mod tests;

use compilation::Compilation;
use futures::channel::mpsc::{TryRecvError, UnboundedReceiver, UnboundedSender, unbounded};
use futures::channel::oneshot;
use futures_util::StreamExt;
use log::{trace, warn};
use protocol::{
    CodeAction, CodeLens, CompletionList, DiagnosticUpdate, Hover, NotebookMetadata, SignatureHelp,
    TestCallables, TextEdit, WorkspaceConfigurationUpdate,
};
use qsc::{
    line_column::{Encoding, Position, Range},
    location::Location,
};
use qsc_project::JSProjectHost;
use state::{CompilationState, CompilationStateUpdater};
use std::{
    cell::{Cell, RefCell},
    fmt::Debug,
    rc::Rc,
};

pub struct LanguageService {
    /// All [`Position`]s and [`Range`]s will be mapped using this encoding.
    /// In LSP the equivalent would be the `positionEncoding` server capability.
    position_encoding: Encoding,
    /// The compilation state. This state is protected by a `RefCell` so that
    /// read and update operations can share it. Update operations should take
    /// care never leave `CompilationState` in an inconsistent state during an
    /// `await` point, as readers may have access to it.
    state: Rc<RefCell<CompilationState>>,
    /// Channel for compilation state update messages coming from the client.
    state_updater: Option<UnboundedSender<Update>>,
    /// Callers parked in [`LanguageService::wait_for_document_version`].
    version_waiters: Rc<VersionWaiters>,
}

/// The outcome of waiting for a specific version of a document to be compiled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionWaitResult {
    /// The compilation state reflects exactly the requested version.
    Ready,
    /// The document has already moved past the requested version. That version was
    /// coalesced away and will never be compiled, so it can no longer be answered for.
    Superseded,
    /// Indicates that the language service has been shutdown and the awaited version
    /// will never be compiled.
    Never,
}

/// Callers parked in [`LanguageService::wait_for_document_version`], waiting for the
/// next batch of updates to be applied so they can re-inspect the compilation state.
#[derive(Default)]
struct VersionWaiters {
    parked: RefCell<Vec<oneshot::Sender<()>>>,
    is_shut_down: Cell<bool>,
}

impl VersionWaiters {
    /// Returns `None` once the update handler has stopped, since no further version
    /// will ever be compiled and nothing would arrive to wake the caller.
    fn park(&self) -> Option<oneshot::Receiver<()>> {
        if self.is_shut_down.get() {
            return None;
        }
        let (send, recv) = oneshot::channel();
        let mut parked = self.parked.borrow_mut();
        // Backstop for callers that gave up; `wake_all` collects the rest on the next update.
        parked.retain(|sender| !sender.is_canceled());
        parked.push(send);
        Some(recv)
    }

    fn wake_all(&self) {
        // Taken rather than drained in place: a woken caller may immediately re-park,
        // and that would re-enter the borrow if it were still held here.
        let parked = std::mem::take(&mut *self.parked.borrow_mut());
        for waiter in parked {
            let _ = waiter.send(());
        }
    }

    /// Releases everyone currently parked and rejects any that arrive later. Dropping
    /// the senders is what lets parked callers observe the shutdown.
    fn shut_down(&self) {
        self.is_shut_down.set(true);
        let parked = std::mem::take(&mut *self.parked.borrow_mut());
        drop(parked);
    }
}

impl LanguageService {
    #[must_use]
    pub fn new(position_encoding: Encoding) -> Self {
        Self {
            position_encoding,
            state: Rc::default(),
            state_updater: Option::default(),
            version_waiters: Rc::default(),
        }
    }

    /// Creates an `UpdateHandler`. An update handler will read messages posted
    /// to the update channel and apply them, sequentially, to the compilation state.
    ///
    /// This method *must* be called for the language service to do any work.
    /// The caller needs to start the handler by calling `.run()` with a yield function.
    pub fn create_update_handler<'a>(
        &mut self,
        diagnostics_receiver: impl Fn(DiagnosticUpdate) + 'a,
        // Callback which receives detected test callables and does something with them
        // in the case of VS Code, updates the test explorer with them
        test_callable_receiver: impl Fn(TestCallables) + 'a,
        project_host: impl JSProjectHost + 'static,
    ) -> UpdateHandler<'a> {
        assert!(self.state_updater.is_none());
        let (send, recv) = unbounded();
        let handler = UpdateHandler {
            updater: CompilationStateUpdater::new(
                self.state.clone(),
                diagnostics_receiver,
                test_callable_receiver,
                project_host,
                self.position_encoding,
            ),
            recv,
            version_waiters: self.version_waiters.clone(),
        };
        self.state_updater = Some(send);
        handler
    }

    /// Stops the language service from processing further updates.
    /// This will stop the update handler, and any update operations
    /// that the language service receives after this call will be ignored.
    pub fn stop_updates(&mut self) {
        // Dropping the sender will cause the
        // handler created in [`create_update_handler()`] to stop.
        self.state_updater = None;
    }

    /// Updates the workspace configuration. If any compiler settings are updated,
    /// a recompilation may be triggered, which will result in a new set of diagnostics
    /// being published.
    ///
    /// LSP: workspace/didChangeConfiguration
    pub fn update_configuration(&mut self, configuration: WorkspaceConfigurationUpdate) {
        trace!("update_configuration: {configuration:?}");
        self.send_update(Update::Configuration {
            changed: configuration,
        });
    }

    /// Indicates that the document has been opened or the source has been updated.
    /// This should be called before any language service requests have been made
    /// for the document, typically when the document is first opened in the editor.
    /// It should also be called whenever the source code is updated.
    ///
    /// This is the "entry point" for the language service's logic, after its constructor.
    ///
    /// LSP: textDocument/didOpen, textDocument/didChange
    pub fn update_document(&mut self, uri: &str, version: u32, text: &str, language_id: &str) {
        trace!("update_document: {uri} {version}");
        self.send_update(Update::Document {
            uri: uri.into(),
            version,
            text: text.into(),
            language_id: language_id.into(),
        });
    }

    /// Indicates that the client is no longer interested in the document,
    /// typically occurs when the document is closed in the editor.
    ///
    /// LSP: textDocument/didClose
    pub fn close_document(&mut self, uri: &str, language_id: &str) {
        trace!("close_document: {uri}");
        self.send_update(Update::CloseDocument {
            uri: uri.into(),
            language_id: language_id.into(),
        });
    }

    /// The uri refers to the notebook itself, not any of the individual cells.
    ///
    /// This function expects all Q# content in the notebook every time
    /// it is called, not just the changed cells.
    ///
    /// At this layer we expect the client to have stripped
    /// off all non-Q# content, including Python cells and lines
    /// containing the "%%qsharp" cell magic.
    ///
    /// LSP: notebookDocument/didOpen, notebookDocument/didChange
    pub fn update_notebook_document<'b, I>(
        &mut self,
        notebook_uri: &str,
        notebook_metadata: NotebookMetadata,
        cells: I,
    ) where
        I: Iterator<Item = (&'b str, u32, &'b str)>, // uri, version, text - basically DidChangeTextDocumentParams in LSP
    {
        trace!("update_notebook_document: {notebook_uri}");
        self.send_update(Update::NotebookDocument {
            notebook_uri: notebook_uri.into(),
            notebook_metadata,
            cells: cells
                .map(|(uri, version, contents)| (uri.into(), version, contents.into()))
                .collect(),
        });
    }

    /// Indicates that the client is no longer interested in the notebook.
    ///
    /// # Panics
    ///
    /// Panics if `cell_uris` does not contain all the cells associated with
    /// the notebook in the previous `update_notebook_document` call.
    ///
    /// LSP: notebookDocument/didClose
    pub fn close_notebook_document(&mut self, notebook_uri: &str) {
        trace!("close_notebook_document: {notebook_uri}");
        self.send_update(Update::CloseNotebookDocument {
            notebook_uri: notebook_uri.into(),
        });
    }

    /// Waits until the compilation state reflects exactly `version` of `uri`, reporting
    /// [`VersionWaitResult::Superseded`] if the document moves past it first, or
    /// [`VersionWaitResult::Never`] if the language service stops updating before the
    /// version can arrive. Whether a superseded version is still usable is left to the caller.
    ///
    /// The returned future is independent of `&self` so that callers can hold it across
    /// await points without keeping the language service borrowed. The `use<>` bound is
    /// precise capturing, which is what keeps the input lifetimes out of the future.
    pub fn wait_for_document_version(
        &self,
        uri: &str,
        version: u32,
    ) -> impl std::future::Future<Output = VersionWaitResult> + 'static + use<> {
        let state = self.state.clone();
        let waiters = self.version_waiters.clone();
        let uri = uri.to_string();

        async move {
            loop {
                // Scoped so the borrow is released before awaiting. Holding it across an
                // await would break the updater, which needs mutable access.
                let receiver = {
                    let state = state.borrow();
                    match state.get_open_document_version(&uri) {
                        Some(current) if current == version => return VersionWaitResult::Ready,
                        Some(current) if current > version => return VersionWaitResult::Superseded,
                        // Either behind, or the document hasn't been processed at all yet.
                        _ => match waiters.park() {
                            Some(receiver) => receiver,
                            // The update handler is gone, so the version will never arrive.
                            None => return VersionWaitResult::Never,
                        },
                    }
                };

                if receiver.await.is_err() {
                    // The update handler shut down while we were parked.
                    return VersionWaitResult::Never;
                }
            }
        }
    }

    #[must_use]
    pub fn get_code_actions(&self, uri: &str, range: Range) -> Vec<CodeAction> {
        self.document_op(
            code_action::get_code_actions,
            "get_code_actions",
            uri,
            range,
        )
    }

    /// LSP: textDocument/completion
    #[must_use]
    pub fn get_completions(&self, uri: &str, position: Position) -> CompletionList {
        self.document_op(
            completion::get_completions,
            "get_completions",
            uri,
            position,
        )
    }

    /// LSP: textDocument/definition
    #[must_use]
    pub fn get_definition(&self, uri: &str, position: Position) -> Option<Location> {
        self.document_op(definition::get_definition, "get_definition", uri, position)
    }

    /// LSP: textDocument/references
    #[must_use]
    pub fn get_references(
        &self,
        uri: &str,
        position: Position,
        include_declaration: bool,
    ) -> Vec<Location> {
        self.document_op(
            |compilation, uri, position, position_encoding| {
                references::get_references(
                    compilation,
                    uri,
                    position,
                    position_encoding,
                    include_declaration,
                )
            },
            "get_references",
            uri,
            position,
        )
    }

    /// LSP: textDocument/format
    #[must_use]
    pub fn get_format_changes(&self, uri: &str) -> Vec<TextEdit> {
        self.document_op(
            |compilation, uri, (), position_encoding| {
                format::get_format_changes(compilation, uri, position_encoding)
            },
            "get_format_changes",
            uri,
            (),
        )
    }

    /// LSP: textDocument/hover
    #[must_use]
    pub fn get_hover(&self, uri: &str, position: Position) -> Option<Hover> {
        self.document_op(hover::get_hover, "get_hover", uri, position)
    }

    /// LSP textDocument/signatureHelp
    #[must_use]
    pub fn get_signature_help(&self, uri: &str, position: Position) -> Option<SignatureHelp> {
        self.document_op(
            signature_help::get_signature_help,
            "get_signature_help",
            uri,
            position,
        )
    }

    /// LSP: textDocument/rename
    #[must_use]
    pub fn get_rename(&self, uri: &str, position: Position) -> Vec<Location> {
        self.document_op(rename::get_rename, "get_rename", uri, position)
    }

    /// LSP: textDocument/prepareRename
    #[must_use]
    pub fn prepare_rename(&self, uri: &str, position: Position) -> Option<(Range, String)> {
        self.document_op(rename::prepare_rename, "prepare_rename", uri, position)
    }

    /// LSP: textDocument/codeLens
    #[must_use]
    pub fn get_code_lenses(&self, uri: &str) -> Vec<CodeLens> {
        self.document_op(
            |compilation, uri, (), position_encoding| {
                code_lens::get_code_lenses(compilation, uri, position_encoding)
            },
            "get_code_lenses",
            uri,
            (),
        )
    }

    /// Executes an operation that takes a document uri, using the current compilation for that document.
    /// All "read" operations should go through this method. This method will borrow the current
    /// compilation state to perform the request.
    ///
    /// If there are outstanding updates to the compilation in the update message queue,
    /// this method will still just return the current compilation state.
    fn document_op<F, T, A>(&self, op: F, op_name: &str, uri: &str, arg: A) -> T
    where
        F: Fn(&Compilation, &str, A, Encoding) -> T,
        T: Debug + Default,
        A: Debug,
    {
        trace!("{op_name}: uri: {uri}, arg: {arg:?}");

        // Borrow must succeed here. If it doesn't succeed, a writer
        // (i.e. [`state::CompilationStateUpdater`]) must be holding a mutable reference across
        // an `await` point. Which it shouldn't be doing.
        let compilation_state = self.state.borrow();
        if let Some(compilation) = compilation_state.get_compilation(uri) {
            let res = op(compilation, uri, arg, self.position_encoding);
            trace!("{op_name} result: {res:?}");
            res
        } else {
            // The current state doesn't yet contain the document. Updates must be pending.
            trace!("Skipping {op_name} for {uri} since compilation is in progress");
            T::default()
        }
    }

    /// Queues an update to the compilation state. The message will be handled, and the
    /// actual compilation state update, by the update handler which was created in `create_update_handler()`.
    ///
    /// All "update" operations should go through this method.
    fn send_update(&mut self, update: Update) {
        if let Some(updater) = self.state_updater.as_mut() {
            updater
                .unbounded_send(update)
                .expect("send error in queue_update");
        } else {
            warn!("Ignoring update, no handler is listening");
        }
    }
}

pub struct UpdateHandler<'a> {
    updater: CompilationStateUpdater<'a>,
    recv: UnboundedReceiver<Update>,
    version_waiters: Rc<VersionWaiters>,
}

/// Caps how many times [`UpdateHandler::run`] will give the host a turn to deliver more
/// input before processing what it has. Only bounds the worst case: the loop normally
/// exits earlier, as soon as a yield produces nothing new.
const MAX_YIELDS_PER_BATCH: usize = 4;

impl UpdateHandler<'_> {
    /// Runs the update handler. This method is expected to run
    /// for the entire lifetime of the language service.
    ///
    /// It returns a future that will only complete when the
    /// language service has explicitly closed the message
    /// channel, in `stop_update_handler()`.
    ///
    /// `yield_to_host` gives the host event loop a turn. Applying an update blocks that
    /// loop, so input events that arrive while one is in flight aren't delivered until
    /// we yield. Without yielding, the channel looks empty and each event ends up being
    /// processed on its own instead of being coalesced with the others.
    pub async fn run<F, Fut>(&mut self, yield_to_host: F)
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        while let Some(update) = self.recv.next().await {
            let mut updates = vec![update];

            // Keep giving the host a turn for as long as it has more to deliver. When the
            // user pauses, the first yield comes back with nothing and this exits after a
            // single tick. While they're typing, the backlog arrives over a few passes.
            for _ in 0..MAX_YIELDS_PER_BATCH {
                yield_to_host().await;

                let batch_before = updates.len();
                let drained = self.drain_pending(&mut updates);
                if drained == 0 {
                    break;
                }

                // Every drained update either claims a new slot in the batch or merges
                // into an existing one, and each merge discards one update.
                let dropped = drained - (updates.len() - batch_before);
                trace!("drained {drained} update(s), merging dropped {dropped} as redundant");
            }

            self.apply(updates).await;
        }

        // Not just left to `drop`, so waiters are released as soon as the loop ends even
        // if the handler itself is kept alive.
        self.version_waiters.shut_down();
    }

    /// Convenience method to apply *only* the pending updates
    /// in the message queue. Used for testing, when it's desirable
    /// to control exactly when updates are applied.
    ///
    /// Since `run()` will mutably borrow `self` for the entire
    /// lifetime of the handler, this method should not ever be used
    /// if `run()` has been called.
    #[cfg(test)]
    async fn apply_pending(&mut self) {
        let mut updates = Vec::new();
        self.drain_pending(&mut updates);
        self.apply(updates).await;
    }

    /// Drains everything currently queued into `updates`, returning the number of
    /// messages received. A closed channel reports zero, since `Closed` means empty
    /// *and* closed: messages already buffered are still handed over first.
    ///
    /// The count is of messages received rather than the length of `updates`, because
    /// [`push_update`] merges redundant updates in place and so may not grow it.
    fn drain_pending(&mut self, updates: &mut Vec<Update>) -> usize {
        let mut received = 0;
        loop {
            match self.recv.try_recv() {
                Ok(update) => {
                    push_update(updates, update);
                    received += 1;
                }
                Err(TryRecvError::Empty | TryRecvError::Closed) => return received,
            }
        }
    }

    async fn apply(&mut self, mut updates: Vec<Update>) {
        trace!("applying {} updates", updates.len());
        if updates.len() > 100 {
            // This indicates that we're not keeping up with incoming updates.
            // Harmless, but an indicator that we could try intelligently
            // dropping updates or otherwise optimizing.
            warn!(
                "perf: {} pending updates found even after deduping",
                updates.len()
            );
        }

        for update in updates.drain(..) {
            apply_update(&mut self.updater, update).await;
        }
        trace!("end applying updates");

        // Let anyone waiting on a particular version re-check where the state landed.
        self.version_waiters.wake_all();
    }
}

impl Drop for UpdateHandler<'_> {
    /// Covers every way the handler can go away, including `run` never being called or
    /// its future being cancelled, not just the loop exiting normally.
    fn drop(&mut self) {
        self.version_waiters.shut_down();
    }
}

fn push_update(pending_updates: &mut Vec<Update>, update: Update) {
    // Dedup consecutive updates to the same document.
    match &update {
        Update::Document { uri, .. } => {
            if let Some(last) = pending_updates.last_mut()
                && let Update::Document { uri: last_uri, .. } = last
                && last_uri == uri
            {
                // overwrite the last element
                *last = update;
                return;
            }
        }
        Update::NotebookDocument { notebook_uri, .. } => {
            if let Some(last) = pending_updates.last_mut()
                && let Update::NotebookDocument {
                    notebook_uri: last_uri,
                    ..
                } = last
                && last_uri == notebook_uri
            {
                // overwrite the last element
                *last = update;
                return;
            }
        }
        Update::Configuration { .. }
        | Update::CloseDocument { .. }
        | Update::CloseNotebookDocument { .. } => (), // These events aren't noisy enough to bother deduping.
    }
    pending_updates.push(update);
}

async fn apply_update(updater: &mut CompilationStateUpdater<'_>, update: Update) {
    match update {
        Update::CloseDocument { uri, language_id } => {
            updater.close_document(&uri, &language_id).await;
        }
        Update::Document {
            uri,
            version,
            text,
            language_id,
        } => {
            updater
                .update_document(&uri, version, &text, &language_id)
                .await;
        }
        Update::NotebookDocument {
            notebook_uri,
            notebook_metadata,
            cells,
        } => {
            updater
                .update_notebook_document(
                    &notebook_uri,
                    &notebook_metadata,
                    cells.iter().map(|(uri, version, contents)| {
                        (uri.as_ref(), *version, contents.as_ref())
                    }),
                )
                .await;
        }
        Update::CloseNotebookDocument { notebook_uri } => {
            updater.close_notebook_document(&notebook_uri);
        }
        Update::Configuration { changed } => {
            updater.update_configuration(changed);
        }
    }
}

enum Update {
    Configuration {
        changed: WorkspaceConfigurationUpdate,
    },
    Document {
        uri: String,
        version: u32,
        text: String,
        language_id: String,
    },
    CloseDocument {
        uri: String,
        language_id: String,
    },
    NotebookDocument {
        notebook_uri: String,
        notebook_metadata: NotebookMetadata,
        cells: Vec<(String, u32, String)>,
    },
    CloseNotebookDocument {
        notebook_uri: String,
    },
}

impl Update {
    #[cfg(test)]
    fn summary(&self) -> String {
        match self {
            Update::Configuration { .. } => "Configuration".to_string(),
            Update::Document { uri, version, .. } => format!("Document({uri}, {version})"),
            Update::CloseDocument { uri, .. } => format!("CloseDocument({uri})"),
            Update::NotebookDocument { notebook_uri, .. } => {
                format!("NotebookDocument({notebook_uri})")
            }
            Update::CloseNotebookDocument { notebook_uri } => {
                format!("CloseNotebookDocument({notebook_uri})")
            }
        }
    }
}

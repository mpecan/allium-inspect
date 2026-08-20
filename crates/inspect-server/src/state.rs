//! What the server holds, and how it is replaced.
//!
//! One [`Inspection`] is the whole answer to every request: the graph, and the
//! text of each spec file behind it. It is immutable — a reload builds a new one
//! and swaps it in — so a request that arrives mid-reload sees a consistent
//! picture rather than a graph from after the edit and source from before it.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use inspect_model::{
    AlliumRunner, FileReader, IngestError, SourceReader, SpecGraph, ingest, module_name,
};
use serde::Serialize;

/// One spec file's text, as the source panel needs it.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleSource {
    pub module: String,
    pub path: String,
    pub text: String,
}

/// A graph and the sources it was built from.
#[derive(Debug)]
pub struct Inspection {
    pub graph: SpecGraph,
    sources: BTreeMap<String, ModuleSource>,
}

impl Inspection {
    /// Run the CLI over `paths` and read their text.
    ///
    /// # Errors
    ///
    /// Returns [`IngestError`] when the CLI cannot be run or a spec cannot be
    /// read.
    pub fn build<R: AlliumRunner>(runner: &R, paths: &[PathBuf]) -> Result<Self, IngestError> {
        let graph = ingest(runner, &FileReader, paths)?;
        let mut sources = BTreeMap::new();
        for path in paths {
            let module = module_name(path);
            // A file that ingested but cannot be re-read is odd rather than
            // fatal: the graph is still worth serving, and the source panel
            // reports the gap where it would have shown the text.
            if let Ok(text) = FileReader.read(path) {
                sources.insert(
                    module.clone(),
                    ModuleSource { module, path: path.to_string_lossy().into_owned(), text },
                );
            }
        }
        Ok(Self { graph, sources })
    }

    /// An inspection assembled from parts, without running anything.
    ///
    /// The seam the route tests use: they are about which URL returns which
    /// document, and building a real graph would make every one of them depend
    /// on the CLI being installed.
    #[must_use]
    pub fn from_parts(graph: SpecGraph, sources: Vec<ModuleSource>) -> Self {
        Self {
            graph,
            sources: sources.into_iter().map(|source| (source.module.clone(), source)).collect(),
        }
    }

    /// One module's source.
    #[must_use]
    pub fn source(&self, module: &str) -> Option<&ModuleSource> {
        self.sources.get(module)
    }

    /// Every module that has source, in name order.
    pub fn modules(&self) -> impl Iterator<Item = &str> {
        self.sources.keys().map(String::as_str)
    }
}

/// The current inspection, swappable while requests are in flight.
///
/// A read lock is held only long enough to clone the `Arc`, so a reload never
/// blocks a request for longer than a pointer copy — and a request that took the
/// pointer before a reload keeps serving the version it started with.
#[derive(Debug, Clone)]
pub struct AppState {
    current: Arc<RwLock<Arc<Inspection>>>,
    /// What the last reload failed with, if it did. Kept so the UI can say the
    /// spec is broken rather than silently showing the version before the edit.
    error: Arc<RwLock<Option<String>>>,
}

impl AppState {
    /// Hold `inspection` as the current one.
    #[must_use]
    pub fn new(inspection: Inspection) -> Self {
        Self {
            current: Arc::new(RwLock::new(Arc::new(inspection))),
            error: Arc::new(RwLock::new(None)),
        }
    }

    /// The current inspection.
    #[must_use]
    pub fn get(&self) -> Arc<Inspection> {
        // A poisoned lock means a previous holder panicked while swapping. The
        // data behind it is a whole `Arc<Inspection>` that was either replaced
        // or not, so it is still consistent, and refusing to serve would turn
        // one panic into a dead server.
        match self.current.read() {
            Ok(guard) => Arc::clone(&guard),
            Err(poisoned) => Arc::clone(&poisoned.into_inner()),
        }
    }

    /// Replace the current inspection.
    pub fn replace(&self, inspection: Inspection) {
        if let Ok(mut guard) = self.current.write() {
            *guard = Arc::new(inspection);
        }
        self.set_error(None);
    }

    /// Record why the last reload failed, or that it did not.
    pub fn set_error(&self, message: Option<String>) {
        if let Ok(mut guard) = self.error.write() {
            *guard = message;
        }
    }

    /// Why the last reload failed, if it did.
    #[must_use]
    pub fn error(&self) -> Option<String> {
        self.error.read().ok().and_then(|guard| guard.clone())
    }
}

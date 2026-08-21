//! What the server holds, and how it is replaced.
//!
//! One [`Inspection`] is the whole answer to every request: the graph, and the
//! text of each spec file behind it. It is immutable — a reload builds a new one
//! and swaps it in — so a request that arrives mid-reload sees a consistent
//! picture rather than a graph from after the edit and source from before it.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use inspect_model::{
    AlliumRunner, FileReader, IngestError, Ingestion, Program, SourceReader, SpecGraph, ingest,
    module_name,
};
use inspect_sim::step::Sources;
use serde::Serialize;

use crate::journeys::JourneyReport;

/// One spec file's text, as the source panel needs it.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleSource {
    pub module: String,
    pub path: String,
    pub text: String,
}

/// A graph, the expression trees behind it, and the sources both came from.
#[derive(Debug)]
pub struct Inspection {
    pub graph: SpecGraph,
    /// The clause ASTs. Never sent to the browser — the simulator runs here.
    pub program: Program,
    sources: BTreeMap<String, ModuleSource>,
    texts: Sources,
    /// Every authored journey, walked against this graph.
    ///
    /// Part of the inspection rather than held beside it, because the answer a
    /// journey gives is a fact about a particular version of the spec. Edit the
    /// spec and the verdicts have to move with it; a report that outlived the
    /// graph it was computed from would be the most confident kind of wrong.
    journeys: JourneyReport,
}

impl Inspection {
    /// Run the CLI over `paths` and read their text.
    ///
    /// # Errors
    ///
    /// Returns [`IngestError`] when the CLI cannot be run or a spec cannot be
    /// read.
    pub fn build<R: AlliumRunner>(
        runner: &R,
        paths: &[PathBuf],
        journeys: &[PathBuf],
    ) -> Result<Self, IngestError> {
        let Ingestion { graph, program } = ingest(runner, &FileReader, paths)?;
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
        let texts: Sources =
            sources.iter().map(|(module, source)| (module.clone(), source.text.clone())).collect();
        let journeys = JourneyReport::build(journeys, &graph, &program, &texts);
        Ok(Self { graph, program, sources, texts, journeys })
    }

    /// An inspection assembled from parts, without running anything.
    ///
    /// The seam the route tests use: they are about which URL returns which
    /// document, and building a real graph would make every one of them depend
    /// on the CLI being installed.
    #[must_use]
    pub fn from_parts(graph: SpecGraph, sources: Vec<ModuleSource>) -> Self {
        let sources: BTreeMap<String, ModuleSource> =
            sources.into_iter().map(|source| (source.module.clone(), source)).collect();
        let texts =
            sources.iter().map(|(module, source)| (module.clone(), source.text.clone())).collect();
        Self { graph, program: Program::new(), sources, texts, journeys: JourneyReport::empty() }
    }

    /// Give an assembled inspection a journey report, for the route tests.
    #[must_use]
    pub fn with_journeys(mut self, journeys: JourneyReport) -> Self {
        self.journeys = journeys;
        self
    }

    /// Every authored journey, walked.
    #[must_use]
    pub fn journeys(&self) -> &JourneyReport {
        &self.journeys
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

    /// The spec text of each module, as the simulator wants it.
    ///
    /// Built once and held, rather than assembled per request: a step quotes
    /// source for every undecided sub-expression it meets, and rebuilding the
    /// map each time would copy every spec file on every keystroke.
    #[must_use]
    pub fn sources_by_module(&self) -> &Sources {
        &self.texts
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
    /// How many times the spec set has been re-read since the server started.
    ///
    /// The browser has no other way to find out. It holds a graph it fetched
    /// once, and a watcher that reloads the server's copy without telling
    /// anyone leaves a reader studying a picture of a file that no longer says
    /// that — which is the one failure this tool cannot afford.
    ///
    /// A counter rather than a timestamp: it is compared, never displayed, and
    /// the two pure crates are kept clear of clocks for the same reason
    /// everything else here is.
    revision: Arc<AtomicU64>,
}

impl AppState {
    /// Hold `inspection` as the current one.
    #[must_use]
    pub fn new(inspection: Inspection) -> Self {
        Self {
            current: Arc::new(RwLock::new(Arc::new(inspection))),
            error: Arc::new(RwLock::new(None)),
            revision: Arc::new(AtomicU64::new(0)),
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
        self.revision.fetch_add(1, Ordering::Relaxed);
    }

    /// Record why the last reload failed, or that it did not.
    ///
    /// A failure counts as a revision too. The graph did not change, but what
    /// the reader needs to be told about it did, and they find that out by
    /// noticing the number move.
    pub fn set_error(&self, message: Option<String>) {
        let changed = self.error() != message;
        if let Ok(mut guard) = self.error.write() {
            *guard = message;
        }
        if changed {
            self.revision.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Why the last reload failed, if it did.
    #[must_use]
    pub fn error(&self) -> Option<String> {
        self.error.read().ok().and_then(|guard| guard.clone())
    }

    /// How many times the answer has changed since the server started.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }
}

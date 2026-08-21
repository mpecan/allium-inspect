//! Ingestion of the `allium` CLI's JSON output into a single linked graph.
//!
//! The `allium` CLI answers four different questions about a specification and
//! none of them alone is enough to draw it:
//!
//! | command   | carries                                                    |
//! |-----------|------------------------------------------------------------|
//! | `model`   | entities, fields, relationships, transition graphs, enums  |
//! | `parse`   | the full byte-spanned AST — the only source of rule clauses |
//! | `plan`    | test obligations, including each rule's trigger/entity deps |
//! | `analyse` | reachability, deadlock and conflict findings               |
//!
//! This crate runs all four per spec file, merges them into a `Module`, links
//! modules to each other through their `use` declarations, and projects the
//! result into the view graphs the UI draws.
//!
//! Nothing here touches a clock, a socket or a random number generator, and the
//! CLI itself is reached only through the `AlliumRunner` trait — so the whole
//! crate is testable against recorded fixtures with no `allium` binary present.

#![forbid(unsafe_code)]

pub mod diagnostic;
pub mod graph;
pub mod ingest;
pub mod program;
pub mod runner;
pub mod span;

pub use diagnostic::{Diagnostic, Finding, Location, Severity};
pub use graph::{
    Edge, EdgeKind, Module, Node, NodeDetail, NodeId, NodeKind, Obligation, SpecGraph,
};
pub use ingest::{
    FileReader, IngestError, Ingestion, MemoryReader, SourceReader, ingest, module_name,
};
pub use program::{Program, RuleAst};
pub use runner::{AlliumRunner, Command, MapRunner, ProcessRunner, RunError};
pub use span::{LineIndex, Position, Span};

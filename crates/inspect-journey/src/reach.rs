//! What an act sets off that some listener can never bind (#12).
//!
//! Ingestion already reports, about the specification, every emission that
//! leaves a listener's parameter unbound ([`inspect_model::listening`]), as a
//! diagnostic on the emitting rule. A journey asks a narrower question, and
//! `check` answers it before anything is walked: of those, which does *this
//! act* reach — through the rules its trigger wakes, what they emit, the rules
//! that wakes, and so on. It reads the diagnostics rather than working them
//! out again, so the two say the same words and the spec is read once.
//!
//! A **remark**, not a verdict. Whether the emission is reached in a world is
//! the walk's question — it may sit in a branch no world takes — and a checker
//! that called the step undecided would be answering it without a world. The
//! walk does not stop at a remark, and when the emission is reached it says
//! the same thing again as the listener being undecided.
//!
//! Followed through emissions only. A creation the act makes wakes the rules
//! reacting to it too, and what *those* emit is not followed here: which
//! module's `Order.created` a reaction answers is a question about the
//! instance, which a checker without a world does not have. The ingestion
//! diagnostic still reports such an emission, and the walk still reports the
//! listener undecided when it is reached.

use std::collections::BTreeSet;

use inspect_model::{NodeId, Program, SpecGraph, listening};

use crate::check::{Note, Verdict};

/// A remark on `line` for each listener parameter an emission reachable from
/// `trigger` leaves unbound, in the order ingestion reported them, saying
/// where the emission is.
pub(crate) fn check(
    trigger: &str,
    line: usize,
    graph: &SpecGraph,
    program: &Program,
    notes: &mut Vec<Note>,
) {
    let unbound = graph
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some(listening::CODE));
    let mut unbound = unbound.peekable();
    if unbound.peek().is_none() {
        return;
    }
    let reached = reached(trigger, graph, program);
    for diagnostic in unbound {
        let Some(emitter) = &diagnostic.node else { continue };
        if !reached.iter().any(|rule| rule.as_str() == emitter) {
            continue;
        }
        let message = match &diagnostic.location {
            Some(at) => format!("{} (at {}:{})", diagnostic.message, at.file, at.line),
            None => diagnostic.message.clone(),
        };
        notes.push(Note { line, verdict: Verdict::Remark, message });
    }
}

/// Every rule a firing of `trigger` wakes, and every rule woken by what those
/// emit, however far down — each emission heard only where the walk would
/// hear it. Bounded by the emissions already followed, so a chain that comes
/// back to where it started ends.
fn reached(trigger: &str, graph: &SpecGraph, program: &Program) -> BTreeSet<NodeId> {
    let mut rules = BTreeSet::new();
    // The act first, which nobody emitted; then each trigger with the module
    // that emitted it.
    let mut followed: BTreeSet<(String, Option<String>)> = BTreeSet::new();
    let mut pending = vec![(trigger.to_owned(), None)];
    while let Some((next, from)) = pending.pop() {
        if !followed.insert((next.clone(), from.clone())) {
            continue;
        }
        for rule in listening::listeners(graph, program, &next, from.as_deref()) {
            if !rules.insert(rule.clone()) {
                continue;
            }
            let Some(ast) = program.rule(rule.as_str()) else { continue };
            let module = graph.node(rule).map(|node| node.module.clone()).unwrap_or_default();
            for emission in listening::emissions(&ast.ensures) {
                pending.push((emission.trigger.to_owned(), Some(module.clone())));
            }
        }
    }
    rules
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use inspect_model::{Command, MemoryReader, ingest, runner::MapRunner};
    use serde_json::json;

    use super::*;

    /// `modules`, ingested from their source alone.
    fn ingested_all(modules: &[(&str, &str)]) -> (SpecGraph, Program) {
        let mut runner = MapRunner::new("test");
        let mut reader = MemoryReader::default();
        let mut paths = Vec::new();
        for (module, source) in modules {
            let path = PathBuf::from(format!("{module}.allium"));
            for command in [Command::Model, Command::Plan] {
                runner = runner.with(command, &path, json!({}));
            }
            reader = reader.with(&path, *source);
            paths.push(path);
        }
        let it = ingest(&runner, &reader, &paths).unwrap_or_else(|error| panic!("{error}"));
        (it.graph, it.program)
    }

    /// One module, ingested from its source alone.
    fn ingested(source: &str) -> (SpecGraph, Program) {
        ingested_all(&[("m", source)])
    }

    const SPEC: &str = "-- allium: 3
entity Doc { seen: Boolean }
rule Start {
    when: Start(doc)
    ensures: Middle(doc)
}
rule Middle {
    when: Middle(doc)
    ensures: End(item: doc)
}
rule Finish {
    when: End(doc)
    ensures: doc.seen = true
}
rule Loop {
    when: Loop(doc)
    ensures: Loop(doc)
}
rule Elsewhere {
    when: Elsewhere(doc)
    ensures: End(other: doc)
}
";

    fn remarks(trigger: &str) -> Vec<String> {
        let (graph, program) = ingested(SPEC);
        let mut notes = Vec::new();
        check(trigger, 7, &graph, &program, &mut notes);
        assert!(notes.iter().all(|note| note.line == 7 && note.verdict == Verdict::Remark));
        notes.into_iter().map(|note| note.message).collect()
    }

    /// Two emissions away from the act, and only the emission reached:
    /// `Elsewhere` leaves the same parameter unbound and is not this act's.
    #[test]
    fn an_emission_two_hops_from_the_act_is_remarked_on_and_one_off_the_path_is_not() {
        let said = remarks("Start");
        assert_eq!(said.len(), 1, "{said:?}");
        assert_eq!(
            said[0],
            "`Middle` emits `End(item: doc)` without binding `doc` for `Finish`, which listens \
             for `End(doc)`: it was handed `item` by name, and a named argument binds only the \
             parameter it names (at m.allium:9)"
        );
    }

    /// Followed where the walk would follow it: `b` does not import `a`, so its
    /// `Done` is another trigger of the same name, and an act in `a` reaches
    /// nothing in `b` — while `c`, which listens for `a/Done`, is reached.
    #[test]
    fn an_emission_is_followed_only_into_modules_that_hear_it() {
        let (graph, program) = ingested_all(&[
            (
                "a",
                "-- allium: 3\nentity Doc { seen: Boolean }\n\
                 rule Go {\n    when: Go(doc)\n    ensures: Done(doc)\n}\n",
            ),
            (
                "b",
                "-- allium: 3\nrule Close {\n    when: Done(task, reason)\n    ensures: task.seen = true\n}\n",
            ),
            (
                "c",
                "-- allium: 3\nuse \"./a.allium\" as a\n\
                 rule Hear {\n    when: a/Done(doc)\n    ensures: Heard(note: doc)\n}\n\
                 rule Log {\n    when: Heard(doc)\n    ensures: doc.seen = true\n}\n",
            ),
        ]);
        let reached: Vec<String> =
            reached("Go", &graph, &program).iter().map(ToString::to_string).collect();
        assert_eq!(reached, ["a::rule::Go", "c::rule::Hear", "c::rule::Log"]);
        let mut notes = Vec::new();
        check("Go", 3, &graph, &program, &mut notes);
        let said: Vec<&str> = notes.iter().map(|note| note.message.as_str()).collect();
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].starts_with("`Hear` emits `Heard(note: doc)` without binding `doc` for `Log`")
        );
    }

    #[test]
    fn a_chain_that_comes_back_to_itself_ends() {
        assert!(remarks("Loop").is_empty());
    }

    #[test]
    fn an_act_nothing_listens_for_reaches_nothing() {
        assert!(remarks("Nobody").is_empty());
    }
}

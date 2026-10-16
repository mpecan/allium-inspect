//! An emission a listener can never bind, found before anything runs (#12).
//!
//! A rule that `ensures: Published(item: doc)` wakes every rule waiting on
//! `when: Published(doc)`, and the walk binds each listener's parameters from
//! what the emission carried by [`crate::binding::plan`]. When that leaves one
//! unbound the listener is undecided every time it is woken — and a walk only
//! says so if a journey happens to reach it. This says it up front, about the
//! specification, for every emission and every listener at once.
//!
//! **The same answer the walk gives, by construction.** Which calls are
//! emissions is the simulator's own test ([`emitted`]), found in the places the
//! simulator applies a postcondition ([`emissions`]); which rules an emission
//! wakes is the simulator's own match ([`hears`]), on the trigger's name and
//! the modules the two rules are in; the listener's parameters are read off its `when` by
//! [`crate::RuleAst::parameters`]; and what binds them is
//! [`crate::binding::plan`], which the simulator binds values by. A parameter
//! the listener's own `let` gives a value is bound at run time too, and is not
//! reported.
//!
//! Only the *shape* of an emission decides this — which names it carried and
//! how many arguments by position — so there is nothing here that could come
//! back undecided. Whether the emission is ever reached (it may sit in a branch
//! no world takes) is the walk's question, and this does not claim to answer
//! it: it says that *if* it is reached, this listener is handed nothing for
//! this parameter.

use std::collections::{BTreeMap, BTreeSet};

use allium_parser::ast::{CallArg, Expr};

use crate::{
    Diagnostic, LineIndex, Location, NodeId, NodeKind, Program, Severity, Span, SpecGraph,
    binding::{self, Bound, Carried},
};

/// The code the diagnostic carries. Not `allium.*`: allium did not say it.
pub const CODE: &str = "inspect.trigger.unbound";

/// One trigger emitted from a rule's `ensures`.
#[derive(Debug, Clone)]
pub struct Emission<'a> {
    /// The trigger's name, as written.
    pub trigger: &'a str,
    /// What it passed, named and by position.
    pub args: &'a [CallArg],
    /// The call, `Published(item: doc)`.
    pub span: Span,
}

impl Emission<'_> {
    /// The shape binding reads.
    #[must_use]
    pub fn carried(&self) -> Carried<'_> {
        let mut named = BTreeSet::new();
        let mut positional = 0;
        for argument in self.args {
            match argument {
                CallArg::Named(argument) => {
                    named.insert(argument.name.name.as_str());
                }
                CallArg::Positional(_) => positional += 1,
            }
        }
        Carried { trigger: self.trigger, named, positional }
    }
}

/// The trigger a call in an `ensures` emits, when it emits one: a bare name.
///
/// `Entity.created(…)` is a creation and `memo/Note.created(…)` one across a
/// boundary; anything else that is not a bare name is a call the simulator
/// does not apply. The simulator asks this same function.
#[must_use]
pub fn emitted(function: &Expr) -> Option<&str> {
    match function {
        Expr::Ident(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}

/// Every emission in `ensures`, in the order written: a clause, the items of
/// a block, each branch of an `if`, the body of a `for`, and a `let` whose
/// value is a call — the places the simulator applies a postcondition, and no
/// others. A call inside a value (`x.n = f(y)`) is evaluated, not emitted.
#[must_use]
pub fn emissions(ensures: &[Expr]) -> Vec<Emission<'_>> {
    let mut found = Vec::new();
    for clause in ensures {
        collect(clause, &mut found);
    }
    found
}

fn collect<'a>(clause: &'a Expr, found: &mut Vec<Emission<'a>>) {
    match clause {
        Expr::Call { function, args, span } => {
            if let Some(trigger) = emitted(function) {
                found.push(Emission { trigger, args, span: Span::new(span.start, span.end) });
            }
        }
        Expr::Block { items, .. } => items.iter().for_each(|item| collect(item, found)),
        Expr::Conditional { branches, else_body, .. } => {
            for branch in branches {
                collect(&branch.body, found);
            }
            if let Some(body) = else_body {
                collect(body, found);
            }
        }
        Expr::For { body, .. } => collect(body, found),
        Expr::LetExpr { value, .. } if matches!(value.as_ref(), Expr::Call { .. }) => {
            collect(value, found);
        }
        _ => {}
    }
}

/// A listener parameter one emission leaves unbound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unbindable {
    /// The rule whose `ensures` emits.
    pub emitter: NodeId,
    /// The trigger emitted.
    pub trigger: String,
    /// Where the emission is written, in the emitter's module.
    pub span: Span,
    /// The rule woken by it.
    pub listener: NodeId,
    /// The listener's parameter nothing binds.
    pub parameter: String,
    /// What the emission carried instead, as the walk says it.
    pub why: String,
}

/// Every listener parameter some emission in `graph` leaves unbound, by
/// emitting rule, then emission, then listener, then parameter — the order of
/// the graph, which is sorted, so the same spec always says the same thing.
#[must_use]
pub fn unbindable(graph: &SpecGraph, program: &Program) -> Vec<Unbindable> {
    let mut found = Vec::new();
    for emitter in graph.nodes_of(NodeKind::Rule) {
        let Some(ast) = program.rule(emitter.id.as_str()) else { continue };
        for emission in emissions(&ast.ensures) {
            let carried = emission.carried();
            let from = Some(emitter.module.as_str());
            for listener in listeners(graph, program, emission.trigger, from) {
                let Some(heard) = program.rule(listener.as_str()) else { continue };
                let parameters = heard.parameters();
                for (parameter, bound) in binding::plan(&carried, &parameters) {
                    let Bound::Unbound(why) = bound else { continue };
                    // The listener's own `let` of that name is bound before
                    // its `when` is read, at run time, and so here.
                    if heard.lets.iter().any(|(name, _)| name == parameter) {
                        continue;
                    }
                    found.push(Unbindable {
                        emitter: emitter.id.clone(),
                        trigger: emission.trigger.to_owned(),
                        span: emission.span,
                        listener: listener.clone(),
                        parameter: parameter.to_owned(),
                        why,
                    });
                }
            }
        }
    }
    found
}

/// The rules a firing of `trigger` wakes, in graph order.
///
/// `from` is the module an emission is written in, and `None` for a firing
/// nobody emitted — an act, or a trigger fired by hand — which wakes every rule
/// waiting on that name, because which module's trigger a person meant is not
/// something to guess. An emission wakes only the rules that [`hears`] it from
/// there. A rule reacting to a creation or a state waits on something no
/// emission's parameters reach, and reads no parameters off its `when`, so it
/// is never reported either.
pub fn listeners<'g>(
    graph: &'g SpecGraph,
    program: &'g Program,
    trigger: &'g str,
    from: Option<&'g str>,
) -> impl Iterator<Item = &'g NodeId> {
    graph
        .nodes_of(NodeKind::Rule)
        .filter(move |node| node.detail.as_rule().is_some_and(|rule| rule.trigger == trigger))
        .filter(move |node| {
            from.is_none_or(|from| {
                let when = program.rule(node.id.as_str()).and_then(|ast| ast.when.as_ref());
                hears(graph, &node.module, when, from)
            })
        })
        .map(|node| &node.id)
}

/// Whether a rule in `module` waiting on `when` hears a trigger emitted in
/// `from` — the simulator asks this same function.
///
/// - `when: chains/Published(doc)` hears what the module it imported `as
///   chains` emits, and nothing else: the qualifier says whose trigger it is.
/// - `when: Published(doc)` hears its own module, and any module it `use`s.
///
/// Two modules that do not import each other and each have a trigger called
/// `Closed` are two triggers that share a word, and one module's emission
/// waking the other's listener reported a correct rule undecided. An emission
/// that names no module is heard everywhere, as before.
#[must_use]
pub fn hears(graph: &SpecGraph, module: &str, when: Option<&Expr>, from: &str) -> bool {
    if from.is_empty() {
        return true;
    }
    let qualifier = match when {
        Some(Expr::Call { function, .. }) => match function.as_ref() {
            Expr::QualifiedName(name) => name.qualifier.as_deref(),
            _ => None,
        },
        _ => None,
    };
    let imports = graph
        .modules
        .iter()
        .find(|it| it.name == module)
        .map(|it| it.imports.as_slice())
        .unwrap_or_default();
    let target = |import: &crate::graph::Import| import.target.as_deref() == Some(from);
    match qualifier {
        Some(alias) => imports.iter().any(|import| import.alias == alias && target(import)),
        None => module == from || imports.iter().any(target),
    }
}

impl Unbindable {
    /// What a reader is told: the emitting rule, the emission and the
    /// parameter, the listener and its `when`, and why.
    ///
    /// The emission and the `when` are quoted as the author wrote them. The
    /// emission is sliced from `source`, the emitter's module, by the span the
    /// parser gave and collapsed onto one line; without the text it is named by
    /// its trigger alone rather than by a spelling this put together.
    ///
    /// The listener is qualified with its module when that is not the
    /// emitter's, because two modules may each have a rule of that name.
    #[must_use]
    pub fn message(&self, graph: &SpecGraph, source: Option<&str>) -> String {
        let emitter = graph.node(&self.emitter);
        let listener = graph.node(&self.listener);
        let emitter_module = emitter.map(|node| node.module.as_str()).unwrap_or_default();
        let emitter_name = emitter.map_or(self.emitter.name(), |node| node.name.as_str());
        let listener_name = match listener {
            Some(node) if node.module != emitter_module => format!("{}/{}", node.module, node.name),
            Some(node) => node.name.clone(),
            None => self.listener.name().to_owned(),
        };
        let emission =
            source.and_then(|source| source.get(self.span.start..self.span.end)).map_or_else(
                || self.trigger.clone(),
                |text| text.split_whitespace().collect::<Vec<_>>().join(" "),
            );
        let when = listener
            .and_then(|node| node.detail.as_rule())
            .and_then(|rule| rule.clauses_of("when").next())
            .map_or_else(|| format!("`{}`", self.trigger), |clause| format!("`{}`", clause.text));
        format!(
            "`{emitter_name}` emits `{emission}` without binding `{}` for `{listener_name}`, \
             which listens for {when}: {}",
            self.parameter, self.why
        )
    }

    /// The same, as a diagnostic at the emission, on the emitting rule.
    ///
    /// `sources` is each module's text, for the line and the column — counted
    /// in characters from the byte offset the parser gave, never from a char
    /// index. A module whose text is not there is reported without a location
    /// rather than at a guessed one.
    #[must_use]
    pub fn diagnostic(&self, graph: &SpecGraph, sources: &BTreeMap<String, String>) -> Diagnostic {
        let module = graph.node(&self.emitter).map(|node| node.module.clone()).unwrap_or_default();
        let file = graph.modules.iter().find(|it| it.name == module).map(|it| it.path.clone());
        let source = sources.get(&module).map(String::as_str);
        let location = source.zip(file).map(|(source, file)| {
            let at = LineIndex::new(source).position(source, self.span.start);
            Location { file, line: at.line, column: at.column }
        });
        Diagnostic {
            severity: Severity::Warning,
            message: self.message(graph, source),
            code: Some(CODE.to_owned()),
            location,
            module,
            node: Some(self.emitter.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::{Command, MemoryReader, ingest, runner::MapRunner};

    /// `modules`, ingested from their source alone: `model` and `plan` are
    /// replayed empty, because nothing here reads what they carry.
    fn ingested(modules: &[(&str, &str)]) -> crate::Ingestion {
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
        ingest(&runner, &reader, &paths).unwrap_or_else(|error| panic!("ingests: {error}"))
    }

    fn found(source: &str) -> Vec<(String, String, String)> {
        let it = ingested(&[("m", source)]);
        unbindable(&it.graph, &it.program)
            .into_iter()
            .map(|it| (it.emitter.name().to_owned(), it.listener.name().to_owned(), it.parameter))
            .collect()
    }

    const HEAD: &str = "-- allium: 3\nentity Doc { seen: Boolean }\n";

    fn spec(body: &str) -> String {
        format!("{HEAD}{body}")
    }

    #[test]
    fn a_named_argument_the_listener_has_no_parameter_for() {
        let found = found(&spec(
            "rule A {\n    when: A(doc)\n    ensures: B(item: doc)\n}\n\
             rule L {\n    when: B(doc)\n    ensures: doc.seen = true\n}\n",
        ));
        assert_eq!(found, [("A".to_owned(), "L".to_owned(), "doc".to_owned())]);
    }

    #[test]
    fn what_binds_is_not_reported() {
        assert!(
            found(&spec(
                "rule A {\n    when: A(doc)\n    ensures: B(doc: doc)\n}\n\
                 rule P {\n    when: P(doc)\n    ensures: B(doc.seen)\n}\n\
                 rule L {\n    when: B(doc)\n    ensures: doc.seen = true\n}\n",
            ))
            .is_empty()
        );
    }

    #[test]
    fn too_few_by_position_and_both_ways() {
        let found = found(&spec(
            "rule A {\n    when: A(doc)\n    ensures: B(doc)\n}\n\
             rule C {\n    when: C(doc)\n    ensures: B(doc, from: doc)\n}\n\
             rule L {\n    when: B(from, to)\n    ensures: from.seen = true\n}\n",
        ));
        let pairs: Vec<(&str, &str)> = found
            .iter()
            .map(|(emitter, _, parameter)| (emitter.as_str(), parameter.as_str()))
            .collect();
        assert_eq!(pairs, [("A", "to"), ("C", "from"), ("C", "to")]);
    }

    /// Inside an `if`, a `for` and a `let`: everywhere a postcondition is
    /// applied. A call in a value is not an emission, and neither is a `let`
    /// of something other than a call.
    #[test]
    fn emissions_are_found_where_postconditions_are_applied() {
        let found = found(&spec(
            "rule A {\n    when: A(doc)\n    ensures:\n        if doc.seen:\n            B(x: doc)\n        else:\n            B(y: doc)\n}\n\
             rule F {\n    when: F(docs)\n    ensures:\n        for d in docs:\n            B(z: d)\n}\n\
             rule G {\n    when: G(doc)\n    ensures:\n        let x = B(w: doc)\n}\n\
             rule V {\n    when: V(doc)\n    ensures: doc.seen = f(B(v: doc))\n}\n\
             rule W {\n    when: W(doc)\n    ensures:\n        let y = doc.seen\n}\n\
             rule L {\n    when: B(doc)\n    ensures: doc.seen = true\n}\n",
        ));
        let emitters: Vec<&str> = found.iter().map(|(emitter, _, _)| emitter.as_str()).collect();
        assert_eq!(emitters, ["A", "A", "F", "G"], "{found:?}");
    }

    #[test]
    fn an_optional_parameter_is_reported_only_beside_a_misnamed_argument() {
        let found = found(&spec(
            "rule A {\n    when: A(doc)\n    ensures: B(doc: doc)\n}\n\
             rule C {\n    when: C(doc)\n    ensures: B(doc: doc, reason: doc)\n}\n\
             rule L {\n    when: B(doc, cause?)\n    ensures: doc.seen = true\n}\n",
        ));
        assert_eq!(found, [("C".to_owned(), "L".to_owned(), "cause".to_owned())]);
    }

    const A: &str = "-- allium: 3\nentity Doc { seen: Boolean }\nrule A {\n    when: A(doc)\n    ensures: B(item: doc)\n}\n";

    /// A listener qualified with the emitter's module hears it from across
    /// the boundary, and is named with its module; the emission is quoted as
    /// written.
    #[test]
    fn a_listener_across_a_module_boundary_is_named_with_its_module() {
        let it = ingested(&[
            ("a", A),
            (
                "b",
                "-- allium: 3\nuse \"./a.allium\" as a\nrule L {\n    when: a/B(doc)\n    ensures: doc.seen = true\n}\n",
            ),
        ]);
        let found = unbindable(&it.graph, &it.program);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            found[0].message(&it.graph, Some(A)),
            "`A` emits `B(item: doc)` without binding `doc` for `b/L`, which listens for \
             `a/B(doc)`: it was handed `item` by name, and a named argument binds only the \
             parameter it names"
        );
        assert!(found[0].message(&it.graph, None).starts_with("`A` emits `B` without"));
    }

    /// Two modules that do not import each other, each with its own `B`, are
    /// two triggers that share a word: neither emission is heard by the other
    /// module's listener, so nothing is reported against a correct spec.
    #[test]
    fn a_module_that_does_not_import_the_emitter_does_not_hear_it() {
        let it = ingested(&[
            ("a", A),
            (
                "b",
                "-- allium: 3\nrule L {\n    when: B(doc, reason)\n    ensures: doc.seen = true\n}\n",
            ),
        ]);
        assert_eq!(unbindable(&it.graph, &it.program), []);
        assert!(it.graph.diagnostics.iter().all(|it| it.code.as_deref() != Some(CODE)));
    }

    /// An unqualified listener hears a module it `use`s as well as its own; a
    /// qualified one hears only the module its qualifier names, so a
    /// listener qualified with `a` does not hear its own module's `B`.
    #[test]
    fn which_modules_a_listener_hears() {
        let it = ingested(&[
            ("a", A),
            (
                "b",
                "-- allium: 3\nuse \"./a.allium\" as a\nentity Note { seen: Boolean }\n\
                 rule Own {\n    when: Own(n)\n    ensures: B(other: n)\n}\n\
                 rule Plain {\n    when: B(doc)\n    ensures: doc.seen = true\n}\n\
                 rule Qualified {\n    when: a/B(doc)\n    ensures: doc.seen = true\n}\n",
            ),
        ]);
        let found = unbindable(&it.graph, &it.program);
        let pairs: Vec<(&str, &str)> =
            found.iter().map(|it| (it.emitter.as_str(), it.listener.as_str())).collect();
        assert_eq!(
            pairs,
            [
                ("a::rule::A", "b::rule::Plain"),
                ("a::rule::A", "b::rule::Qualified"),
                ("b::rule::Own", "b::rule::Plain"),
            ]
        );
    }

    #[test]
    fn a_listener_s_own_let_of_that_name_is_bound() {
        assert!(
            found(&spec(
                "rule A {\n    when: A(doc)\n    ensures: B(item: doc)\n}\n\
                 rule L {\n    when: B(doc)\n    let doc = item\n    ensures: doc.seen = true\n}\n",
            ))
            .is_empty()
        );
    }

    /// The column counts characters from a byte offset: an em-dash before the
    /// emission on its line is three bytes and one column.
    #[test]
    fn the_diagnostic_is_at_the_emission_in_characters() {
        // The dash goes in a string the condition compares against, on the
        // emission's own line.
        let source = spec(
            "rule A {\n    when: A(doc)\n    ensures:\n        if doc.seen = \"—\": B(item: doc)\n}\n\
             rule L {\n    when: B(doc)\n    ensures: doc.seen = true\n}\n",
        );
        let it = ingested(&[("m", &source)]);
        let found = unbindable(&it.graph, &it.program);
        let sources: BTreeMap<String, String> = [("m".to_owned(), source.clone())].into();
        let said = found[0].diagnostic(&it.graph, &sources);
        let at = source.find("B(item").unwrap_or_default();
        let line_start = source[..at].rfind('\n').map_or(0, |newline| newline + 1);
        let location = said.location.unwrap_or_else(|| panic!("a location"));
        assert_eq!(location.column, source[line_start..at].chars().count() + 1);
        assert_ne!(location.column, at - line_start + 1, "bytes and characters differ here");
        assert_eq!(said.code.as_deref(), Some(CODE));
        assert_eq!(said.node.as_deref(), Some("m::rule::A"));
    }

    #[test]
    fn a_call_that_is_not_a_bare_name_is_not_an_emission() {
        let it = ingested(&[(
            "m",
            &spec("rule A {\n    when: A(doc)\n    ensures: Doc.created(seen: false)\n}\n"),
        )]);
        let ast = it.program.rule("m::rule::A").unwrap_or_else(|| panic!("A"));
        assert!(emissions(&ast.ensures).is_empty());
    }
}

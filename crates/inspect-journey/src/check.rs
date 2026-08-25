//! What the specification says about a journey, before anything is run.
//!
//! This pass needs no world and no simulator. It asks only whether the spec
//! declares the things the journey names — and because journeys are written
//! *first*, the answer is usually the most useful output the tool has: these
//! are the surfaces, operations and exposures the specification still owes.
//!
//! So a name the spec does not have is not an error. It is
//! [`Verdict::Unspecified`], which is a requirement nobody has met yet.
//!
//! Two things this deliberately does not do. It does not reject a cast type
//! that differs from the surface's own actor — a surface facing a
//! `membership/Member` and an act taking an `identity/Identity` is either a
//! small inconsistency or a person being both, and a checker that picks is the
//! same failure as a simulator that guesses. And it does not check anything
//! about values, which is the runner's job and needs a world.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

use inspect_model::{
    NodeKind, SpecGraph,
    graph::{NodeDetail, SurfaceDetail},
};

use crate::journey::{Cast, Clause, Given, Journey, Step};

/// What the spec had to say about one line of a journey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub line: usize,
    pub verdict: Verdict,
    pub message: String,
}

/// How a line came out — statically, or once it was walked.
///
/// The three a simulator gives, and the two a journey needs and a simulation
/// does not. `Unspecified` is a requirement nobody has met yet, which is the
/// ordinary state of a journey written before the spec it demands; `Unexposed`
/// is a system that does the right thing and tells nobody.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub enum Verdict {
    /// The spec declares everything this line names, and it held.
    Specified,
    /// Something could not be evaluated, and the detail says what.
    ///
    /// Never a polite way of saying it held. A `cannot see` that could not be
    /// evaluated lands here rather than passing, because a privacy claim that
    /// comes back green because nothing checked it is the worst answer
    /// available.
    Undecided,
    /// The spec forbids it: a precondition is definitely false.
    Refused,
    /// The spec does not have what this names. A requirement, not a mistake.
    Unspecified,
    /// The act exists; nothing lets this actor see the result.
    Unexposed,
    /// Worth a person's attention, and not a reason to stop.
    Remark,
}

/// Check `journey` against `graph`, in the order it was written.
#[must_use]
pub fn check(journey: &Journey, graph: &SpecGraph) -> Vec<Note> {
    let mut notes = Vec::new();
    let mut known = Names::new(journey);

    for member in &journey.cast {
        if let Some(note) = missing_type(member, graph) {
            notes.push(note);
        }
    }
    // A `given` instance is a cast member declared inline, so it is checked as
    // one. Registering its name without checking its type let
    // `given: ghost: Phantom { … }` walk away reporting every step held, which
    // is the same fault the cast had and one line further down.
    for given in &journey.given {
        let Given::Instance { name, type_expr, line, .. } = given else { continue };
        let member = Cast { name: name.clone(), type_expr: type_expr.clone(), line: *line };
        if let Some(note) = missing_type(&member, graph) {
            notes.push(note);
        }
    }
    for step in &journey.steps {
        check_step(step, graph, &mut known, &mut notes);
    }
    notes
}

/// Every name a journey has bound so far, and where.
struct Names {
    types: BTreeMap<String, String>,
}

impl Names {
    fn new(journey: &Journey) -> Self {
        let mut types = BTreeMap::new();
        for member in &journey.cast {
            types.insert(member.name.clone(), member.type_expr.clone());
        }
        for given in &journey.given {
            if let crate::journey::Given::Instance { name, type_expr, .. } = given {
                types.insert(name.clone(), type_expr.clone());
            }
        }
        Self { types }
    }

    fn bind(&mut self, caught: &Cast) {
        self.types.insert(caught.name.clone(), caught.type_expr.clone());
    }

    fn knows(&self, name: &str) -> bool {
        // `config` is not cast: it is the one root every spec has.
        name == "config" || self.types.contains_key(name)
    }
}

fn check_step(step: &Step, graph: &SpecGraph, known: &mut Names, notes: &mut Vec<Note>) {
    for clause in &step.clauses {
        match clause {
            Clause::Does { actor, trigger, arguments, surface, creating, line } => {
                check_actor(actor, *line, known, notes);
                check_act(trigger, surface, *line, graph, notes);
                check_arity(trigger, arguments.len(), *line, graph, notes);
                if let Some(caught) = creating {
                    if let Some(note) = missing_type(caught, graph) {
                        notes.push(note);
                    }
                    known.bind(caught);
                }
            }
            Clause::Sees { actor, subject, surface, context, negated, line } => {
                check_actor(actor, *line, known, notes);
                // Not `check_actor`: a context is a room rather than a
                // person, and "add them to the cast" is the wrong advice for
                // one — a group is usually something `given` made.
                if let Some(context) = context
                    && !known.knows(context)
                {
                    notes.push(Note {
                        line: *line,
                        verdict: Verdict::Unspecified,
                        message: format!(
                            "`{context}` is nothing in this journey — name it in the cast, in \
                             `given`, or catch it with `creating`"
                        ),
                    });
                }
                check_sight(
                    &Sight {
                        seen: &subject.as_written(),
                        surface,
                        context: context.as_deref(),
                        negated: *negated,
                        line: *line,
                    },
                    graph,
                    notes,
                );
            }
            Clause::Then { assertion, line } => match assertion {
                crate::journey::Assertion::Fires { rule, .. }
                    if !declares(graph, NodeKind::Rule, rule) =>
                {
                    notes.push(Note {
                        line: *line,
                        verdict: Verdict::Unspecified,
                        message: format!("no rule called `{rule}`"),
                    });
                }
                crate::journey::Assertion::Within { haystack, .. } => {
                    check_collection(haystack, *line, known, graph, notes);
                }
                _ => {}
            },
            Clause::Stipulate { .. } | Clause::After { .. } => {}
        }
    }
}

/// Is the collection an `in` reads from one the spec declares?
///
/// A capitalised name the journey never bound reads as "every instance of that
/// entity" — `Loans` is every loan. A capitalised name of an entity that does
/// not exist reads as an *empty* set, and `ada in Dragons` then comes back
/// **refused**: the specification saying no to a question about something it
/// has never heard of. A typo blaming the spec is the same failure as a
/// simulator guessing, one level up.
fn check_collection(
    haystack: &crate::journey::Path,
    line: usize,
    known: &Names,
    graph: &SpecGraph,
    notes: &mut Vec<Note>,
) {
    // Only a bare, unbound, capitalised root is read as a collection; anything
    // else is a path through the world and is the walk's business, not this
    // function's.
    if !haystack.segments.is_empty() || known.knows(&haystack.root) {
        return;
    }
    if !haystack.root.chars().next().is_some_and(char::is_uppercase) {
        return;
    }
    let entity = singular(&haystack.root);
    if declares(graph, NodeKind::Entity, &entity)
        || declares(graph, NodeKind::Entity, &haystack.root)
    {
        return;
    }
    notes.push(Note {
        line,
        verdict: Verdict::Unspecified,
        message: format!(
            "no entity called `{entity}`, so `{}` is nothing to be one of",
            haystack.root
        ),
    });
}

/// `Loans` -> `Loan`, `Copies` -> `Copy`. The same reading the world uses.
fn singular(name: &str) -> String {
    if let Some(stem) = name.strip_suffix("ies") {
        return format!("{stem}y");
    }
    name.strip_suffix('s').filter(|stem| !stem.is_empty()).unwrap_or(name).to_owned()
}

fn check_actor(actor: &str, line: usize, known: &Names, notes: &mut Vec<Note>) {
    if !known.knows(actor) {
        notes.push(Note {
            line,
            verdict: Verdict::Unspecified,
            message: format!("`{actor}` is nobody in this journey — add them to the cast"),
        });
    }
}

/// Does the act pass the number of arguments the trigger declares?
///
/// Extra arguments used to be bound to invented names — `arg2`, `arg3` — that
/// no clause reads, so a journey handing an act one argument too many walked
/// away reporting every step held. The rule fired on the two it understood and
/// nothing anywhere mentioned the third.
///
/// Too few is reported for the same reason and reads differently downstream: a
/// parameter nobody bound makes every clause that names it undecided, which is
/// honest but says nothing about *why* it is unbound.
fn check_arity(trigger: &str, given: usize, line: usize, graph: &SpecGraph, notes: &mut Vec<Note>) {
    let Some(detail) = graph
        .nodes_of(NodeKind::Trigger)
        .find(|node| node.name == trigger)
        .and_then(|node| node.detail.as_trigger())
    else {
        // No such trigger is `check_act`'s to report, and saying it twice would
        // send the reader looking for two faults.
        return;
    };
    let declared = detail.parameters.len();
    // An optional parameter may be left off, which is the whole of what `?`
    // means. Counting them as required refused every ordinary call of
    // `MemberSends(group, author, body, parents, attachment_name?,
    // attachment_size?)` — six declared, four written, and nobody ever writes
    // six.
    let required = declared - detail.optional.len();
    if given >= required && given <= declared {
        return;
    }
    notes.push(Note {
        line,
        verdict: Verdict::Unspecified,
        message: format!(
            "`{trigger}` takes {}, and this gives {given}: {}",
            if required == declared {
                format!("{declared} argument{}", if declared == 1 { "" } else { "s" })
            } else {
                format!("{required} to {declared} arguments")
            },
            if detail.parameters.is_empty() {
                "it declares none".to_owned()
            } else {
                detail.parameters.join(", ")
            }
        ),
    });
}

/// Does some surface offer this act, and does *this* one?
fn check_act(trigger: &str, surface: &str, line: usize, graph: &SpecGraph, notes: &mut Vec<Note>) {
    let Some(detail) = surface_named(graph, surface) else {
        notes.push(Note {
            line,
            verdict: Verdict::Unspecified,
            message: format!("no surface called `{surface}`"),
        });
        return;
    };
    if detail.provides.iter().any(|operation| operation.trigger == trigger) {
        return;
    }

    // Offered somewhere else is a different problem from offered nowhere, and
    // the fix differs: one is the wrong surface, the other is an operation
    // nobody has specified.
    let elsewhere: Vec<&str> = graph
        .nodes_of(NodeKind::Surface)
        .filter_map(|node| Some((node.name.as_str(), node.detail.as_surface()?)))
        .filter(|(_, other)| other.provides.iter().any(|operation| operation.trigger == trigger))
        .map(|(name, _)| name)
        .collect();

    notes.push(Note {
        line,
        verdict: Verdict::Unspecified,
        message: if elsewhere.is_empty() {
            format!("no surface offers `{trigger}`")
        } else {
            format!("`{surface}` does not offer `{trigger}` — {} does", elsewhere.join(", "))
        },
    });
}

/// Does this surface expose the thing the journey says somebody can see?
///
/// A surface's `exposes` clause is text — `for m in group.messages: m.body` —
/// so the question asked here is whether the field path appears in it. That is
/// enough to answer the two cases worth answering: a value no surface mentions,
/// and a value this surface does not. Whether the clause's *filter* admits this
/// particular actor is a question about a world, and the runner asks it.
struct Sight<'a> {
    seen: &'a str,
    surface: &'a str,
    /// Which instance of the surface's context, when the journey says.
    context: Option<&'a str>,
    negated: bool,
    line: usize,
}

fn check_sight(sight: &Sight<'_>, graph: &SpecGraph, notes: &mut Vec<Note>) {
    let Sight { seen, surface, negated, line, .. } = *sight;
    let Some(detail) = surface_named(graph, surface) else {
        notes.push(Note {
            line,
            verdict: Verdict::Unspecified,
            message: format!("no surface called `{surface}`"),
        });
        return;
    };
    // `in <name>` names an instance of the surface's `context`, so a surface
    // that has no context has nothing for it to name. Reported here rather
    // than ignored: silently dropping it would leave a journey saying which
    // room it means and a tool answering about a different question.
    if sight.context.is_some() && detail.context.is_none() {
        notes.push(Note {
            line,
            verdict: Verdict::Unspecified,
            message: format!("`{surface}` is not scoped to anything, so `in` has nothing to name"),
        });
    }

    if exposes(detail, seen) {
        return;
    }

    // Not exposing it is what `cannot see` asked for, and the strongest form of
    // it: not "no instance matched" but "this boundary does not carry it".
    notes.push(Note {
        line,
        verdict: if negated { Verdict::Specified } else { Verdict::Unexposed },
        message: if negated {
            format!("`{surface}` exposes nothing like `{seen}`, so nobody sees it there")
        } else {
            format!("`{surface}` exposes nothing like `{seen}`")
        },
    });
}

/// Whether an `exposes` clause mentions this path.
///
/// Matched on the tail rather than the whole: a journey says `loan.status`
/// where a surface writes `Loan.status`, and a projection binds its own name
/// with `for m in group.messages: m.body`. The tail is the part both agree on.
pub(crate) fn exposes(detail: &SurfaceDetail, seen: &str) -> bool {
    // A call the surface exposes: `exposes: announces_reads(owner)`, asked
    // about as `announces_reads(ada)`. The names are what have to match — the
    // arguments are the surface's own binding on one side and whoever that is
    // on the other, which is one call about one person written two ways, and
    // comparing the text of them says no to every case there is.
    if let Some((name, _)) = seen.split_once('(') {
        let opens = format!("{name}(");
        return detail.exposes.iter().any(|clause| clause.trim_start().starts_with(&opens));
    }

    let Some((_, tail)) = seen.rsplit_once('.') else {
        return detail.exposes.iter().any(|clause| mentions(clause, seen));
    };
    let field = format!(".{tail}");
    detail.exposes.iter().any(|clause| clause.ends_with(&field) || mentions(clause, &field))
}

fn mentions(clause: &str, needle: &str) -> bool {
    clause.split_whitespace().any(|word| word == needle || word.ends_with(needle))
}

/// The node id and module of the surface called `name`.
pub(crate) fn surface_id(graph: &SpecGraph, name: &str) -> Option<(String, String)> {
    graph
        .nodes_of(NodeKind::Surface)
        .find(|node| node.name == name)
        .map(|node| (node.id.to_string(), node.module.clone()))
}

pub(crate) fn surface_named<'a>(graph: &'a SpecGraph, name: &str) -> Option<&'a SurfaceDetail> {
    graph
        .nodes_of(NodeKind::Surface)
        .find(|node| node.name == name)
        .and_then(|node| node.detail.as_surface())
}

/// Whether the spec declares a construct of this kind with this name.
fn declares(graph: &SpecGraph, kind: NodeKind, name: &str) -> bool {
    graph.nodes_of(kind).any(|node| node.name == name)
}

/// The construct a cast member's type names, if the spec has one.
fn missing_type(member: &Cast, graph: &SpecGraph) -> Option<Note> {
    // Qualified names carry their module: `catalogue/Copy` is `Copy` over there.
    let bare = member.type_expr.rsplit('/').next().unwrap_or(&member.type_expr);
    let found = graph.nodes.iter().any(|node| {
        node.name == bare
            && matches!(
                node.detail,
                NodeDetail::Entity(_) | NodeDetail::Actor(_) | NodeDetail::None
            )
            && matches!(node.kind, NodeKind::Entity | NodeKind::Actor | NodeKind::Variant)
    });
    if found {
        return None;
    }
    Some(Note {
        line: member.line,
        verdict: Verdict::Unspecified,
        message: format!("no entity or actor called `{}`", member.type_expr),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface(exposes: &[&str]) -> SurfaceDetail {
        SurfaceDetail {
            actor: None,
            actor_binding: None,
            context: None,
            exposes: exposes.iter().map(|clause| (*clause).to_owned()).collect(),
            provides: Vec::new(),
            guarantees: Vec::new(),
        }
    }

    #[test]
    fn a_surface_that_names_the_field_exposes_it() {
        // A journey writes `loan.status` where the surface writes `Loan.status`
        // — the instance against the type. The tail is the part both agree on,
        // and matching on the whole would report every real spec as hiding
        // everything.
        let desk = surface(&["Loan.status", "Member.open_loan_count"]);
        assert!(exposes(&desk, "loan.status"));
        assert!(exposes(&desk, "ada.open_loan_count"));
    }

    #[test]
    fn a_surface_that_does_not_name_it_does_not_expose_it() {
        let desk = surface(&["Loan.status"]);
        assert!(!exposes(&desk, "copy.shelfmark"));
        assert!(!exposes(&desk, "loan.window"));
    }

    #[test]
    fn a_field_named_partway_through_a_clause_still_counts() {
        // A projection is the ordinary shape in a real spec: `for m in
        // group.messages: m.body` exposes `body`, and the clause does not end
        // there. Requiring the clause to *end* with the field would report the
        // whole projection as hiding what it plainly shows.
        let wall = surface(&["for m in group.messages: m.body where m.status = live"]);
        assert!(exposes(&wall, "message.body"));
    }

    #[test]
    fn a_field_named_only_at_the_very_end_still_counts_too() {
        // The other half of the same test. The two conditions cover different
        // clauses, and either one alone leaves a real surface misread.
        let shelf = surface(&["Loan.is_late"]);
        assert!(exposes(&shelf, "loan.is_late"));
    }

    #[test]
    fn a_bare_name_with_no_field_is_matched_whole() {
        let shelf = surface(&["Loan", "Member.name"]);
        assert!(exposes(&shelf, "Loan"));
        assert!(!exposes(&shelf, "Copy"));
    }

    #[test]
    fn mentioning_is_by_word_rather_than_by_substring() {
        // `.status` must not match `.status_history`, and `body` must not match
        // `nobody`. A substring search would report a surface as exposing a
        // field it has never heard of, which is the one direction of error this
        // check must not make: it would say the spec supports a journey it does
        // not.
        assert!(mentions("Loan.status", ".status"));
        assert!(mentions("for m in x: m.body", ".body"));
        assert!(!mentions("Loan.status_history", ".status"));
        assert!(!mentions("Loan.statuses where x", ".status"));
    }

    #[test]
    fn a_word_that_is_exactly_the_needle_counts() {
        // `ends_with` covers it too, but only by accident of the needle being
        // the whole word — and the two halves are read by different clauses.
        assert!(mentions("Loan status", "status"));
        assert!(!mentions("Loan", "status"));
        assert!(!mentions("", "status"));
    }
}

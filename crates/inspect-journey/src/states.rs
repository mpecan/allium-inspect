//! A state named in an act, checked against the states the spec declares.
//!
//! An act's arguments are words, and a bare word the journey did not bind is
//! read by the walker as the state it spells — `changed_mind`, `set_admin`.
//! That reading is right, and it has one way of going quietly wrong: a word
//! that is no state anything declares, handed to a rule that compares it
//! against states. `MemberProposes(room, ada, removal, bruno)` against a rule
//! written `decision = set_admin` walks; `removal` reaches the rule; no clause
//! can ever match it; and the rule reads as a gap in the specification. A
//! journey spent months blaming the walker for a rule that could never fire,
//! over a word.
//!
//! So the checker says so, before anything is walked — and only then. Both
//! halves have to hold, because each alone has a false positive a real spec set
//! trips over at once:
//!
//! - *No enumeration declares it.* A word that is a state somewhere is very
//!   likely the state meant, and a trigger's parameters carry no types to say
//!   otherwise.
//! - *The rule compares that parameter against states.* `PersonCreatesGroup(
//!   she, "Bruno", genesis, …)` hands an unbound word to a rule that only
//!   stores it, and nothing in that rule is ever asked to match it. Reporting
//!   it would stop a step the spec supports.

use std::collections::BTreeSet;

use allium_parser::ast::{ComparisonOp, Expr};
use inspect_model::{NodeKind, Program, RuleAst, SpecGraph, graph::TriggerSource, ingest};

use crate::{
    check::{Note, Verdict},
    journey::Term,
};

/// What the checker needs to say whether one act names a state nobody has.
pub(crate) struct Act<'a> {
    pub(crate) trigger: &'a str,
    pub(crate) arguments: &'a [Term],
    pub(crate) line: usize,
}

/// A note for each argument of `act` that is nobody, no state, and compared
/// against states by a rule it reaches.
pub(crate) fn check(
    act: &Act<'_>,
    knows: impl Fn(&str) -> bool,
    graph: &SpecGraph,
    program: &Program,
    notes: &mut Vec<Note>,
) {
    let mut declared = None;
    for (position, argument) in act.arguments.iter().enumerate() {
        let Term::Path(path) = argument else { continue };
        if !path.segments.is_empty() || knows(&path.root) {
            continue;
        }
        if declared.get_or_insert_with(|| graph.declared_states()).contains(path.root.as_str()) {
            continue;
        }
        let Some((rule, parameter, states)) = compared(act.trigger, position, graph, program)
        else {
            continue;
        };
        notes.push(Note {
            line: act.line,
            verdict: Verdict::Unspecified,
            message: format!(
                "`{}` is no state the spec declares, and `{rule}` compares `{parameter}` against \
                 states — {} — so it can never match",
                path.root,
                states.into_iter().collect::<Vec<_>>().join(", ")
            ),
        });
    }
}

/// The first rule on `trigger` that compares the parameter at `position`
/// against states, with that parameter's name and the states it names.
fn compared(
    trigger: &str,
    position: usize,
    graph: &SpecGraph,
    program: &Program,
) -> Option<(String, String, BTreeSet<String>)> {
    graph.nodes_of(NodeKind::Rule).find_map(|node| {
        let detail = node.detail.as_rule()?;
        if detail.trigger != trigger || detail.source != TriggerSource::External {
            return None;
        }
        let ast = program.rule(node.id.as_str())?;
        let parameters: Vec<String> = ast.parameters().into_iter().map(|(name, _)| name).collect();
        let parameter = parameters.get(position).filter(|name| !name.is_empty())?.clone();
        let states = states_against(ast, &parameter, &parameters);
        (!states.is_empty()).then(|| (node.name.clone(), parameter, states))
    })
}

/// Every bare name the rule compares `parameter` with: `decision = set_admin`,
/// `reason in {changed_mind, found_elsewhere}`.
///
/// A bare name that is another of the rule's own names — a parameter or a
/// `let` — is a binding, not a state, and is left out.
fn states_against(ast: &RuleAst, parameter: &str, parameters: &[String]) -> BTreeSet<String> {
    let bound = |name: &str| {
        parameters.iter().any(|other| other == name) || ast.lets.iter().any(|(own, _)| own == name)
    };
    let is_parameter = |expr: &Expr| matches!(expr, Expr::Ident(ident) if ident.name == parameter);
    let state = |expr: &Expr| match expr {
        Expr::Ident(ident) if !bound(&ident.name) => Some(ident.name.clone()),
        _ => None,
    };

    let mut states = BTreeSet::new();
    let mut pending: Vec<&Expr> =
        ast.lets.iter().map(|(_, value)| value).chain(&ast.requires).chain(&ast.ensures).collect();
    while let Some(expr) = pending.pop() {
        match expr {
            Expr::Comparison {
                left, op: ComparisonOp::Eq | ComparisonOp::NotEq, right, ..
            } => {
                if is_parameter(left) {
                    states.extend(state(right));
                } else if is_parameter(right) {
                    states.extend(state(left));
                }
            }
            Expr::In { element, collection, .. } | Expr::NotIn { element, collection, .. }
                if is_parameter(element) =>
            {
                if let Expr::SetLiteral { elements, .. } | Expr::ListLiteral { elements, .. } =
                    collection.as_ref()
                {
                    states.extend(elements.iter().filter_map(state));
                }
            }
            _ => {}
        }
        pending.extend(ingest::subexpressions(expr));
    }
    states
}

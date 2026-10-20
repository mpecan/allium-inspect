//! Which context an act is at, from a guard on its `provides` line.
//!
//! `Member` is identified within a `Group`, and `Conversation` is at one. An
//! act that carries the group — `MemberPosts(group, reader)` — is at the group
//! it was given. Reading a message carries the message, not its group, so the
//! line said nothing about which group Ada was reading in, and every read by
//! every member came back undecided.
//!
//! `PersonReadsMessage(message, reader) when message.group = group` is the
//! surface saying which, once, where the operation is declared: the act is at
//! the message's group. So a guard is read for a **tie** — `group = …` or
//! `… = group`, on its own or as one side of an `and` — and the other side is
//! evaluated with the line's parameters bound to the act's arguments.
//!
//! Nothing looser counts. Under an `or` the act is offered at a group the tie
//! does not name, an inequality names no group at all, and two ties landing on
//! different groups leave the choice to the walk. Each of those is undecided,
//! saying why; picking would be this tool deciding where somebody is.
//!
//! Only the tie is read. Whether the guard as a whole holds — whether the
//! operation is offered at all — is not asked here, nor anywhere else in the
//! walk yet.

use std::collections::BTreeMap;

use allium_parser::ast::{ComparisonOp, Expr, LogicalOp};
use inspect_sim::{Value, eval::span_of, value::EntityId};

use crate::run::{Act, Walker};

/// Where an act stands: the contexts it placed, and why each it did not
/// place was not.
#[derive(Debug, Default)]
pub(crate) struct Placing {
    /// Each context of the surface placed so far, by entity.
    pub(crate) placed: Vec<(String, EntityId)>,
    /// Why a context the surface's `provides` does not pass was not placed by
    /// its guard either, by the context's entity.
    pub(crate) untied: BTreeMap<String, Untied>,
}

/// Why a guard did not say which context an act is at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Untied {
    /// There is no guard; one like `example` would say which.
    Unsaid {
        /// `message.group = group`, or as near to it as the line allows.
        example: String,
    },
    /// There is a guard, and it does not settle it, said as a clause that
    /// follows "and".
    Unsettled(String),
}

/// The operation an act fires, as its surface declares it.
pub(crate) struct Offer<'a> {
    pub(crate) surface: &'a str,
    pub(crate) module: &'a str,
    /// The parameter names of the `provides` line, in order.
    pub(crate) parameters: &'a [String],
    /// The binding the surface gives whoever it faces, never a tie's example.
    pub(crate) facing: Option<&'a str>,
    pub(crate) guard: Option<&'a Expr>,
}

impl Walker<'_> {
    /// The instance of the context `name` (of entity `entity`) that `offer`'s
    /// guard ties `act` to.
    pub(crate) fn tied(
        &self,
        act: &Act<'_>,
        offer: &Offer<'_>,
        (name, entity): (&str, &str),
    ) -> Result<EntityId, Untied> {
        let Offer { surface, module, parameters, facing, guard } = *offer;
        let Some(guard) = guard else {
            return Err(Untied::Unsaid { example: example(parameters, name, facing) });
        };
        let source = self.sources.get(module).map_or("", String::as_str);
        let written = |expr: &Expr| {
            span_of(expr).and_then(|span| span.slice(source)).unwrap_or("its guard").to_owned()
        };

        let trigger = act.trigger.rsplit('/').next().unwrap_or(act.trigger);
        let sides = ties(guard, name);
        let Some((first, rest)) = sides.split_first() else {
            return Err(Untied::Unsettled(format!(
                "`{surface}` offers `{trigger}` only `when {}`, which ties `{name}` to nothing — \
                 only `{name} = …` on its own or under `and` says which",
                written(guard)
            )));
        };

        let scope: BTreeMap<String, Value> = parameters
            .iter()
            .zip(act.arguments)
            .map(|(parameter, argument)| (parameter.clone(), self.value_of(argument)))
            .collect();
        let settle = |side: &Expr| {
            let as_written = written(side);
            let passes = format!("`{surface}` passes `{name}` as `{as_written}`");
            match self.evaluate(side, &scope, module) {
                Err(why) => Err(Untied::Unsettled(format!("{passes}, and {}", self.named(&why)))),
                Ok((Value::Ref(id), _))
                    if self.world.instance(&id).is_some_and(|it| it.entity == entity) =>
                {
                    Ok((as_written, id))
                }
                Ok((other, _)) => Err(Untied::Unsettled(format!(
                    "{passes}, which is {}, not a `{entity}`",
                    self.named(&other.described())
                ))),
            }
        };
        let (one, at) = settle(first)?;
        for side in rest {
            let (other, elsewhere) = settle(side)?;
            if elsewhere != at {
                return Err(Untied::Unsettled(format!(
                    "`{surface}` passes `{name}` as both `{one}` and `{other}`, which are not \
                     the same `{entity}`"
                )));
            }
        }
        Ok(at)
    }

    /// The evaluator's sentence about the world, said about the journey: an
    /// instance by the name the journey gave it rather than the world's id.
    fn named(&self, why: &str) -> String {
        self.bound.iter().fold(why.to_owned(), |said, (name, id)| {
            said.replace(&format!("`{id}`"), &format!("`{name}`"))
                .replace(&format!("`{id}."), &format!("`{name}."))
        })
    }
}

/// A tie that would say which `name` an act is at, through the first of
/// `parameters` that is neither that context nor whoever the surface faces.
fn example(parameters: &[String], name: &str, facing: Option<&str>) -> String {
    let carrier = parameters
        .iter()
        .find(|it| it.as_str() != name && Some(it.as_str()) != facing)
        .map_or("<argument>", String::as_str);
    format!("{carrier}.{name} = {name}")
}

/// The other side of every `name = …` or `… = name` in `guard`, read at its
/// top level and through `and` only.
fn ties<'e>(guard: &'e Expr, name: &str) -> Vec<&'e Expr> {
    let is_name = |expr: &Expr| matches!(expr, Expr::Ident(it) if it.name == name);
    match guard {
        Expr::LogicalOp { left, op: LogicalOp::And, right, .. } => {
            let mut sides = ties(left, name);
            sides.extend(ties(right, name));
            sides
        }
        Expr::Comparison { left, op: ComparisonOp::Eq, right, .. } => {
            match (is_name(left), is_name(right)) {
                (true, false) => vec![right.as_ref()],
                (false, true) => vec![left.as_ref()],
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `guard`, parsed as the condition of a `provides` line's `when`.
    fn guard_of(guard: &str) -> Expr {
        let source =
            format!("surface S {{\n    provides:\n        Act(message) when {guard}\n}}\n");
        let module = allium_parser::parse(&source).module;
        let found = module.declarations.iter().find_map(|declaration| {
            let allium_parser::ast::Decl::Block(block) = declaration else { return None };
            block.items.iter().find_map(|item| match &item.kind {
                allium_parser::ast::BlockItemKind::Clause { value, .. } => match value {
                    Expr::WhenGuard { condition, .. } => Some(condition.as_ref().clone()),
                    Expr::Block { items, .. } => items.iter().find_map(|it| match it {
                        Expr::WhenGuard { condition, .. } => Some(condition.as_ref().clone()),
                        _ => None,
                    }),
                    _ => None,
                },
                _ => None,
            })
        });
        found.expect("the guard")
    }

    fn sides(guard: &str) -> usize {
        ties(&guard_of(guard), "group").len()
    }

    #[test]
    fn a_tie_is_read_either_way_round_and_through_and() {
        assert_eq!(sides("message.group = group"), 1);
        assert_eq!(sides("group = message.group"), 1);
        assert_eq!(sides("message.read = false and message.group = group"), 1);
        assert_eq!(sides("message.group = group and group = message.other"), 2);
    }

    /// The example is carried by an argument of the act, never by whoever
    /// is at the surface or by the context it is about.
    #[test]
    fn the_example_tie_is_carried_by_neither_the_facing_nor_the_context() {
        let line = |names: &[&str]| names.iter().map(|it| (*it).to_owned()).collect::<Vec<_>>();
        let facing = Some("reader");
        assert_eq!(
            example(&line(&["reader", "message"]), "group", facing),
            "message.group = group"
        );
        assert_eq!(example(&line(&["group", "message"]), "group", facing), "message.group = group");
        assert_eq!(example(&line(&["message", "reader"]), "group", None), "message.group = group");
        assert_eq!(example(&line(&["reader"]), "group", facing), "<argument>.group = group");
    }

    #[test]
    fn nothing_looser_than_a_tie_is_read_as_one() {
        assert_eq!(sides("message.group = group or message.pinned = true"), 0);
        assert_eq!(sides("message.group != group"), 0);
        assert_eq!(sides("not (message.group = group)"), 0);
        assert_eq!(sides("message.other = message.group"), 0);
        assert_eq!(sides("group = group"), 0);
    }
}

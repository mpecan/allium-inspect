//! The world a journey says exists, and reading values back out of it.
//!
//! `given` is precise on purpose. There is no `a group with two members`
//! shorthand, because a shorthand invents a shape the specification never
//! stated — and putting facts in the spec's mouth is the one thing this tool
//! does not do anywhere else. The setup will sometimes be longer than the
//! journey, and that is the cost of the journey meaning something.
//!
//! One world, many instances. Two people of the same kind with different
//! preconditions is the ordinary case rather than the interesting one, and it
//! is a precondition on an instance rather than a second world — which is what
//! the spec itself models, with sets like `OutboxEntry.awaiting` naming the
//! devices that do not have it yet.

use inspect_model::{NodeKind, SpecGraph};
use inspect_sim::{Value, value::EntityId};

use crate::{
    Outcome,
    check::Verdict,
    journey::{Cast, Given, Journey, Path, Term},
    run::{CastMember, Origin, Walker},
};

impl Walker<'_> {
    /// Build the world the journey says exists.
    pub(crate) fn lay_out(&mut self, journey: &Journey) {
        // Cast first, so `given` can assign to any of them. A cast member is an
        // instance of its type: two people of one kind is the ordinary case.
        for member in &journey.cast {
            let created = self.create(&member.type_expr);
            self.bind(member, Some(created), Origin::Cast);
        }
        for given in &journey.given {
            match given {
                Given::Instance { name, type_expr, fields, line } => {
                    let id = self.create(type_expr);
                    for (field, value) in fields {
                        let value = self.value_of(value);
                        self.world.set_field(&id, field, value);
                    }
                    // A given instance is a cast member declared inline, so it
                    // goes through the same door under the same shape.
                    let who =
                        Cast { name: name.clone(), type_expr: type_expr.clone(), line: *line };
                    self.bind(&who, Some(id), Origin::Given);
                }
                Given::Assign { path, value, line } => {
                    let value = self.value_of(value);
                    // A `given` that wrote nothing is the same fault as a
                    // stipulation that wrote nothing, one step earlier: the
                    // journey believes it set the world up and every assertion
                    // after it is answered against a world nobody arranged.
                    if let Err(reason) = self.assign(path, value) {
                        self.notes.push(Outcome {
                            line: *line,
                            verdict: Verdict::Undecided,
                            about: format!("given {}", path.as_written()),
                            detail: Some(reason),
                        });
                    }
                }
            }
        }
    }

    /// An instance of `type_expr`, in the module that declares it.
    pub(crate) fn create(&mut self, type_expr: &str) -> EntityId {
        let bare = type_expr.rsplit('/').next().unwrap_or(type_expr);
        let module = declaring_module(self.spec, bare);
        self.world.create(bare, &module)
    }

    /// Record a name the journey bound, and what it bound it to.
    ///
    /// One place, because the report is only useful if it lists *everybody* —
    /// a cast member, a given instance and a thing a step caught are all people
    /// as far as a reader is concerned, and one of the three going unlisted is
    /// the kind of gap nobody notices until they are looking for it.
    pub(crate) fn bind(&mut self, who: &Cast, id: Option<EntityId>, origin: Origin) {
        if let Some(id) = &id {
            self.bound.insert(who.name.clone(), id.clone());
        }
        self.cast.push(CastMember {
            name: who.name.clone(),
            type_expr: who.type_expr.clone(),
            entity: id.map(|id| id.as_str().to_owned()),
            origin,
            line: who.line,
        });
    }

    /// Write a value the journey asserted, or say why nothing was written.
    ///
    /// Every way of failing is a reason rather than a silent return, because
    /// the stipulation ledger this feeds is the guardrail the whole design
    /// leans on: an agent can make any journey pass, but it cannot make one
    /// pass *invisibly*. A write that quietly did nothing and then listed
    /// itself anyway shows the reader a change to the world that never
    /// happened, which is worse than showing them nothing.
    pub(crate) fn assign(&mut self, path: &Path, value: Value) -> Result<(), String> {
        let Some(id) = self.bound.get(&path.root).cloned() else {
            return Err(format!("`{}` is not a name this journey bound", path.root));
        };
        let Some((field, through)) = path.segments.split_last() else {
            return Err(format!("`{}` names no field to set", path.as_written()));
        };
        // Follow everything before the last segment, so `loan.window.due_at`
        // writes `due_at` on the window. Taking the *first* segment instead
        // wrote `window` on the loan — a field the journey never named, with
        // the ledger printing the path in full underneath it.
        let mut current = id;
        for segment in through {
            match self.world.instance(&current).map(|instance| instance.field(segment)) {
                Some(Value::Ref(id)) => current = id,
                _ => {
                    return Err(format!(
                        "`{}` cannot be followed: `{segment}` is not set to something with fields",
                        path.as_written()
                    ));
                }
            }
        }
        // `set_field` answers `None` only when there is no such instance — a
        // field that was simply unset still answers `Some(Unknown)`. So this is
        // the one place a write can fail after the path itself resolved, and it
        // covers a reference left pointing at something gone without needing a
        // second existence check inside the loop above.
        if self.world.set_field(&current, field, value).is_none() {
            return Err(format!(
                "`{}` points at something that is no longer in the world",
                path.as_written()
            ));
        }
        Ok(())
    }

    /// A term as a value in this world.
    pub(crate) fn value_of(&self, term: &Term) -> Value {
        match term {
            Term::Literal(value) => value.clone(),
            Term::Set(items) => Value::Set(items.iter().map(|item| self.value_of(item)).collect()),
            Term::Path(path) => self.read(path),
            // Resolved against the world's clock at the moment it is read, so
            // `now + 1.day` in a `given` means a day after the world started
            // and the same words in a `stipulate` after `after 3.days` mean a
            // day after *that*. Both are what the line says where it is.
            Term::Clock { offset, .. } => Value::Timestamp(self.world.now + offset),
        }
    }

    /// Walk a path through the world.
    ///
    /// Following `Ref`s from one instance to the next, with `count` answered
    /// from whatever it lands on. A path that runs out is `Unknown` rather than
    /// an error, which is the same answer the simulator gives and for the same
    /// reason.
    pub(crate) fn read(&self, path: &Path) -> Value {
        if path.root == "config" {
            let key = path.segments.join(".");
            return self
                .world
                .config
                .iter()
                .find(|(name, _)| name.ends_with(&key))
                .map_or(Value::Unknown, |(_, value)| value.clone());
        }
        let Some(id) = self.bound.get(&path.root) else {
            if !path.segments.is_empty() {
                return Value::Unknown;
            }
            // Capitalised and unbound is a collection, which is the rule the
            // evaluator already follows: `Loans` is every loan in the world.
            // It is how a journey says "and it is one of them" without having
            // cast every instance by hand.
            if let Some(entity) = collection(&path.root) {
                return Value::Set(
                    self.world
                        .instances_of(&entity)
                        .map(|instance| Value::Ref(instance.id.clone()))
                        .collect(),
                );
            }
            // Otherwise a state the spec declares: `available`, `open`,
            // `active`. A name that is neither reaches a comparison and comes
            // back false, which is the same answer as comparing against the
            // wrong state and is what the detail line is for.
            return Value::Enum(path.root.clone());
        };
        let mut current = Value::Ref(id.clone());
        for segment in &path.segments {
            if segment == "count" {
                return current.count().map_or(Value::Unknown, |n| {
                    i64::try_from(n).map_or(Value::Unknown, Value::Int)
                });
            }
            let Value::Ref(id) = &current else { return Value::Unknown };
            let Some(instance) = self.world.instance(id) else { return Value::Unknown };
            current = instance.field(segment);
        }
        current
    }
}

/// The entity a capitalised name collects, if it is one.
///
/// `Loans` -> `Loan`, `Copies` -> `Copy`, and `Loan` -> `Loan`, because a
/// journey that writes the type name means the instances of it either way. A
/// lowercase name is a state the spec declares and is left alone.
fn collection(name: &str) -> Option<String> {
    if !name.chars().next().is_some_and(char::is_uppercase) {
        return None;
    }
    if let Some(stem) = name.strip_suffix("ies") {
        return Some(format!("{stem}y"));
    }
    Some(name.strip_suffix('s').filter(|stem| !stem.is_empty()).unwrap_or(name).to_owned())
}

/// The module that declares the type called `bare`.
///
/// The trigger exclusion is not defensive. A state-condition rule's trigger
/// *is* the entity name — `when: loan: Loan.window.due_at <= now` puts a
/// trigger node called `Loan` in the graph beside the entity called `Loan` —
/// so a search that took the first match would resolve a cast against the
/// trigger and put the instance in whichever module that trigger came from.
fn declaring_module(spec: &SpecGraph, bare: &str) -> String {
    spec.nodes
        .iter()
        .find(|node| node.name == bare && node.kind != NodeKind::Trigger)
        .map_or_else(|| bare.to_owned(), |node| node.module.clone())
}

#[cfg(test)]
mod tests {
    use inspect_model::Node;

    use super::*;

    fn spec(nodes: &[(&str, NodeKind, &str)]) -> SpecGraph {
        let mut graph = SpecGraph::new("test");
        for (module, kind, name) in nodes {
            graph.nodes.push(Node::new(module, *kind, name));
        }
        graph
    }

    #[test]
    fn a_capitalised_name_collects_its_instances_however_it_is_spelled() {
        assert_eq!(collection("Loans").as_deref(), Some("Loan"));
        assert_eq!(collection("Loan").as_deref(), Some("Loan"));
        assert_eq!(collection("Copies").as_deref(), Some("Copy"));
    }

    #[test]
    fn a_lowercase_name_is_a_state_rather_than_a_collection() {
        // `then loan.status = open` compares against the state `open`. Reading
        // it as the collection of every `ope` would answer that comparison
        // with an empty set, which is false rather than undecided — a wrong
        // answer stated confidently.
        assert_eq!(collection("open"), None);
        assert_eq!(collection("available"), None);
        assert_eq!(collection(""), None);
    }

    #[test]
    fn a_cast_lands_in_the_module_that_declares_its_type() {
        let graph =
            spec(&[("catalogue", NodeKind::Entity, "Copy"), ("lending", NodeKind::Entity, "Loan")]);
        assert_eq!(declaring_module(&graph, "Copy"), "catalogue");
        assert_eq!(declaring_module(&graph, "Loan"), "lending");
    }

    #[test]
    fn a_trigger_of_the_same_name_does_not_win() {
        // A state-condition rule's trigger is the entity's own name, so the two
        // always collide. Resolving to the trigger would file the instance in
        // whichever module the *rule* lives in, and every clause about it would
        // then read against an empty module.
        let graph = spec(&[
            ("lending", NodeKind::Trigger, "Copy"),
            ("catalogue", NodeKind::Entity, "Copy"),
        ]);
        assert_eq!(declaring_module(&graph, "Copy"), "catalogue");
    }

    #[test]
    fn a_type_the_spec_never_declares_stands_for_its_own_module() {
        // Nothing better is available, and inventing a module would file the
        // instance somewhere a later lookup would not think to look.
        assert_eq!(declaring_module(&spec(&[]), "Ghost"), "Ghost");
    }
}

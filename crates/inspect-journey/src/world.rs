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

use inspect_model::NodeKind;
use inspect_sim::{Value, value::EntityId};

use crate::{
    journey::{Given, Journey, Path, Term},
    run::Walker,
};

impl Walker<'_> {
    /// Build the world the journey says exists.
    pub(crate) fn lay_out(&mut self, journey: &Journey) {
        // Cast first, so `given` can assign to any of them. A cast member is an
        // instance of its type: two people of one kind is the ordinary case.
        for member in &journey.cast {
            let created = self.create(&member.type_expr);
            self.bound.insert(member.name.clone(), created);
        }
        for given in &journey.given {
            match given {
                Given::Instance { name, type_expr, fields, .. } => {
                    let id = self.create(type_expr);
                    for (field, value) in fields {
                        let value = self.value_of(value);
                        self.world.set_field(&id, field, value);
                    }
                    self.bound.insert(name.clone(), id);
                }
                Given::Assign { path, value, .. } => {
                    let value = self.value_of(value);
                    self.assign(path, value);
                }
            }
        }
    }

    /// An instance of `type_expr`, in the module that declares it.
    pub(crate) fn create(&mut self, type_expr: &str) -> EntityId {
        let bare = type_expr.rsplit('/').next().unwrap_or(type_expr);
        let module = self
            .spec
            .nodes
            .iter()
            .find(|node| node.name == bare && node.kind != NodeKind::Trigger)
            .map_or_else(|| type_expr.to_owned(), |node| node.module.clone());
        self.world.create(bare, &module)
    }

    pub(crate) fn assign(&mut self, path: &Path, value: Value) {
        let Some(id) = self.bound.get(&path.root).cloned() else { return };
        let Some(field) = path.segments.first() else { return };
        self.world.set_field(&id, field, value);
    }

    /// A term as a value in this world.
    pub(crate) fn value_of(&self, term: &Term) -> Value {
        match term {
            Term::Literal(value) => value.clone(),
            Term::Set(items) => Value::Set(items.iter().map(|item| self.value_of(item)).collect()),
            Term::Path(path) => self.read(path),
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
            // Not somebody this journey cast, so it is a state the spec
            // declares: `available`, `open`, `active`. A name that is neither
            // reaches a comparison and comes back false, which is the same
            // answer as comparing against the wrong state and is what the
            // detail line is for.
            return if path.segments.is_empty() {
                Value::Enum(path.root.clone())
            } else {
                Value::Unknown
            };
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

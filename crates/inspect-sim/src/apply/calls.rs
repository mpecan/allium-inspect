//! Calls as postconditions: `Entity.created(…)` and `Trigger(…)`.
//!
//! A creation makes an instance and places it in the module that declares its
//! entity; an emission carries its arguments to whichever rules listen. Its own
//! file because both are about what a call hands on, and because `apply` is
//! long enough without them.

use std::collections::BTreeMap;

use allium_parser::ast::{CallArg, Expr};
use inspect_model::NodeKind;

use super::{Against, Application, Applied, Effect, entity_node};
use crate::{
    eval::{Evaluation, Unresolved, bare_name},
    value::Value,
};

impl Application<'_> {
    /// `Entity.created(...)` or `TriggerName(...)`.
    pub(super) fn call(&mut self, function: &Expr, args: &[CallArg]) -> Applied {
        // `Message.created(...)`: a member access whose field is `created`.
        // Or `memo/Note.created(...)`, an entity another module declares —
        // which, read only as a bare name, was neither a creation nor a
        // trigger, so the rule fired and nothing was made.
        if let Expr::MemberAccess { object, field, .. } = function
            && field.name == "created"
        {
            match object.as_ref() {
                Expr::Ident(ident) => return self.creation(&ident.name, None, args),
                Expr::QualifiedName(name) => {
                    return self.creation(&name.name, name.qualifier.as_deref(), args);
                }
                _ => {}
            }
        }

        // Otherwise a bare name: a trigger emission. Asked of the same function
        // ingestion reports an unbindable emission by, so the two agree on
        // what an emission is.
        match inspect_model::listening::emitted(function) {
            Some(trigger) => {
                let (arguments, positional) = self.emission_arguments(args);
                Applied::effect(Effect::Emitted {
                    trigger: trigger.to_owned(),
                    module: self.against.module.to_owned(),
                    arguments,
                    positional,
                })
            }
            None => self.unmodelled(function, "this call is neither a creation nor a trigger"),
        }
    }

    /// What an emission carries: named arguments by the name written, and
    /// positional ones in the order written.
    ///
    /// `MessageSent(message: message)` names its argument and binds the
    /// listener's `message`; `Announced(doc)` passes one by position, and each
    /// listener takes it under whatever its own `when` calls the first
    /// parameter. Naming a positional argument here — after whichever
    /// listener the graph happened to list first — handed it to that one and
    /// to no other listener that called it something else.
    fn emission_arguments(&self, args: &[CallArg]) -> (BTreeMap<String, Value>, Vec<Value>) {
        let mut named = BTreeMap::new();
        let mut positional = Vec::new();
        for argument in args {
            match argument {
                CallArg::Named(argument) => {
                    named.insert(argument.name.name.clone(), self.carried(&argument.value));
                }
                CallArg::Positional(value) => positional.push(self.carried(value)),
            }
        }
        (named, positional)
    }

    /// One argument of an emission, as the receiving rule will be handed it.
    ///
    /// `Finish(repo, cancel)` passes a word nothing binds, and evaluated as a
    /// name it is nobody — so `Finish` was handed nothing for `how`, and its
    /// `how = cancel` could never be decided. When the spec declares that word
    /// as a state, the state is what was meant: an emission is the spec's own
    /// act. A journey's act reads any unbound word as a state, and a word the
    /// spec does not declare then compares false; here, with no journey to
    /// have said it, only a declared one is.
    ///
    /// Only a declared state. A trigger's parameters carry no types, so there
    /// is no other side to say a word is a state, and an undeclared word may be
    /// something this simulator does not model. It stays undecided, and the
    /// rule that reads it says so.
    fn carried(&self, value: &Expr) -> Value {
        let evaluated = self.evaluate(value).value;
        match bare_name(value) {
            Some(word)
                if evaluated.is_unknown()
                    && !self.bindings.contains_key(word)
                    && self.against.spec.declared_states().contains(word) =>
            {
                Value::Enum(word.to_owned())
            }
            _ => evaluated,
        }
    }

    /// `let group = Group.created(…)`.
    ///
    /// The value is applied — it is usually a creation, and a creation is a
    /// postcondition rather than an expression — and then bound under the name
    /// the rule gave it, which is the whole point of writing one. `creation`
    /// already binds a new instance under its own lowercased type name; this
    /// binds it under the author's name as well, so
    ///
    /// ```text
    /// let group = Group.created(…)
    /// Membership.created(group: group, …)
    /// ```
    ///
    /// joins the two. Without the binding the second line's `group` is
    /// undecided and the membership is created pointing at nothing.
    pub(super) fn binding(&mut self, name: &str, value: &Expr) -> Applied {
        // Only a call is applied: a creation or an emission is a postcondition
        // in its own right. Anything else — `let pending = user.tasks where …`
        // inside a rule's `for` — is a value, and applying it would note it as
        // a postcondition the simulator does not model, which it is not.
        if !matches!(value, Expr::Call { .. }) {
            let evaluated = self.evaluate(value);
            self.bindings.insert(name.to_owned(), evaluated.value);
            return Applied::note(evaluated.unresolved);
        }
        let applied = self.apply(value);

        // What the value produced, if it produced an instance. A `let` over
        // anything else is bound to what it evaluates to.
        let bound = applied.effects.iter().rev().find_map(|effect| match effect {
            Effect::Created { id, .. } => Some(Value::Ref(id.clone())),
            _ => None,
        });

        match bound {
            Some(value) => {
                self.bindings.insert(name.to_owned(), value);
                applied
            }
            None => {
                let evaluated = self.evaluate(value);
                let mut applied = applied;
                applied.unresolved.extend(evaluated.unresolved);
                self.bindings.insert(name.to_owned(), evaluated.value);
                applied
            }
        }
    }

    /// `Entity.created(field: value, ...)`, or `alias/Entity.created(...)`.
    fn creation(&mut self, entity: &str, alias: Option<&str>, args: &[CallArg]) -> Applied {
        let (home, guessed) = self.declaring_module(entity, alias);
        let home = home.unwrap_or_else(|| self.against.module.to_owned());
        let id = self.world.create(entity, &home);
        let mut applied =
            Applied::effect(Effect::Created { id: id.clone(), entity: entity.to_owned() });
        if let Some(reason) = guessed {
            applied.unresolved.push(Unresolved { reason, expression: None, span: None });
        }

        for argument in args {
            let CallArg::Named(named) = argument else { continue };
            let field = named.name.name.as_str();
            let value_node = &named.value;

            let mut evaluated = self.evaluate(value_node);
            // `Loan.created(status: open)` — `open` is a state, and at creation
            // there is no previous value to say so. The entity's declared states
            // are what settle it, and they come from the spec rather than from
            // the world. Without this the new instance starts with an undecided
            // status, and every rule that later reads it is undecided too.
            if evaluated.value.is_unknown()
                && let Some(name) = bare_name(value_node)
                && let Some(state) = self.declared_state(&home, entity, field, name)
            {
                evaluated = Evaluation { value: state, unresolved: Vec::new() };
            }
            applied.unresolved.extend(evaluated.unresolved);
            self.world.set_field(&id, field, evaluated.value);
        }

        // The instance binds its own name, so the next clause can refer to it.
        self.bindings.insert(entity.to_lowercase(), Value::Ref(id));
        applied
    }

    /// The module that declares `entity`, so a created instance knows where it
    /// is from even when the rule creating it lives elsewhere.
    ///
    /// Where the creation's own spelling leads first: `memo/Note` through
    /// whatever this module imported `as memo`, a bare `Order` to this module
    /// when it declares one — two modules may each have an `Order`, and the
    /// first one in the graph is not this one's. Then by name alone, which is
    /// how an unqualified creation of another module's entity is placed. An
    /// alias that leads nowhere is placed the same way, and says so: the
    /// spec did not give the home this guesses.
    fn declaring_module(
        &self,
        entity: &str,
        alias: Option<&str>,
    ) -> (Option<String>, Option<String>) {
        let Against { spec, module, .. } = self.against;
        if let Some(home) = crate::creation::declared_in(spec, module, alias, entity) {
            return (Some(home), None);
        }
        let by_name = spec
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::Entity && node.name == entity)
            .map(|node| node.module.clone());
        let note = alias.map(|alias| {
            format!(
                "`{alias}/{entity}` leads to no module `{module}` imports that declares \
                 `{entity}`, so where the created `{entity}` belongs is a guess"
            )
        });
        (by_name, note)
    }

    /// `name` as a state of `entity.field`, when the spec declares it as one.
    ///
    /// Checked against the declared states rather than accepted on sight: a
    /// misspelled state should stay undecided and be reported, not become a
    /// state nothing in the lifecycle mentions.
    ///
    /// A field's states are written in one of two places, and both count.
    /// `status: open | returned` carries its own; `kind: ReceiptKind` carries
    /// none and names an enumeration that does. Reading only the first left
    /// `Receipt.created(kind: read)` storing an unknown — so the rule that
    /// asks `not exists Receipt{…, kind: read}` before creating one could
    /// never tell whether it already had, and a person reading the same
    /// message twice was undecidable from the second time on.
    pub(super) fn declared_state(
        &self,
        home: &str,
        entity: &str,
        field: &str,
        name: &str,
    ) -> Option<Value> {
        // The entity of the module the instance was filed in, when it is one:
        // another module's `Order` need not have the same fields.
        let spec = self.against.spec;
        let node = entity_node(spec, home, entity)?;
        let declared = node.detail.as_entity()?.field(field)?;
        let known = spec.states_of(declared).iter().any(|state| state == name);
        known.then(|| Value::Enum(name.to_owned()))
    }
}

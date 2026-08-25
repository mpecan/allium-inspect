//! Deciding whether a `then` or a `sees` line holds.
//!
//! Split from the walk itself because the two ask different questions. The walk
//! asks what happens next; this asks what is so afterwards, and it never
//! changes the world — every method here takes `&self`.

use std::collections::BTreeMap;

use allium_parser::ast::{CallArg, Expr, ForBinding};
use inspect_model::{Boundary, NodeKind};
use inspect_sim::{
    Truth, Value,
    eval::{Env, eval},
    value::EntityId,
};

use crate::{
    check::Verdict,
    journey::{Assertion, Comparison, Subject},
    run::{Outcome, Walker},
};

impl Walker<'_> {
    /// Evaluate one assertion against the world the last step left.
    pub(crate) fn assert(&self, assertion: &Assertion, line: usize, about: String) -> Outcome {
        let (truth, detail) = match assertion {
            Assertion::Compare { left, operator, right } => {
                let found = self.read(left);
                let wanted = self.value_of(right);
                (
                    compare(&found, *operator, &wanted),
                    Some(format!("{} is {}", left.as_written(), found.render())),
                )
            }
            Assertion::Within { needle, haystack } => {
                let wanted = self.value_of(needle);
                let inside = self.read(haystack);
                let truth = match &inside {
                    Value::Set(items) => Truth::any(items.iter().map(|item| item.equals(&wanted))),
                    _ => Truth::Unknown,
                };
                (truth, Some(format!("{} is {}", haystack.as_written(), inside.render())))
            }
            Assertion::Fires { rule, negated } => {
                let ran = self.fired.iter().any(|name| name == rule);
                // "did not run" and "could not be told whether it should" are
                // different answers, and only one of them is about the spec.
                if !ran && self.undecided.iter().any(|name| name == rule) {
                    (Truth::Unknown, Some(format!("`{rule}` could not be decided")))
                } else if !ran && self.waits_on_the_world(rule) {
                    // Nobody fires a state-condition rule; it becomes true or it
                    // does not. The simulator lists the ones that became true and
                    // says nothing about the rest, so a rule whose condition is
                    // *false* and one whose condition could not be *decided* look
                    // identical from here — and reporting the second as a flat no
                    // is the failure this whole design refuses.
                    (
                        Truth::Unknown,
                        Some(format!(
                            "`{rule}` never became true, and whether its condition is false or \
                             could not be decided is not visible from here"
                        )),
                    )
                } else {
                    (
                        Truth::from_bool(ran != *negated),
                        (!ran).then(|| format!("`{rule}` did not run")),
                    )
                }
            }
            Assertion::Exists { path, negated } => {
                let value = self.read(path);
                // An unknown is not an absence, and this is the one arm where
                // reading it as one goes wrong in *both* directions at once: a
                // path that ran out would make `does not exist` hold on a world
                // nothing described, and `exists` refuse — the spec saying no to
                // a question nobody put to it. Stipulation 1 names those two
                // failure modes; here they are the same line.
                //
                // An unbound bare name is a different thing and stays decidable:
                // `read` answers it as a state name rather than as unknown, so a
                // journey that never caught a reservation can still say so.
                if value.is_unknown() {
                    (Truth::Unknown, Some(format!("{} is unknown", path.as_written())))
                } else {
                    let found =
                        matches!(&value, Value::Ref(id) if self.world.instance(id).is_some());
                    (Truth::from_bool(found != *negated), None)
                }
            }
        };

        let verdict = match truth {
            Truth::True => Verdict::Specified,
            // A false assertion is the spec doing something other than what
            // somebody said it should, which is the same thing as a refusal
            // from the reader's side: this journey is not what this spec does.
            Truth::False => Verdict::Refused,
            Truth::Unknown => Verdict::Undecided,
        };
        Outcome { line, verdict, about, detail: if truth == Truth::True { None } else { detail } }
    }

    /// Whether a rule waits on the world rather than on somebody acting.
    fn waits_on_the_world(&self, rule: &str) -> bool {
        use inspect_model::graph::TriggerSource;
        self.spec
            .nodes_of(NodeKind::Rule)
            .find(|node| node.name == rule)
            .and_then(|node| node.detail.as_rule())
            .is_some_and(|detail| {
                matches!(detail.source, TriggerSource::State | TriggerSource::Temporal)
            })
    }

    /// Can this actor observe this value here?
    ///
    /// Two questions, and only the first is answerable today. Whether the
    /// boundary carries the field at all is a fact about the surface, and it
    /// decides a `cannot see` outright: not "no instance matched" but "this
    /// boundary does not carry it", which is the strongest form of the claim.
    ///
    /// Whether the surface's own filter admits *this* actor needs the
    /// `exposes` clause as an expression rather than as text, and that is not
    /// read yet. So once the field *is* carried, neither direction can be
    /// settled — including the negative one. That last part is the whole
    /// reason this reads the surface itself rather than the value: a field
    /// nothing has set used to make `cannot see` come back satisfied, so
    /// `ada cannot see ada.open_loan_count on MemberShelf` held against a
    /// surface that exposes it on the line above. A privacy claim that passes
    /// because nothing checked it is the worst answer this tool could give.
    pub(crate) fn observe(&self, sight: &Sight<'_>, about: String) -> Outcome {
        let Sight { actor, subject, surface, negated, line, .. } = *sight;
        let written = subject.as_written();
        let say = |verdict, detail: String| Outcome {
            line,
            verdict,
            about: about.clone(),
            detail: Some(detail),
        };

        let carried = crate::check::surface_named(self.spec, surface)
            .is_some_and(|detail| crate::check::exposes(detail, &written));
        if !carried {
            // Settled from the clause text, and settled in both directions. A
            // surface that carries nothing like the field shows it to nobody.
            return say(
                if negated { Verdict::Specified } else { Verdict::Unexposed },
                format!("`{surface}` exposes nothing like `{written}`"),
            );
        }

        match self.admits(sight) {
            Admission::Yes => say(
                if negated { Verdict::Refused } else { Verdict::Specified },
                if negated {
                    format!("`{surface}` does show `{written}` to {actor}")
                } else {
                    format!("`{surface}` shows `{written}` to {actor}")
                },
            ),
            // Carried by the surface and not by this actor's instance of it.
            // Somebody else's row, which is the whole point of a filter.
            Admission::No => say(
                if negated { Verdict::Specified } else { Verdict::Unexposed },
                format!("`{surface}` exposes `{written}`, and not to {actor}"),
            ),
            Admission::Undecided(why) => {
                say(Verdict::Undecided, format!("`{surface}` exposes `{written}` — {why}"))
            }
        }
    }
}

/// Whether a surface shows a particular thing to a particular person.
enum Admission {
    Yes,
    No,
    /// With the reason, which is the half a reader can act on.
    Undecided(String),
}

impl Walker<'_> {
    /// Whether this surface, scoped to this actor, shows this path.
    ///
    /// The surface says *what* it exposes; the question a journey asks is
    /// whether it exposes it **to them**. `for device in
    /// identity.listed_devices: device.label` shows a label — one of the labels
    /// on one identity's list — and answering "yes, labels are exposed" to
    /// somebody asking about a device that is not on it would be a privacy
    /// claim that passed because nothing checked it.
    fn admits(&self, sight: &Sight<'_>) -> Admission {
        let Sight { actor, subject, surface, .. } = *sight;
        let Some((id, module)) = crate::check::surface_id(self.spec, surface) else {
            return Admission::Undecided(format!("no surface called `{surface}`"));
        };
        let Some(boundary) = self.program.boundary(&id) else {
            return Admission::Undecided("its boundary was not read".to_owned());
        };
        // A clause with several entries is a `Block`; one with a single entry
        // is that entry. Reading only the first shape left a surface whose
        // whole boundary is one `for` block reporting that its clause was not
        // a list, which is a sentence about this parser rather than about the
        // specification.
        let items: Vec<&Expr> = match &boundary.exposes {
            Some(Expr::Block { items, .. }) => items.iter().collect(),
            Some(only) => vec![only],
            None => return Admission::Undecided("it exposes nothing".to_owned()),
        };

        // Who is looking, and which instance of the surface they are at.
        let Some(looking) = self.bound.get(actor) else {
            return Admission::Undecided(format!("`{actor}` is nobody in this journey"));
        };
        let mut context = match self.context_for(boundary, sight, looking) {
            Ok(bound) => bound,
            Err(why) => return Admission::Undecided(why),
        };

        // And whoever the surface faces, under the name it gave them.
        // `facing owner: Identity` and `exposes: announces_reads(owner)` are
        // one sentence: the clause refers to the person looking, and this is
        // who that is.
        if let Some(binding) =
            crate::check::surface_named(self.spec, surface).and_then(|s| s.actor_binding.clone())
        {
            context.entry(binding).or_insert_with(|| Value::Ref(looking.clone()));
        }

        // What is being looked at. A call is answered against the exposure's own
        // call; a path against the instance it starts from and the field it
        // ends on.
        let asking = match subject {
            Subject::Call { name, arguments } => {
                let wanted: Vec<Value> = arguments.iter().map(|term| self.value_of(term)).collect();
                Asked::Call { name, arguments: wanted }
            }
            Subject::Path(path) => {
                let Some(of) = self.bound.get(&path.root) else {
                    return Admission::Undecided(format!(
                        "`{}` is nobody in this journey",
                        path.root
                    ));
                };
                if path.segments.is_empty() {
                    return Admission::Undecided("nothing is being read".to_owned());
                }
                Asked::Path { of, segments: &path.segments }
            }
        };

        let mut undecided = None;
        for item in items {
            match self
                .exposed_by(item, &Asking { asked: &asking, context: &context, module: &module })
            {
                Admission::Yes => return Admission::Yes,
                Admission::Undecided(why) => undecided = undecided.or(Some(why)),
                Admission::No => {}
            }
        }

        undecided.map_or(Admission::No, Admission::Undecided)
    }

    /// The surface's `context`, bound to the instance this actor stands at.
    ///
    /// Two ways of knowing which one, and the journey's own word comes first.
    /// `bruno sees proposal.decision on GroupMembers in room` says which group
    /// he has open, and nothing else can: a person is in several, and picking
    /// for them would be this tool deciding a fact about somebody's afternoon.
    ///
    /// Failing that, the actor *is* the context when they are an instance of
    /// its type, which is the ordinary case and not a guess: `surface
    /// DeviceManagement` is scoped to an `Identity`, the journey says Ada is
    /// looking, and Ada is an Identity. Anything else is undecided rather than
    /// inferred, with the remedy in the reason.
    fn context_for(
        &self,
        boundary: &Boundary,
        sight: &Sight<'_>,
        looking: &EntityId,
    ) -> Result<BTreeMap<String, Value>, String> {
        let Some((name, entity)) = &boundary.context else {
            // No context: the surface is not scoped, and what it exposes it
            // exposes to whoever it faces. A journey that named one anyway has
            // already been told so by the checker.
            return Ok(BTreeMap::new());
        };

        let standing = match sight.context {
            Some(named) => self
                .bound
                .get(named)
                .ok_or_else(|| format!("`{named}` is nobody in this journey"))?,
            None => looking,
        };

        let Some(instance) = self.world.instance(standing) else {
            return Err("whoever is looking is not in this world".to_owned());
        };
        if &instance.entity != entity {
            return Err(match sight.context {
                Some(named) => {
                    format!("it is scoped to `{entity}`, and `{named}` is `{}`", instance.entity)
                }
                None => format!(
                    "it is scoped to `{entity}`, and `{}` is `{}` — say which one with \
                     `… on {} in <the {entity}>`",
                    sight.actor, instance.entity, sight.surface
                ),
            });
        }

        Ok(BTreeMap::from([(name.clone(), Value::Ref(standing.clone()))]))
    }

    /// Whether one item of an `exposes` block shows what is being asked about.
    fn exposed_by(&self, item: &Expr, asking: &Asking<'_>) -> Admission {
        let Asking { asked, context, module } = *asking;

        // A call the surface exposes — `announces_reads(owner)` — matched
        // against the call being asked about. The name has to be the same and
        // the arguments have to be the same *things*: the clause writes the
        // surface's own binding for the actor and the journey writes whoever
        // that is, which is one call about one person written two ways.
        if let Expr::Call { function, args, .. } = item {
            let Asked::Call { name, arguments } = asked else { return Admission::No };
            let Expr::Ident(called) = function.as_ref() else { return Admission::No };
            if &called.name != name || args.len() != arguments.len() {
                return Admission::No;
            }
            for (argument, wanted) in args.iter().zip(arguments) {
                let CallArg::Positional(value) = argument else { return Admission::No };
                match self.evaluate(value, context, module) {
                    Ok((held, _)) if &held == wanted => {}
                    Ok(_) => return Admission::No,
                    Err(why) => return Admission::Undecided(why),
                }
            }
            return Admission::Yes;
        }

        let Asked::Path { of, segments } = asked else { return Admission::No };
        match item {
            // `identity.name` — a field of the context itself — or
            // `SpecSet.module_count`, which is the same clause written over a
            // type rather than over a binding and means every instance of it.
            Expr::MemberAccess { .. } => self.walks_to(item, (*of, segments), context, module),
            // `for device in identity.listed_devices: device.label` — a walk
            // that starts at one of a collection.
            Expr::For { binding, collection, filter, body, .. } => {
                let ForBinding::Single(name) = binding else {
                    return Admission::Undecided(
                        "its iteration binds more than one name".to_owned(),
                    );
                };
                let (members, unsettled) = match self.evaluate(collection, context, module) {
                    Ok((Value::Set(items), unsettled)) => (items, unsettled),
                    Ok((other, _)) => {
                        return Admission::Undecided(format!(
                            "it ranges over {}, which has no elements",
                            other.described()
                        ));
                    }
                    Err(why) => return Admission::Undecided(why),
                };
                self.shown_to_a_member(
                    &Iteration {
                        binding: &name.name,
                        members: &members,
                        filter: filter.as_deref(),
                        body,
                    },
                    (*of, segments),
                    asking,
                )
                // A collection that dropped what it could not decide may have
                // dropped the very element this walk starts from, and "no"
                // about a set nothing finished reading is the privacy claim
                // pointed the other way.
                .or_undecided(unsettled)
            }
            _ => Admission::No,
        }
    }

    /// Whether an `exposes` item is the very walk being asked about.
    ///
    /// `shot.size_bytes` and `m.attachment.size_bytes` are the same walk
    /// written from two places: the journey names the attachment it caught,
    /// the surface reaches it through the message it hangs off. So the item is
    /// split at every point, and the question at each is whether the part
    /// *before* the split is the thing being asked about and the part after it
    /// is the fields being read.
    ///
    /// Matching only the last field was the old rule, and it had the two
    /// failures a rule this loose always has: `m.attachment.size_bytes` did not
    /// match `shot.size_bytes` at all — the surface exposes it and the tool
    /// reported a spec gap — while a clause reading some *other* thing's
    /// `status` would have matched anything ending in `.status`.
    fn walks_to(
        &self,
        item: &Expr,
        (of, segments): (&EntityId, &[String]),
        context: &BTreeMap<String, Value>,
        module: &str,
    ) -> Admission {
        // A `for` body is usually several lines, and any of them may be the
        // one being asked about.
        if let Expr::Block { items, .. } = item {
            let mut undecided = None;
            for item in items {
                match self.walks_to(item, (of, segments), context, module) {
                    Admission::Yes => return Admission::Yes,
                    Admission::Undecided(why) => undecided = undecided.or(Some(why)),
                    Admission::No => {}
                }
            }
            return undecided.map_or(Admission::No, Admission::Undecided);
        }

        let (root, fields) = walked(item);
        let Some(prefix) = split_before(root, &fields, segments) else { return Admission::No };
        match self.evaluate(prefix, context, module) {
            Ok((Value::Ref(id), _)) => Admission::from(&id == of),
            Ok((Value::Set(items), unsettled)) => holds(&items, of, unsettled),
            Ok(_) => Admission::No,
            Err(why) => Admission::Undecided(why),
        }
    }

    /// Whether an iteration reaches what is being asked about, and admits it.
    ///
    /// Every element in turn rather than the asked-about thing alone, because
    /// the walk may pass *through* an element on its way: `for m in
    /// group.messages: m.attachment.name` shows the name of an attachment,
    /// and the attachment is not in `group.messages`.
    fn shown_to_a_member(
        &self,
        over: &Iteration<'_>,
        (of, segments): (&EntityId, &[String]),
        asking: &Asking<'_>,
    ) -> Admission {
        let Asking { context, module, .. } = *asking;
        let mut undecided = None;
        for member in over.members {
            let Value::Ref(id) = member else { continue };
            let mut scope = context.clone();
            scope.insert(over.binding.to_owned(), member.clone());

            match self.walks_to(over.body, (of, segments), &scope, module) {
                Admission::No => continue,
                Admission::Undecided(why) => {
                    undecided = undecided.or(Some(why));
                    continue;
                }
                Admission::Yes => {}
            }

            // Reached from this element, and its `where` is a second gate.
            match self.admits_element(over.filter, id, scope, module) {
                Admission::Yes => return Admission::Yes,
                Admission::Undecided(why) => undecided = undecided.or(Some(why)),
                Admission::No => {}
            }
        }
        undecided.map_or(Admission::No, Admission::Undecided)
    }

    /// Whether an iteration's `where` admits one element.
    fn admits_element(
        &self,
        filter: Option<&Expr>,
        element: &EntityId,
        mut scope: BTreeMap<String, Value>,
        module: &str,
    ) -> Admission {
        let Some(filter) = filter else { return Admission::Yes };

        // The element's own fields are in scope bare, which is how `where
        // status = pending` reads — it is the element's status, not anybody
        // else's.
        if let Some(instance) = self.world.instance(element) {
            for (field, value) in &instance.fields {
                scope.insert(field.clone(), value.clone());
            }
        }

        match self.evaluate(filter, &scope, module) {
            Ok((Value::Bool(true), _)) => Admission::Yes,
            Ok((Value::Bool(false), _)) => Admission::No,
            Ok(_) => Admission::Undecided("its filter did not come back true or false".to_owned()),
            Err(why) => Admission::Undecided(why),
        }
    }

    /// Evaluate one expression of an `exposes` clause.
    ///
    /// Returns what was left unsettled along with the value, and the caller has
    /// to care. A filtered collection keeps only its definite members and
    /// *notes* the ones nothing could decide, so a subject that is missing from
    /// the result may have been dropped for being undecided rather than for not
    /// belonging — and answering "this surface does not show you that" on those
    /// grounds is a privacy claim nothing checked, in the other direction.
    fn evaluate(
        &self,
        expr: &Expr,
        bindings: &BTreeMap<String, Value>,
        module: &str,
    ) -> Result<(Value, Option<String>), String> {
        let source = self.sources.get(module).map_or("", String::as_str);
        let mut env = Env::new(&self.world, module, source).deriving(self.program.derivations());
        for (name, value) in bindings {
            env = env.bind(name.clone(), value.clone());
        }

        let evaluated = eval(expr, &env);
        let unsettled = evaluated.unresolved.first().map(|note| note.reason.clone());
        if evaluated.value.is_unknown() {
            return Err(unsettled.unwrap_or_else(|| "it could not be settled".to_owned()));
        }
        Ok((evaluated.value, unsettled))
    }
}

/// What one `exposes` item is being asked about.
///
/// The four travel together through every branch and none of them changes on
/// the way, which is what a struct is for — and six positional arguments of
/// four shapes is a call nobody can read at the site.
struct Asking<'a> {
    asked: &'a Asked<'a>,
    /// The surface's context, bound to the instance this actor stands at.
    context: &'a BTreeMap<String, Value>,
    /// The module the surface is declared in, for `config` and for the source.
    module: &'a str,
}

/// What a `sees` line is asking about, resolved.
enum Asked<'a> {
    /// `tablet.label` — the thing it starts from, and the walk off it.
    ///
    /// The whole walk rather than its last field: `shot.size_bytes` and
    /// `note.author.display_name` end in a field each and are not the same
    /// question, and a surface that shows one need not show the other.
    Path { of: &'a EntityId, segments: &'a [String] },
    /// `announces_reads(ada)` — a call the surface may expose.
    Call { name: &'a str, arguments: Vec<Value> },
}

/// One `for … in … where …:` of an `exposes` clause, with its members read.
struct Iteration<'a> {
    binding: &'a str,
    members: &'a [Value],
    filter: Option<&'a Expr>,
    body: &'a Expr,
}

/// Whether `subject` is among `items`, given what could not be settled.
///
/// Absence only means *no* when everything was settled. A filtered collection
/// drops what it could not decide, so a subject missing from one that left
/// notes behind might have belonged — and saying no there is the same mistake
/// as saying yes, pointed the other way.
fn holds(items: &[Value], subject: &EntityId, unsettled: Option<String>) -> Admission {
    if items.iter().any(|item| matches!(item, Value::Ref(id) if id == subject)) {
        return Admission::Yes;
    }
    unsettled.map_or(Admission::No, Admission::Undecided)
}

impl From<bool> for Admission {
    fn from(yes: bool) -> Self {
        if yes { Admission::Yes } else { Admission::No }
    }
}

impl Admission {
    /// Downgrade a *no* to undecided when something was left unsettled.
    ///
    /// A `yes` stands: it was reached, whatever else could not be. A `no`
    /// reached over a set that dropped what it could not decide is a claim
    /// nothing checked.
    fn or_undecided(self, unsettled: Option<String>) -> Self {
        match (self, unsettled) {
            (Admission::No, Some(why)) => Admission::Undecided(why),
            (settled, _) => settled,
        }
    }
}

/// A member-access chain, split into what it starts from and the fields walked.
///
/// `m.attachment.size_bytes` gives `m` and, in order, `attachment` paired with
/// `m.attachment`, then `size_bytes` paired with the whole. Each field carries
/// the sub-expression that *ends* at it, so a prefix can be evaluated without
/// rebuilding one.
fn walked(expr: &Expr) -> (&Expr, Vec<(&str, &Expr)>) {
    let mut fields: Vec<(&str, &Expr)> = Vec::new();
    let mut here = expr;
    while let Expr::MemberAccess { object, field, .. } = here {
        fields.push((field.name.as_str(), here));
        here = object;
    }
    fields.reverse();
    (here, fields)
}

/// The part of a chain that would have to be the thing being asked about.
///
/// `m.attachment.size_bytes` asked about as `<an attachment>.size_bytes` splits
/// after `attachment`, and what comes back is `m.attachment` — evaluate that,
/// and if it is the attachment in question the surface shows this.
///
/// `None` when the chain does not end in the fields being read, which is the
/// ordinary answer: most items of an `exposes` block are about something else.
fn split_before<'a>(
    root: &'a Expr,
    fields: &[(&str, &'a Expr)],
    wanted: &[String],
) -> Option<&'a Expr> {
    let at = fields.len().checked_sub(wanted.len())?;
    if !fields[at..].iter().map(|(name, _)| *name).eq(wanted.iter().map(String::as_str)) {
        return None;
    }
    Some(if at == 0 { root } else { fields[at - 1].1 })
}

/// One `sees` or `cannot see` line, as the walker asks it.
///
/// Grouped rather than passed loose because the four travel together and the
/// checker already asks the same question under the same shape.
pub(crate) struct Sight<'a> {
    /// Who is looking. The surface's `context` is resolved from them when the
    /// journey does not say which one they are at.
    pub(crate) actor: &'a str,
    /// What they are looking at: a path, or a call the surface exposes.
    pub(crate) subject: &'a Subject,
    pub(crate) surface: &'a str,
    /// Which instance of the surface's `context`, when the journey says.
    pub(crate) context: Option<&'a str>,
    pub(crate) negated: bool,
    pub(crate) line: usize,
}

/// Compare two values the way the assertion asked.
fn compare(found: &Value, operator: Comparison, wanted: &Value) -> Truth {
    use std::ops::Not;
    match operator {
        Comparison::Equal => found.equals(wanted),
        Comparison::NotEqual => found.equals(wanted).not(),
        _ => match found.compare(wanted) {
            Some(ordering) => Truth::from_bool(match operator {
                Comparison::Less => ordering.is_lt(),
                Comparison::LessOrEqual => ordering.is_le(),
                Comparison::Greater => ordering.is_gt(),
                Comparison::GreaterOrEqual => ordering.is_ge(),
                Comparison::Equal | Comparison::NotEqual => unreachable!("handled above"),
            }),
            // Two kinds that do not order is a question with no answer rather
            // than a question answered no.
            None => Truth::Unknown,
        },
    }
}

#[cfg(test)]
mod tests {
    use allium_parser::{Span, ast::Ident};

    use super::*;

    const NOWHERE: Span = Span { start: 0, end: 0 };

    fn ident(name: &str) -> Expr {
        Expr::Ident(Ident { span: NOWHERE, name: name.to_owned() })
    }

    fn access(object: &str, field: &str) -> Expr {
        access_of(ident(object), field)
    }

    fn access_of(object: Expr, field: &str) -> Expr {
        Expr::MemberAccess {
            span: NOWHERE,
            object: Box::new(object),
            field: Ident { span: NOWHERE, name: field.to_owned() },
        }
    }

    fn is_named(expr: &Expr, wanted: &str) -> bool {
        matches!(expr, Expr::Ident(ident) if ident.name == wanted)
    }

    fn segments(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    /// The chain, and each field paired with the walk that ends at it.
    #[test]
    fn a_chain_splits_into_what_it_starts_from_and_the_fields_walked() {
        let deep = access_of(access("m", "attachment"), "size_bytes");
        let (root, fields) = walked(&deep);
        assert!(is_named(root, "m"));
        assert_eq!(
            fields.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ["attachment", "size_bytes"]
        );
    }

    #[test]
    fn something_that_is_not_a_chain_is_all_root_and_no_fields() {
        let bare = ident("device");
        let (root, fields) = walked(&bare);
        assert!(is_named(root, "device"));
        assert!(fields.is_empty());
    }

    /// The whole point of the split. `m.attachment.size_bytes` is asked about
    /// as `<the attachment>.size_bytes`, so it splits after `attachment` and
    /// what comes back is the walk to the attachment itself.
    #[test]
    fn a_chain_splits_where_the_fields_being_read_begin() {
        let deep = access_of(access("m", "attachment"), "size_bytes");
        let (root, fields) = walked(&deep);

        let prefix = split_before(root, &fields, &segments(&["size_bytes"])).expect("splits");
        let (prefix_root, prefix_fields) = walked(prefix);
        assert!(is_named(prefix_root, "m"));
        assert_eq!(prefix_fields.iter().map(|(name, _)| *name).collect::<Vec<_>>(), ["attachment"]);
    }

    /// Asked about as the whole walk, the split is before everything and what
    /// has to be the subject is what the chain starts from.
    #[test]
    fn a_chain_asked_about_whole_splits_at_its_root() {
        let deep = access_of(access("intent", "targets"), "count");
        let (root, fields) = walked(&deep);
        let prefix = split_before(root, &fields, &segments(&["targets", "count"])).expect("splits");
        assert!(is_named(prefix, "intent"));
    }

    /// The failure the old rule had in the other direction: matching only the
    /// last field made every clause ending in `.status` an answer to every
    /// question about somebody's status.
    #[test]
    fn a_chain_that_ends_in_the_wrong_fields_does_not_split() {
        let deep = access_of(access("m", "attachment"), "size_bytes");
        let (root, fields) = walked(&deep);
        assert!(split_before(root, &fields, &segments(&["status"])).is_none());
        assert!(split_before(root, &fields, &segments(&["author", "size_bytes"])).is_none());
    }

    /// A walk longer than the clause cannot be inside it, and asking for one
    /// must not panic on the arithmetic.
    #[test]
    fn a_walk_longer_than_the_clause_does_not_split() {
        let shallow = access("group", "name");
        let (root, fields) = walked(&shallow);
        assert!(split_before(root, &fields, &segments(&["a", "long", "way", "down"])).is_none());
    }

    /// A `no` over a set that dropped what it could not decide is a claim
    /// nothing checked; a `yes` stands whatever else was unsettled.
    #[test]
    fn an_unsettled_set_downgrades_a_no_and_leaves_a_yes_alone() {
        let why = || Some("something was unsettled".to_owned());
        assert!(matches!(Admission::No.or_undecided(why()), Admission::Undecided(_)));
        assert!(matches!(Admission::No.or_undecided(None), Admission::No));
        assert!(matches!(Admission::Yes.or_undecided(why()), Admission::Yes));
    }
}

//! Deciding whether a `then` or a `sees` line holds.
//!
//! Split from the walk itself because the two ask different questions. The walk
//! asks what happens next; this asks what is so afterwards, and it never
//! changes the world — every method here takes `&self`.

use std::collections::BTreeMap;

use allium_parser::ast::{CallArg, Expr, ForBinding};
use inspect_sim::{
    Truth, Value,
    eval::{Env, eval},
    value::{EntityId, Instance},
};

use crate::{
    check::Verdict,
    journey::{Assertion, Comparison, Subject},
    run::{Outcome, Walker},
    standing::Unstood,
};

impl Walker<'_> {
    /// Evaluate one assertion against the world the last step left.
    pub(crate) fn assert(&self, assertion: &Assertion, line: usize, about: String) -> Outcome {
        // A parameter the spec does not declare, or declares twice, is read the
        // way it would be written: with the resolver's answer and its reason.
        // Reading it as a bare unknown said only "is unknown" about a switch
        // the spec does not have, where a `given` of it said which.
        if let Some(crate::config::Unwritten { verdict, reason }) =
            self.unreadable_config(assertion)
        {
            return Outcome { line, verdict, about, detail: Some(reason) };
        }
        let (truth, detail) = match assertion {
            Assertion::Compare { left, operator, right } => {
                let (found, why) = self.read_explained(left);
                let wanted = self.value_of(right);
                let read = format!("{} is {}", left.as_written(), found.render());
                (
                    compare(&found, *operator, &wanted),
                    Some(match why {
                        Some(why) => format!("{read} — {why}"),
                        None => read,
                    }),
                )
            }
            Assertion::Within { needle, haystack } => {
                let wanted = self.value_of(needle);
                let (inside, why) = self.read_explained(haystack);
                let truth = match &inside {
                    Value::Set(items) => Truth::any(items.iter().map(|item| item.equals(&wanted))),
                    _ => Truth::Unknown,
                };
                let read = format!("{} is {}", haystack.as_written(), inside.render());
                let detail = match why {
                    Some(why) => format!("{read} — {why}"),
                    None => read,
                };
                (truth, Some(detail))
            }
            Assertion::Fires { rule, negated } => self.fires(rule, *negated),
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

        // Read, in the step that moved it, off a world that step says is not
        // one the act leads to —
        // something it set off could not be decided, fired and was not applied
        // in full, or never stopped — the answer is about that world and not
        // the spec's, in either direction: `then b.fired = false` holding when
        // the member that would have set it was skipped is the simulator
        // guessing. `fires` has already weighed this, rule by rule.
        let (truth, detail) = match (truth, assertion) {
            (Truth::True | Truth::False, Assertion::Compare { .. })
            | (Truth::True | Truth::False, Assertion::Within { .. })
            | (Truth::True | Truth::False, Assertion::Exists { .. }) => match self.unsettled() {
                Some(why) if self.moved => {
                    // What was read, so an existence line has a subject.
                    let read = detail.or_else(|| match assertion {
                        Assertion::Exists { path, negated } => {
                            let found = (truth == Truth::True) != *negated;
                            let verb = if found { "exists" } else { "does not exist" };
                            Some(format!("{} {verb}", path.as_written()))
                        }
                        _ => None,
                    });
                    let read = read.map_or_else(String::new, |it| format!("{it}, "));
                    (Truth::Unknown, Some(format!("{read}but this world is not settled — {why}")))
                }
                _ => (truth, detail),
            },
            _ => (truth, detail),
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

    /// Can this actor observe this value here?
    ///
    /// Two questions. Whether the
    /// boundary carries the field at all is a fact about the surface, and it
    /// decides a `cannot see` outright: not "no instance matched" but "this
    /// boundary does not carry it", which is the strongest form of the claim.
    ///
    /// Whether it is carried *to this actor* is `admits`, which walks the
    /// clause as an expression: its contexts, `for … where` filters and `when`
    /// guards read against the world. Whatever it cannot settle stays
    /// undecided in both directions, the negative one included — a field
    /// nothing has set once made `ada cannot see ada.open_loan_count on
    /// MemberShelf` hold against a surface that exposes it on the line above,
    /// and a privacy claim that passes because nothing checked it is the worst
    /// answer this tool could give.
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
            Ok(Admission::Yes) => say(
                if negated { Verdict::Refused } else { Verdict::Specified },
                if negated {
                    format!("`{surface}` does show `{written}` to {actor}")
                } else {
                    format!("`{surface}` shows `{written}` to {actor}")
                },
            ),
            // Nothing this surface shows reaches it from where this actor
            // stands: somebody else's row, which is the whole point of a
            // filter, or a walk off a noun this boundary does not carry.
            //
            // Which of the two is not said, because it is not known here. The
            // clause text got as far as "something ending that way is on this
            // surface", and `MyLoans` answering "exposes `loan.shelfmark`"
            // about a boundary that exposes `loan.copy.shelfmark` would be a
            // false sentence about the specification — the thing this tool
            // exists not to produce — put in a reader's way while they look
            // for a filter that is not the problem.
            Ok(Admission::No) => say(
                if negated { Verdict::Specified } else { Verdict::Unexposed },
                format!("nothing `{surface}` shows to {actor} is `{written}`"),
            ),
            // Hidden by a guard, which is said: the reason is in the world.
            Ok(Admission::Hidden(why)) => say(
                if negated { Verdict::Specified } else { Verdict::Unexposed },
                format!("`{surface}` shows `{written}` {why}"),
            ),
            // Not open here at all, so nothing on it is shown to anybody.
            Err(why) => say(if negated { Verdict::Specified } else { Verdict::Unexposed }, why),
            Ok(Admission::Undecided(why)) => {
                say(Verdict::Undecided, format!("`{surface}` exposes `{written}` — {why}"))
            }
        }
    }
}

/// Whether a surface shows a particular thing to a particular person.
pub(crate) enum Admission {
    Yes,
    No,
    /// Reached, and hidden by a `when` guard — with which one, because that
    /// is a fact about the world a reader set up, not about the surface.
    Hidden(String),
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
    ///
    /// An error when the surface is not open here at all — a context's `where`
    /// refused the instance standing in it — with which filter said so.
    fn admits(&self, sight: &Sight<'_>) -> Result<Admission, String> {
        let Sight { actor, subject, surface, .. } = *sight;
        let Some((id, module)) = crate::check::surface_id(self.spec, surface) else {
            return Ok(Admission::Undecided(format!("no surface called `{surface}`")));
        };
        let Some(boundary) = self.program.boundary(&id) else {
            return Ok(Admission::Undecided("its boundary was not read".to_owned()));
        };
        // A clause with several entries is a `Block`; one with a single entry
        // is that entry. Reading only the first shape left a surface whose
        // whole boundary is one `for` block reporting that its clause was not
        // a list, which is a sentence about this parser rather than about the
        // specification.
        let items: Vec<&Expr> = match &boundary.exposes {
            Some(Expr::Block { items, .. }) => items.iter().collect(),
            Some(only) => vec![only],
            None => return Ok(Admission::Undecided("it exposes nothing".to_owned())),
        };

        // Who is looking, and everything the clause may read from where they
        // stand: contexts, whoever it faces, and its `let`s.
        let Some(looking) = self.bound.get(actor) else {
            return Ok(Admission::Undecided(format!("`{actor}` is nobody in this journey")));
        };
        let standing = match self.stand(boundary, sight, looking, &module) {
            Ok(standing) => standing,
            Err(Unstood::Closed(why)) => return Err(why),
            Err(Unstood::Undecided(why)) => return Ok(Admission::Undecided(why)),
        };
        let context = &standing.bindings;

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
                    return Ok(Admission::Undecided(format!(
                        "`{}` is nobody in this journey",
                        path.root
                    )));
                };
                if path.segments.is_empty() {
                    return Ok(Admission::Undecided("nothing is being read".to_owned()));
                }
                Asked::Path { of, segments: &path.segments }
            }
        };

        let mut found = Admission::No;
        for item in items {
            found = found
                .or(self.exposed_by(item, &Asking { asked: &asking, context, module: &module }));
            if matches!(found, Admission::Yes) {
                break;
            }
        }

        let answer = match (found, standing.gated.clone()) {
            // Shown, by a surface nobody knows is open here.
            (Admission::Yes, Some(why)) => Admission::Undecided(why),
            (Admission::Yes, None) => Admission::Yes,
            (unshown, _) => unshown.or_undecided(standing.unsettled.clone()),
        };
        Ok(match answer {
            Admission::Undecided(why) => Admission::Undecided(standing.explain(why)),
            settled => settled,
        })
    }

    /// Whether one item of an `exposes` block shows what is being asked about.
    fn exposed_by(&self, item: &Expr, asking: &Asking<'_>) -> Admission {
        let Asking { asked, context, module } = *asking;

        // `doc.secret when doc.is_private = false` — shown while the guard
        // holds, which is a question about the world, not about the clause.
        if let Expr::WhenGuard { action, condition, .. } = item {
            let shown = self.exposed_by(action, asking);
            return self.guarded((action, condition), shown, context, module);
        }

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
            let mut found = Admission::No;
            for item in items {
                found = found.or(self.walks_to(item, (of, segments), context, module));
                if matches!(found, Admission::Yes) {
                    break;
                }
            }
            return found;
        }
        // A guard in a `for` body is about the element being visited, which
        // is already bound in `context` by the time this reads it.
        if let Expr::WhenGuard { action, condition, .. } = item {
            let shown = self.walks_to(action, (of, segments), context, module);
            return self.guarded((action, condition), shown, context, module);
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
        let mut found = Admission::No;
        for member in over.members {
            let Value::Ref(id) = member else { continue };
            let mut scope = context.clone();
            scope.insert(over.binding.to_owned(), member.clone());

            let reached = self.walks_to(over.body, (of, segments), &scope, module);
            if !matches!(reached, Admission::Yes) {
                found = found.or(reached);
                continue;
            }
            // Reached from this element, and its `where` is a second gate.
            found = found.or(self.admits_element(over.filter, id, scope, module));
            if matches!(found, Admission::Yes) {
                break;
            }
        }
        found
    }

    /// Whether an iteration's `where` admits one element.
    fn admits_element(
        &self,
        filter: Option<&Expr>,
        element: &EntityId,
        scope: BTreeMap<String, Value>,
        module: &str,
    ) -> Admission {
        let Some(filter) = filter else { return Admission::Yes };
        match self.filter_admits(filter, element, scope, module) {
            Ok(admitted) => Admission::from(admitted),
            Err(why) => Admission::Undecided(why),
        }
    }

    /// Whether `filter` admits `element`, or why that could not be settled.
    ///
    /// The one reading of a `where` about an instance, for an iteration's
    /// filter and a context's alike. The element's own fields are in scope
    /// bare, which is how `where status = pending` reads — it is the element's
    /// status, not anybody else's — and `this` is the element when nothing else
    /// holds it, so `where memberships.count = 1` reaches what it computes. The
    /// same rule the evaluator's own `where` follows.
    pub(crate) fn filter_admits(
        &self,
        filter: &Expr,
        element: &EntityId,
        mut scope: BTreeMap<String, Value>,
        module: &str,
    ) -> Result<bool, String> {
        scope.entry("this".to_owned()).or_insert_with(|| Value::Ref(element.clone()));
        // The element's fields bare, each undecided one saying why.
        let stored = self.world.instance(element);
        match self.evaluate_with(filter, &scope, module, stored) {
            // A decided answer is trusted, notes and all. Whether a member a
            // collection dropped could overturn it is the evaluator's call —
            // `count`, `in`, `exists`, `all` and `any` over one come back
            // undecided themselves — and a note left by an `or` its left side
            // settled is about a part that did not matter.
            Ok((Value::Bool(admitted), _)) => Ok(admitted),
            Ok(_) => Err("its filter did not come back true or false".to_owned()),
            Err(why) => Err(why),
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
    pub(crate) fn evaluate(
        &self,
        expr: &Expr,
        bindings: &BTreeMap<String, Value>,
        module: &str,
    ) -> Result<(Value, Option<String>), String> {
        self.evaluate_with(expr, bindings, module, None)
    }

    /// [`Self::evaluate`], with `stored`'s fields in scope bare as well.
    fn evaluate_with(
        &self,
        expr: &Expr,
        bindings: &BTreeMap<String, Value>,
        module: &str,
        stored: Option<&Instance>,
    ) -> Result<(Value, Option<String>), String> {
        let source = self.sources.get(module).map_or("", String::as_str);
        let mut env = Env::new(&self.world, module, source)
            .deriving(self.program.derivations())
            .quoting(self.sources);
        for (name, value) in bindings {
            env = env.bind(name.clone(), value.clone());
        }
        if let Some(instance) = stored {
            env.bind_stored(instance);
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
            (Admission::No | Admission::Hidden(_), Some(why)) => Admission::Undecided(why),
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
    /// Which instances of the surface's contexts, when the journey says.
    pub(crate) contexts: &'a [String],
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

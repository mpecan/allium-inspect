//! A rule's parameters, bound from the firing that woke it.
//!
//! Allium declares a trigger nowhere but in the rules that listen for it:
//! `when: Published(doc)` *is* the declaration, for that rule. So a firing's
//! arguments are bound by each listener on its own terms —
//!
//! - **by position** when they were passed by position. `Announced(doc)` hands
//!   its first argument to `when: Announced(thing)` as `thing` and to `when:
//!   Announced(doc)` as `doc`; naming it once for every listener gave it to
//!   whichever the graph listed first and to nobody else;
//! - **by name** when they were named. `Published(item: doc)` binds `item`,
//!   and only `item`. A listener that calls its parameter `doc` is not handed
//!   it by being the only parameter there is, because which of a listener's
//!   names an argument it was not given was *meant* for is a guess, and the
//!   simulator does not make one;
//! - **by neither** when it was handed a value both ways, `X(d, doc: e)`
//!   against `when: X(doc, other)`: which of the two was meant is the same
//!   kind of guess.
//!
//! And `?` is `null` only when nothing could have been meant for it: a firing
//! that named an argument this rule has no parameter for may have misnamed
//! the optional one.
//!
//! A required parameter nothing bound is the rule run about nothing. It used to
//! run anyway: every clause reading the name came back undecided or quietly did
//! nothing, the rule reported itself fired, and a journey asking about what it
//! should have written was told *no* — the specification refusing, when
//! nothing had been decided at all. Now its `when` is undecided, saying which
//! name it waited for and what the firing carried instead.

use std::collections::BTreeMap;

use inspect_model::graph::RuleClause;

use crate::{eval::Unresolved, value::Value, world::Event};

/// The bindings `event` gives a rule whose `when` names `parameters`, in order
/// and with which are optional.
///
/// Every named argument under its own name, then each positional one under the
/// parameter in its place. A parameter handed a value both ways is left
/// unbound: `X(d, doc: e)` against `when: X(doc, other)` says two things about
/// `doc`, and which one the author meant is not the simulator's to pick.
///
/// An optional parameter still unbound is `null`, which is what `?` means —
/// unless the firing named an argument this rule has no parameter for. Then
/// `reason` against `cause?` may be the same argument misnamed, and `null`
/// would be a guess that it was not; it stays unbound, and [`unbound`] says
/// why.
pub(crate) fn bind(event: &Event, parameters: &[(String, bool)]) -> BTreeMap<String, Value> {
    let mut bindings = event.arguments.clone();
    for ((name, _), value) in parameters.iter().zip(&event.positional) {
        if name.is_empty() {
            continue;
        }
        if event.arguments.contains_key(name) {
            bindings.remove(name);
        } else {
            bindings.insert(name.clone(), value.clone());
        }
    }
    if unmatched(event, parameters).is_empty() {
        for (name, _) in parameters.iter().filter(|(name, optional)| *optional && !name.is_empty())
        {
            if doubly_handed(event, parameters, name).is_none() {
                bindings.entry(name.clone()).or_insert(Value::Null);
            }
        }
    }
    bindings
}

/// Why each parameter is unbound, anchored to the `when` clause that names it
/// — or nothing, when every one of them is bound.
///
/// Required parameters, and optional ones [`bind`] declined to make `null`.
pub(crate) fn unbound(
    event: &Event,
    parameters: &[(String, bool)],
    bindings: &BTreeMap<String, Value>,
    when: Option<&RuleClause>,
) -> Vec<Unresolved> {
    parameters
        .iter()
        .filter(|(name, _)| !name.is_empty() && !bindings.contains_key(name))
        .map(|(name, _)| Unresolved {
            reason: reason(event, parameters, name),
            expression: when.map(|clause| clause.text.clone()),
            span: when.and_then(|clause| clause.span),
        })
        .collect()
}

/// The named arguments `event` carries that no parameter of this rule has.
fn unmatched(event: &Event, parameters: &[(String, bool)]) -> Vec<String> {
    event
        .arguments
        .keys()
        .filter(|carried| !parameters.iter().any(|(parameter, _)| parameter == *carried))
        .map(|carried| format!("`{carried}`"))
        .collect()
}

/// Where `name` was handed a value by position as well as by name, if it was.
fn doubly_handed(event: &Event, parameters: &[(String, bool)], name: &str) -> Option<usize> {
    if !event.arguments.contains_key(name) {
        return None;
    }
    parameters.iter().take(event.positional.len()).position(|(parameter, _)| parameter == name)
}

/// `1st`, `2nd`, `3rd`, `4th`… for a zero-based place.
fn ordinal(at: usize) -> String {
    let n = at + 1;
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// What a firing carried instead of `name`, in the reader's terms.
///
/// The named arguments no parameter of this rule has are the usual cause —
/// `item` against `doc` — and are what a reader has to see to fix it.
fn reason(event: &Event, parameters: &[(String, bool)], name: &str) -> String {
    let trigger = &event.trigger;
    let unmatched = unmatched(event, parameters);
    let said = if let Some(at) = doubly_handed(event, parameters, name) {
        format!(
            "it was handed `{name}` by name and again as its {} argument by position, and which \
             was meant is not for the simulator to choose",
            ordinal(at)
        )
    } else if !unmatched.is_empty() {
        format!(
            "it was handed {} by name, and a named argument binds only the parameter it names",
            unmatched.join(", ")
        )
    } else if event.arguments.is_empty() && event.positional.is_empty() {
        "it carried no arguments".to_owned()
    } else if event.positional.is_empty() {
        let named: Vec<String> =
            event.arguments.keys().map(|carried| format!("`{carried}`")).collect();
        format!("it named only {}", named.join(", "))
    } else {
        let count = event.positional.len();
        format!(
            "it carried {count} argument{} by position, and none in that place",
            if count == 1 { "" } else { "s" }
        )
    };
    format!("nothing this firing of `{trigger}` carried binds `{name}`: {said}")
}

#[cfg(test)]
mod tests {
    use inspect_model::Span;

    use super::*;
    use crate::value::EntityId;

    fn doc() -> Value {
        Value::Ref(EntityId::new("Doc", 1))
    }

    fn required(names: &[&str]) -> Vec<(String, bool)> {
        names.iter().map(|name| ((*name).to_owned(), false)).collect()
    }

    fn when(text: &str) -> RuleClause {
        RuleClause {
            keyword: "when".to_owned(),
            text: text.to_owned(),
            span: Some(Span { start: 3, end: 9 }),
        }
    }

    /// The same positional argument, under each listener's own name.
    #[test]
    fn a_positional_argument_binds_under_the_listener_s_own_name() {
        let event = Event::new("Announced", "chains").passing(doc());
        assert_eq!(bind(&event, &required(&["thing"])).get("thing"), Some(&doc()));
        assert_eq!(bind(&event, &required(&["doc"])).get("doc"), Some(&doc()));
    }

    /// In place: the second argument is the second parameter's.
    #[test]
    fn positional_arguments_bind_in_order() {
        let other = Value::Ref(EntityId::new("Doc", 2));
        let event = Event::new("Moved", "chains").passing(doc()).passing(other.clone());
        let bound = bind(&event, &required(&["from", "to"]));
        assert_eq!(bound.get("from"), Some(&doc()));
        assert_eq!(bound.get("to"), Some(&other));
    }

    /// A parameter this reads no name for is skipped, not shifted onto.
    #[test]
    fn a_parameter_with_no_name_takes_its_place_and_nothing_else() {
        let other = Value::Ref(EntityId::new("Doc", 2));
        let event = Event::new("Moved", "chains").passing(doc()).passing(other.clone());
        let bound = bind(&event, &[(String::new(), false), ("to".to_owned(), false)]);
        assert_eq!(bound.len(), 1, "{bound:?}");
        assert_eq!(bound.get("to"), Some(&other));
    }

    /// A named argument is not moved onto a parameter it does not name.
    #[test]
    fn a_named_argument_binds_only_its_own_name() {
        let event = Event::new("Published", "chains").with("item", doc());
        let bound = bind(&event, &required(&["doc"]));
        assert_eq!(bound.get("doc"), None);
        assert_eq!(bound.get("item"), Some(&doc()));
    }

    /// Named and positional both say what `doc` is: neither is picked, and
    /// the reason says it was handed both ways, in which place.
    #[test]
    fn a_parameter_handed_a_value_both_by_name_and_by_position_is_left_unbound() {
        let named = Value::Ref(EntityId::new("Doc", 2));
        let event = Event::new("Published", "chains").passing(doc()).with("doc", named);
        let parameters = required(&["doc"]);
        let bound = bind(&event, &parameters);
        assert_eq!(bound.get("doc"), None, "{bound:?}");
        let gaps = unbound(&event, &parameters, &bound, None);
        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert_eq!(
            gaps[0].reason,
            "nothing this firing of `Published` carried binds `doc`: it was handed `doc` by name \
             and again as its 1st argument by position, and which was meant is not for the \
             simulator to choose"
        );
    }

    /// `Moved(doc, to: doc.owner)` against `when: Moved(to, from)`: the one
    /// positional argument went to `to`'s place, so `from` is told it had
    /// none in its own — which is now true — and `to` is told why it is not
    /// bound either.
    #[test]
    fn a_positional_argument_displaced_by_a_name_is_reported_not_dropped() {
        let event = Event::new("Moved", "chains").passing(doc()).with("to", doc());
        let parameters = required(&["to", "from"]);
        let bound = bind(&event, &parameters);
        assert!(bound.is_empty(), "{bound:?}");
        let gaps = unbound(&event, &parameters, &bound, None);
        assert_eq!(gaps.len(), 2, "{gaps:?}");
        assert!(gaps[0].reason.contains("binds `to`: it was handed `to` by name"), "{gaps:?}");
        assert!(
            gaps[1].reason.ends_with(
                "binds `from`: it carried 1 argument by position, and none \
             in that place"
            ),
            "{gaps:?}"
        );
    }

    /// The same, for `?`: handed both ways, it is not quietly `null`.
    #[test]
    fn an_optional_parameter_handed_both_ways_is_not_null() {
        let event = Event::new("Reported", "chains").passing(doc()).with("note", doc());
        let parameters = [("note".to_owned(), true)];
        let bound = bind(&event, &parameters);
        assert_eq!(bound.get("note"), None, "{bound:?}");
        assert_eq!(unbound(&event, &parameters, &bound, None).len(), 1);
    }

    /// `Noted(doc: d, reason: d)` against `when: Noted(doc, cause?)`:
    /// `reason` may be `cause` misnamed, so `cause` is not `null` by default
    /// and the gap names `reason`.
    #[test]
    fn an_optional_parameter_beside_an_unmatched_name_is_not_null() {
        let event = Event::new("Noted", "chains").with("doc", doc()).with("reason", doc());
        let parameters = [("doc".to_owned(), false), ("cause".to_owned(), true)];
        let bound = bind(&event, &parameters);
        assert_eq!(bound.get("doc"), Some(&doc()));
        assert_eq!(bound.get("cause"), None, "{bound:?}");
        let gaps = unbound(&event, &parameters, &bound, None);
        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert_eq!(
            gaps[0].reason,
            "nothing this firing of `Noted` carried binds `cause`: it was handed `reason` by \
             name, and a named argument binds only the parameter it names"
        );
    }

    /// Every name matched, an optional nobody passed is still `null`.
    #[test]
    fn an_optional_parameter_beside_only_matched_names_is_null() {
        let event = Event::new("Noted", "chains").with("doc", doc());
        let parameters = [("doc".to_owned(), false), ("cause".to_owned(), true)];
        let bound = bind(&event, &parameters);
        assert_eq!(bound.get("cause"), Some(&Value::Null));
        assert!(unbound(&event, &parameters, &bound, None).is_empty());
    }

    #[test]
    fn places_are_counted_the_way_a_reader_counts_them() {
        let said: Vec<String> = [0, 1, 2, 3, 10, 11, 12, 20, 21, 101, 111].map(ordinal).into();
        assert_eq!(
            said,
            ["1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "102nd", "112th"]
        );
    }

    /// `?` is `null` when nothing was passed, and the value when one was.
    #[test]
    fn an_optional_parameter_is_null_only_when_nothing_was_passed() {
        let optional = [("note".to_owned(), true)];
        let silent = Event::new("Reported", "chains");
        assert_eq!(bind(&silent, &optional).get("note"), Some(&Value::Null));
        let said = Event::new("Reported", "chains").passing(doc());
        assert_eq!(bind(&said, &optional).get("note"), Some(&doc()));
    }

    #[test]
    fn every_bound_parameter_leaves_nothing_to_say() {
        let event = Event::new("Published", "chains").with("doc", doc());
        let parameters = required(&["doc"]);
        let bound = bind(&event, &parameters);
        assert!(unbound(&event, &parameters, &bound, None).is_empty());
    }

    /// Issue #12: the name waited for, the name handed, and the clause.
    #[test]
    fn a_name_mismatch_says_which_name_was_waited_for_and_which_was_handed() {
        let event = Event::new("Published", "chains").with("item", doc());
        let parameters = required(&["doc"]);
        let bound = bind(&event, &parameters);
        let clause = when("Published(doc)");
        let gaps = unbound(&event, &parameters, &bound, Some(&clause));
        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert_eq!(
            gaps[0].reason,
            "nothing this firing of `Published` carried binds `doc`: it was handed `item` by \
             name, and a named argument binds only the parameter it names"
        );
        assert_eq!(gaps[0].expression.as_deref(), Some("Published(doc)"));
        assert_eq!(gaps[0].span, Some(Span { start: 3, end: 9 }));
    }

    /// A named argument that *is* one of the rule's parameters is not the
    /// cause, and is not blamed.
    #[test]
    fn only_names_the_rule_does_not_have_are_blamed() {
        let event = Event::new("Moved", "chains").with("from", doc());
        let parameters = required(&["from", "to"]);
        let bound = bind(&event, &parameters);
        let gaps = unbound(&event, &parameters, &bound, None);
        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert!(gaps[0].reason.contains("binds `to`"), "{}", gaps[0].reason);
        assert!(gaps[0].reason.ends_with("it named only `from`"), "{}", gaps[0].reason);
    }

    #[test]
    fn a_firing_with_nothing_says_it_carried_nothing() {
        let event = Event::new("Published", "chains");
        let parameters = required(&["doc"]);
        let gaps = unbound(&event, &parameters, &bind(&event, &parameters), None);
        assert_eq!(
            gaps[0].reason,
            "nothing this firing of `Published` carried binds `doc`: it carried no arguments"
        );
        assert_eq!(gaps[0].expression, None);
        assert_eq!(gaps[0].span, None);
    }

    /// Too few by position: counted, and singular when it is one.
    #[test]
    fn too_few_positional_arguments_are_counted() {
        let event = Event::new("Moved", "chains").passing(doc());
        let parameters = required(&["from", "to"]);
        let gaps = unbound(&event, &parameters, &bind(&event, &parameters), None);
        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert!(
            gaps[0].reason.ends_with("it carried 1 argument by position, and none in that place"),
            "{}",
            gaps[0].reason
        );
        let more = Event::new("Moved", "chains").passing(doc()).passing(doc());
        let three = required(&["a", "b", "c"]);
        let gaps = unbound(&more, &three, &bind(&more, &three), None);
        assert!(gaps[0].reason.contains("2 arguments by position"), "{}", gaps[0].reason);
    }

    /// Optional and nameless parameters are never a gap.
    #[test]
    fn optional_and_nameless_parameters_are_never_a_gap() {
        let event = Event::new("Reported", "chains");
        let parameters = [("note".to_owned(), true), (String::new(), false)];
        let bound = bind(&event, &parameters);
        assert!(unbound(&event, &parameters, &bound, None).is_empty());
    }
}

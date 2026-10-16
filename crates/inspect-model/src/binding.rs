//! How what a firing carries meets the parameters a listener's `when` names.
//!
//! Allium declares a trigger nowhere but in the rules that listen for it:
//! `when: Published(doc)` *is* the declaration, for that rule. So a firing's
//! arguments are bound by each listener on its own terms —
//!
//! - **by position** when they were passed by position. `Announced(doc)` hands
//!   its first argument to `when: Announced(thing)` as `thing` and to `when:
//!   Announced(doc)` as `doc`;
//! - **by name** when they were named. `Published(item: doc)` binds `item`,
//!   and only `item`. A listener that calls its parameter `doc` is not handed
//!   it by being the only parameter there is, because which of a listener's
//!   names an argument it was not given was *meant* for is a guess;
//! - **by neither** when it was handed a value both ways, `X(d, doc: e)`
//!   against `when: X(doc, other)`: which of the two was meant is the same
//!   kind of guess.
//!
//! And `?` is `null` only when nothing could have been meant for it: a firing
//! that named an argument this rule has no parameter for may have misnamed
//! the optional one.
//!
//! Here rather than in the simulator because the answer depends on nothing
//! but the *shape* of the firing — which names it carried, and how many
//! arguments by position — and two things ask it. The simulator binds a
//! firing's values by it, and ingestion reports, before anything runs, an
//! emission a listener can never bind. One function answering both is what
//! keeps the up-front report and the walk from ever disagreeing.

use std::collections::BTreeSet;

/// The shape of one firing: what it carried, without the values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carried<'a> {
    /// The trigger, as its reason names it.
    pub trigger: &'a str,
    /// The names of the arguments it passed by name.
    pub named: BTreeSet<&'a str>,
    /// How many arguments it passed by position.
    pub positional: usize,
}

/// Where one parameter's value comes from, for one firing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bound {
    /// The argument passed under the parameter's own name.
    Named,
    /// The argument passed by position, in this zero-based place.
    Positional(usize),
    /// Nothing, and the parameter is `name?`: it is `null`.
    Null,
    /// Nothing binds it, and why — in the reader's terms, without the
    /// trigger's name, so each caller can say whose firing it was.
    Unbound(String),
}

/// Where each named parameter of `parameters` is bound from by `carried`, in
/// order. A parameter this reads no name for is skipped, not shifted onto: it
/// still takes its place.
#[must_use]
pub fn plan<'p>(carried: &Carried<'_>, parameters: &'p [(String, bool)]) -> Vec<(&'p str, Bound)> {
    let everything_matched = unmatched(carried, parameters).is_empty();
    parameters
        .iter()
        .enumerate()
        .filter(|(_, (name, _))| !name.is_empty())
        .map(|(at, (name, optional))| {
            let named = carried.named.contains(name.as_str());
            let placed = at < carried.positional;
            let bound = match (named, placed) {
                (true, true) => Bound::Unbound(why(carried, parameters, name)),
                (true, false) => Bound::Named,
                (false, true) => Bound::Positional(at),
                (false, false) if *optional && everything_matched => Bound::Null,
                (false, false) => Bound::Unbound(why(carried, parameters, name)),
            };
            (name.as_str(), bound)
        })
        .collect()
}

/// Why nothing `carried` binds `name`, as the walk says it: naming the
/// trigger and the parameter.
#[must_use]
pub fn reason(carried: &Carried<'_>, parameters: &[(String, bool)], name: &str) -> String {
    format!(
        "nothing this firing of `{}` carried binds `{name}`: {}",
        carried.trigger,
        why(carried, parameters, name)
    )
}

/// What `carried` held instead of `name`.
///
/// The named arguments no parameter of this rule has are the usual cause —
/// `item` against `doc` — and are what a reader has to see to fix it.
#[must_use]
pub fn why(carried: &Carried<'_>, parameters: &[(String, bool)], name: &str) -> String {
    let unmatched = unmatched(carried, parameters);
    if let Some(at) = doubly_handed(carried, parameters, name) {
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
    } else if carried.named.is_empty() && carried.positional == 0 {
        "it carried no arguments".to_owned()
    } else if carried.positional == 0 {
        let named: Vec<String> = carried.named.iter().map(|it| format!("`{it}`")).collect();
        format!("it named only {}", named.join(", "))
    } else {
        let count = carried.positional;
        format!(
            "it carried {count} argument{} by position, and none in that place",
            if count == 1 { "" } else { "s" }
        )
    }
}

/// The named arguments `carried` holds that no parameter of this rule has.
fn unmatched(carried: &Carried<'_>, parameters: &[(String, bool)]) -> Vec<String> {
    carried
        .named
        .iter()
        .filter(|it| !parameters.iter().any(|(parameter, _)| parameter == *it))
        .map(|it| format!("`{it}`"))
        .collect()
}

/// Where `name` was handed a value by position as well as by name, if it was.
fn doubly_handed(
    carried: &Carried<'_>,
    parameters: &[(String, bool)],
    name: &str,
) -> Option<usize> {
    if !carried.named.contains(name) {
        return None;
    }
    parameters.iter().take(carried.positional).position(|(parameter, _)| parameter == name)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn carried<'a>(named: &[&'a str], positional: usize) -> Carried<'a> {
        Carried { trigger: "T", named: named.iter().copied().collect(), positional }
    }

    fn required(names: &[&str]) -> Vec<(String, bool)> {
        names.iter().map(|name| ((*name).to_owned(), false)).collect()
    }

    fn unbound(plan: &[(&str, Bound)]) -> Vec<String> {
        plan.iter()
            .filter(|(_, bound)| matches!(bound, Bound::Unbound(_)))
            .map(|(name, _)| (*name).to_owned())
            .collect()
    }

    #[test]
    fn by_position_each_in_its_own_place() {
        let parameters = required(&["from", "to"]);
        assert_eq!(
            plan(&carried(&[], 2), &parameters),
            [("from", Bound::Positional(0)), ("to", Bound::Positional(1))]
        );
    }

    #[test]
    fn by_name_only_the_name_it_names() {
        let parameters = required(&["doc"]);
        let planned = plan(&carried(&["item"], 0), &parameters);
        assert_eq!(unbound(&planned), ["doc"]);
        assert_eq!(plan(&carried(&["doc"], 0), &parameters), [("doc", Bound::Named)]);
    }

    #[test]
    fn both_ways_is_neither() {
        let parameters = required(&["doc"]);
        let planned = plan(&carried(&["doc"], 1), &parameters);
        assert_eq!(
            planned,
            [(
                "doc",
                Bound::Unbound(
                    "it was handed `doc` by name and again as its 1st argument by position, \
                     and which was meant is not for the simulator to choose"
                        .to_owned()
                )
            )]
        );
    }

    /// Named in another place than the one it was handed by position: the
    /// name binds, and the place it would have taken is someone else's.
    #[test]
    fn a_name_handed_beyond_the_positional_ones_is_not_doubly_handed() {
        let parameters = required(&["from", "to"]);
        assert_eq!(
            plan(&carried(&["to"], 1), &parameters),
            [("from", Bound::Positional(0)), ("to", Bound::Named)]
        );
    }

    #[test]
    fn too_few_by_position_is_counted() {
        let parameters = required(&["from", "to"]);
        let planned = plan(&carried(&[], 1), &parameters);
        assert_eq!(
            planned[1],
            (
                "to",
                Bound::Unbound(
                    "it carried 1 argument by position, and none in that place".to_owned()
                )
            )
        );
        let three = required(&["a", "b", "c"]);
        let planned = plan(&carried(&[], 2), &three);
        assert!(matches!(&planned[2].1, Bound::Unbound(why) if why.contains("2 arguments")));
    }

    #[test]
    fn an_optional_is_null_only_beside_names_that_all_matched() {
        let parameters = [("doc".to_owned(), false), ("cause".to_owned(), true)];
        assert_eq!(plan(&carried(&["doc"], 0), &parameters)[1], ("cause", Bound::Null));
        let misnamed = plan(&carried(&["doc", "reason"], 0), &parameters);
        assert_eq!(unbound(&misnamed), ["cause"]);
    }

    #[test]
    fn an_optional_handed_both_ways_is_not_null() {
        let parameters = [("note".to_owned(), true)];
        assert_eq!(unbound(&plan(&carried(&["note"], 1), &parameters)), ["note"]);
    }

    #[test]
    fn a_parameter_with_no_name_is_skipped_and_keeps_its_place() {
        let parameters = [(String::new(), false), ("to".to_owned(), false)];
        assert_eq!(plan(&carried(&[], 2), &parameters), [("to", Bound::Positional(1))]);
    }

    #[test]
    fn the_reason_names_the_trigger_and_the_parameter() {
        let parameters = required(&["doc"]);
        assert_eq!(
            reason(&carried(&[], 0), &parameters, "doc"),
            "nothing this firing of `T` carried binds `doc`: it carried no arguments"
        );
        assert_eq!(
            why(&carried(&["from"], 0), &required(&["from", "to"]), "to"),
            "it named only `from`"
        );
    }

    #[test]
    fn places_are_counted_the_way_a_reader_counts_them() {
        let said: Vec<String> = [0, 1, 2, 3, 10, 11, 12, 20, 21, 101, 111].map(ordinal).into();
        assert_eq!(
            said,
            ["1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "102nd", "112th"]
        );
    }
}

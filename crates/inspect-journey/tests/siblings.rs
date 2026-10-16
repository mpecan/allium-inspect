//! Rules woken by one trigger read the world as the trigger found it
//! (issue #24).
//!
//! `CreateIfMissing` and `ReplaceIfPresent` both listen for `Put`, and their
//! preconditions rule each other out, so exactly one runs. Read in a sequence,
//! the replacement saw the box the creation had just made and ran on it as
//! well, and `then ReplaceIfPresent does not fire` was refused.
//!
//! Against the recorded `allium` output and the real simulator, so what passes
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::SIBLINGS;
use inspect_journey::{Verdict, Walk, parse, walk};

const AS_FILED: &str = include_str!("fixtures/siblings.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(AS_FILED).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey {name}"));
    let (graph, program, sources) = common::library(SIBLINGS);
    walk(journey, &journeys, &graph, &program, &sources)
}

/// Every line of a walk held, and so did every step.
fn all_held(walk: &Walk) {
    let bad: Vec<_> = walk
        .steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .filter(|outcome| outcome.verdict != Verdict::Specified)
        .map(|outcome| (outcome.verdict, outcome.about.clone(), outcome.detail.clone()))
        .collect();
    assert!(bad.is_empty(), "every line should hold: {bad:#?}");
    for step in &walk.steps {
        assert_eq!(step.verdict(), Verdict::Specified, "step {}", step.number);
    }
}

/// The line about `about`, and what it said.
fn line<'a>(walk: &'a Walk, about: &str) -> &'a inspect_journey::Outcome {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .find(|outcome| outcome.about == about)
        .unwrap_or_else(|| panic!("no line `{about}`: {walk:#?}"))
}

/// The reproduction from the issue, as it was filed.
#[test]
fn a_first_put_creates_and_does_not_also_replace() {
    all_held(&walked("FirstPut"));
}

/// The other half of the same choice.
#[test]
fn a_second_put_replaces_and_does_not_also_create() {
    all_held(&walked("SecondPut"));
}

/// Two independent reactions to one act both run: neither is refused by what
/// the other wrote.
#[test]
fn independent_reactions_both_run() {
    all_held(&walked("TouchedOnce"));
}

/// Two siblings writing one field differently: whichever sorted last used to
/// win, silently. The colour is undecided, and the step says which two rules
/// disagreed.
#[test]
fn two_siblings_writing_one_field_leave_it_undecided() {
    let walk = walked("TwoPaintersOneLamp");
    let colour = line(&walk, "then l.colour = \"red\"");
    assert_eq!(colour.verdict, Verdict::Undecided, "{colour:#?}");
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{walk:#?}");
    let said: Vec<String> =
        walk.steps[0].outcomes.iter().filter_map(|outcome| outcome.detail.clone()).collect();
    assert!(
        said.iter()
            .any(|detail| detail.contains("`PaintBlue` and `PaintRed` were both woken by `Paint`")
                && detail.contains("does not say which of the two runs first")),
        "{said:#?}"
    );
    // Quoted word for word in the reference, which `allium-journey guide`
    // ships to spec authors.
    const REFERENCE: &str = include_str!("../../../docs/journeys/reference.md");
    let act = line(&walk, "b does Paint(l) on Shelf").detail.clone().unwrap_or_default();
    assert!(
        !act.is_empty() && REFERENCE.contains(&act),
        "the reference does not quote:
{act}"
    );
}

/// What a postcondition reads is the world the trigger found: `Shade` copies
/// the colour the lamp had, not the one `DyeRed` — sorted first — wrote. Read
/// as a sequence, the shade came out red, and renaming a rule turned it white.
#[test]
fn a_postcondition_copies_what_the_trigger_found() {
    all_held(&walked("ShadeFollows"));
}

/// A field two siblings disagreed about stays undecided whatever a third
/// writes, so the next step's precondition reading it is undecided too —
/// it used to hold, on red, because the third sibling agreed with the first.
#[test]
fn a_contested_field_is_undecided_in_the_next_step() {
    let walk = walked("ThreePaintersThenAdmire");
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{walk:#?}");
    let admire = line(&walk, "then Admire fires");
    assert_eq!(admire.verdict, Verdict::Undecided, "{admire:#?}");
    assert!(
        admire
            .detail
            .as_deref()
            .is_some_and(|it| it.contains("`RepaintA` and `RepaintB` had already disagreed")),
        "{admire:#?}"
    );
}

/// A removal and an assignment to what it removed hold in either order.
#[test]
fn a_removal_beside_an_assignment_holds_in_either_order() {
    all_held(&walked("BinnedAndLabelled"));
    all_held(&walked("MarkedAndTossed"));
}

/// A lifecycle refusal from the value the trigger found is the rule's own, so
/// the other sibling's write stands and the step holds; two permitted moves
/// to different states are undecided.
#[test]
fn a_refusal_is_the_rules_own_and_two_permitted_moves_clash() {
    all_held(&walked("KnockedOpen"));
    let walk = walked("SlammedBothWays");
    assert_eq!(line(&walk, "then d.status = open").verdict, Verdict::Undecided, "{walk:#?}");
}

/// State rules and creation reactions woken together read the world the
/// trigger found as well.
#[test]
fn state_rules_and_creation_reactions_read_the_world_the_trigger_found() {
    all_held(&walked("BothStateRulesRun"));
    all_held(&walked("BothReactionsRun"));
}

/// The same set written in two orders is one value, as the spec compares
/// them: both siblings stand, and the set is what they agreed on.
#[test]
fn siblings_writing_one_set_in_two_orders_agree() {
    all_held(&walked("TwoTaggersAgree"));
}

/// Both add one to the count the trigger found and write the same `1` — a
/// value no order of the two produces. Not an agreement: the count is
/// undecided, and says that each read it as the trigger found it.
#[test]
fn two_increments_from_one_read_are_not_an_agreement() {
    let walk = walked("TwoCountsFromOneRead");
    assert_eq!(line(&walk, "then CountOnce fires").verdict, Verdict::Specified);
    let said = line(&walk, "then l.touches = 1");
    assert_eq!(said.verdict, Verdict::Undecided, "{said:#?}");
    let detail = said.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains("both computed it from `l.touches` as the trigger found it")
            && detail.contains("the second would read what the first wrote"),
        "{detail}"
    );
}

//! A bare word in a value position, checked against the states the spec
//! declares.
//!
//! A journey writes a state the way the spec does — `copy.status = available`
//! — and nothing in the grammar tells that word apart from somebody the
//! journey cast. The walker decides: a bound name is what it is bound to, and
//! any other word is read as the state it spells. That reading was taken on
//! sight, so a word the spec never declared was compared all the same:
//! `given copy.status = availble` laid out a status no lifecycle mentions,
//! `then loan.status = opne` came back **refused**, as though the spec had said
//! no, and `then copy.status != on_lone` *held*.
//!
//! So the checker asks the spec first, with the same two questions the
//! simulator asks of a word a rule passes on (`inspect_sim::apply`):
//!
//! - **Where the field is known**, whether *that field* declares the word —
//!   [`SpecGraph::states_of`], its inline states or the enumeration it is typed
//!   by, that name read in the module declaring the field. `available` is a
//!   copy's state and never a loan's, and `then loan.status = available` can
//!   never hold.
//! - **Where it is not**, or the field declares no states, whether *anything*
//!   does — [`SpecGraph::declared_states`]. A field typed by another entity, a
//!   `String`, or a path this checker cannot follow says nothing about which
//!   words it takes, and a word the spec declares somewhere is the state meant.
//!
//! What fails both is **unspecified** and names the word, with the nearest
//! declared state when one is close. Nothing is compared, because there is
//! nothing the spec could say about a state it has never heard of.
//!
//! What is left alone: a call's arguments and answer in a `stipulate`, which
//! are matched against the words the spec spells in that call rather than
//! against states (`stipulate can(r, code)`); `config`, whose values the walk
//! checks against the parameter's declared type; and a capitalised word, which
//! is a collection. An act's arguments are `crate::states`'s, because a
//! trigger's parameters carry no types.

use std::collections::BTreeSet;

use inspect_model::{
    NodeKind, SpecGraph,
    graph::{EntityDetail, EntityField},
};

use crate::{
    check::{Note, Verdict},
    journey::{Given, Path, Term},
};

/// The field a value is written into, when the checker can name it.
#[derive(Clone, Copy)]
pub(crate) struct Field<'g> {
    /// The type that declares it, bare: `Copy`.
    pub(crate) owner: &'g str,
    /// The module that type is declared in, which is where the name of the
    /// enumeration the field is typed by is read from.
    pub(crate) module: &'g str,
    pub(crate) field: &'g EntityField,
}

/// An entity or value type `type_expr` names, with its fields — `None` for
/// an actor, a variant, or a name the spec does not have.
pub(crate) struct Declared<'g> {
    /// Its bare name: `Copy`.
    pub(crate) name: &'g str,
    pub(crate) module: &'g str,
    pub(crate) detail: &'g EntityDetail,
}

/// The entity or value type `type_expr` names.
///
/// Qualified names carry their module, and an optional carries a `?`. The
/// module counts: two modules may each declare an `Order`, with different
/// fields, and `warehouse/Order` is the one the journey said.
pub(crate) fn fields_of<'g>(graph: &'g SpecGraph, type_expr: &str) -> Option<Declared<'g>> {
    let written = type_expr.trim().trim_end_matches('?');
    let (module, bare) = written.rsplit_once('/').map_or((None, written), |(m, b)| (Some(m), b));
    let node = graph
        .nodes
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Entity | NodeKind::Value))
        .filter(|node| module.is_none_or(|module| node.module == module))
        .find(|node| node.name == bare)?;
    Some(Declared {
        name: node.name.as_str(),
        module: node.module.as_str(),
        detail: node.detail.as_entity()?,
    })
}

/// The field `segments` ends on, starting from something of type `type_expr`.
///
/// Followed one hop at a time while each hop's type is an entity or value type
/// the spec declares; anything else — a collection, a scalar, a name the spec
/// does not have — and the field is not known, which is not the same as having
/// none.
///
/// `type_expr` is the journey's, and qualified by module. Every hop after it
/// is the spec's, and read the way [`SpecGraph::states_of`] reads an
/// enumeration: in the module of the entity declaring the field, a bare name
/// that module's own and `alias/X` through its imports. Read by name alone, an
/// issue's `item: Item` was the first `Item` in the set — another module's,
/// with other states — and a correct `then` was reported unspecified.
pub(crate) fn field_at<'g>(
    graph: &'g SpecGraph,
    type_expr: &str,
    segments: &[String],
) -> Option<Field<'g>> {
    let (last, hops) = segments.split_last()?;
    let mut owner = fields_of(graph, type_expr)?;
    for hop in hops {
        owner = declared_in(graph, owner.module, &owner.detail.field(hop)?.type_expr)?;
    }
    let field = owner.detail.field(last)?;
    Some(Field { owner: owner.name, module: owner.module, field })
}

/// The entity or value type `type_expr` names, written in the spec's
/// `module`.
///
/// Where that module leads to no declaration of the name, it is looked up by
/// name alone — and only when the set declares exactly one, because picking
/// one of several is a guess, and an unknown field is one the checker already
/// reads as *not known* rather than *none*.
fn declared_in<'g>(graph: &'g SpecGraph, module: &str, type_expr: &str) -> Option<Declared<'g>> {
    let written = type_expr.trim().trim_end_matches('?');
    let (qualifier, bare) =
        written.rsplit_once('/').map_or((None, written), |(alias, bare)| (Some(alias), bare));
    let home = graph.home_of(module, qualifier);
    let named = |node: &&'g inspect_model::Node| {
        matches!(node.kind, NodeKind::Entity | NodeKind::Value) && node.name == bare
    };
    let node = match graph.nodes.iter().filter(named).find(|node| node.module == home) {
        Some(node) => node,
        None => {
            let mut anywhere = graph.nodes.iter().filter(named);
            let only = anywhere.next()?;
            if anywhere.next().is_some() {
                return None;
            }
            only
        }
    };
    Some(Declared {
        name: node.name.as_str(),
        module: node.module.as_str(),
        detail: node.detail.as_entity()?,
    })
}

/// A value a journey line writes, and where.
#[derive(Clone, Copy)]
pub(crate) struct Written<'t, 'g> {
    pub(crate) value: &'t Term,
    /// The field it is written into, when the checker can name it.
    pub(crate) into: Option<Field<'g>>,
    pub(crate) line: usize,
}

/// Who a line can name.
pub(crate) trait Knows {
    /// Bound by the time the line is laid out.
    fn now(&self, name: &str) -> bool;
    /// Bound only afterwards: somebody the journey casts, named by a line of
    /// the file's world, which is laid out first.
    fn later(&self, _name: &str) -> bool {
        false
    }
    /// The type `name` was bound under, as the journey wrote it.
    ///
    /// No default: every caller that reads a path needs the answer, and a
    /// default of "none" would quietly check a field against every state.
    fn type_of(&self, name: &str) -> Option<&str>;
}

/// The words one `given` line writes, against the fields it writes them to.
pub(crate) fn given(given: &Given, known: &dyn Knows, graph: &SpecGraph, notes: &mut Vec<Note>) {
    match given {
        Given::Instance { type_expr, fields, line, .. } => {
            for (field, value) in fields {
                let into = field_at(graph, type_expr, std::slice::from_ref(field));
                check(Written { value, into, line: *line }, known, graph, notes);
            }
        }
        Given::Assign { path, value, line } => {
            written(path, Written { value, into: None, line: *line }, known, graph, notes);
        }
        // A call's arguments and answer are matched against what the spec
        // spells in that call, not against states — as in a step's
        // `stipulate`.
        Given::Answer { .. } => {}
    }
}

/// A word written into `path` — by `given`, `stipulate` or compared in a
/// `then` — against the field `path` ends on.
///
/// `config` is the walk's: it checks a value against the parameter's declared
/// type, which says more than any state could.
pub(crate) fn written(
    path: &Path,
    written: Written<'_, '_>,
    known: &dyn Knows,
    graph: &SpecGraph,
    notes: &mut Vec<Note>,
) {
    if crate::config::is_config(path) {
        return;
    }
    let into =
        known.type_of(&path.root).and_then(|type_expr| field_at(graph, type_expr, &path.segments));
    check(Written { into, ..written }, known, graph, notes);
}

/// Whatever `written` writes that is a word nobody bound and no state of the
/// field it goes into — or of anything, when the field is not known or
/// declares none.
pub(crate) fn check(
    written: Written<'_, '_>,
    knows: &dyn Knows,
    graph: &SpecGraph,
    notes: &mut Vec<Note>,
) {
    let Written { value, into, line } = written;
    match value {
        // `{a, b}` into a set: each member is a word of its own. A set-typed
        // field declares no states of its own, so this is the spec-wide check.
        Term::Set(items) => {
            for item in items {
                check(Written { value: item, ..written }, knows, graph, notes);
            }
        }
        Term::Path(path) => {
            let Some(word) = unbound_word(path, &|name| knows.now(name)) else { return };
            let message = if knows.later(word) {
                Some(format!(
                    "`{word}` is cast by this journey, and the file's world is laid out before \
                     the journey's cast, so this line cannot name them — cast `{word}` in the \
                     world, or write this line in the journey's own `given`"
                ))
            } else {
                unknown(word, into, graph)
            };
            if let Some(message) = message {
                notes.push(Note { line, verdict: Verdict::Unspecified, message });
            }
        }
        Term::Literal(_) | Term::Clock { .. } => {}
    }
}

/// Why `term` is a word [`check`] reports — one that, laid out, would be a
/// state the spec has never declared for `field` — or `None` when it is not.
/// A set is reported when any member is, as `check` reports each member.
///
/// The walk asks this before it lays a `given` out, and leaves such a field
/// unset rather than holding the word. Held, it was compared: `copy.status =
/// availble` had `BorrowCopy` *refused* on `copy.status = available`, the spec
/// saying no on the strength of a state it has never heard of. Unset, the rule
/// is undecided and names the field — and a note on the line says why nothing
/// was written there: the checker's, or the walk's own where the checker said
/// nothing about that line.
pub(crate) fn rejected(
    term: &Term,
    field: Option<Field<'_>>,
    knows: &dyn Fn(&str) -> bool,
    graph: &SpecGraph,
) -> Option<String> {
    match term {
        Term::Set(items) => items.iter().find_map(|item| rejected(item, field, knows, graph)),
        Term::Path(path) => unbound_word(path, knows).and_then(|word| unknown(word, field, graph)),
        Term::Literal(_) | Term::Clock { .. } => None,
    }
}

/// The word `path` is, when it is a single lowercase word nobody bound.
pub(crate) fn unbound_word<'p>(path: &'p Path, knows: &dyn Fn(&str) -> bool) -> Option<&'p str> {
    let word = path.root.as_str();
    let lowercase = word.chars().next().is_some_and(|first| !first.is_uppercase());
    (path.segments.is_empty() && lowercase && !knows(word)).then_some(word)
}

/// Why `word` is no state for `field`, or `None` when it is one.
///
/// A field that declares no states — or whose enumeration's name is ambiguous
/// from its module, which [`SpecGraph::states_of`] also answers with none —
/// takes any state the spec declares. The simulator is stricter about the
/// same field: `x.f = word` in a rule's `ensures` stays undecided unless the
/// field itself declares the word (`inspect_sim`'s `declared_state`). Both are
/// honest: a journey's word is the author's own and is read as the state it
/// spells when the spec has one, while a rule's word written into a field
/// that names no states could be anything. So a journey may lay out a word
/// that the spec's own postcondition, writing the same field, leaves
/// undecided.
fn unknown(word: &str, field: Option<Field<'_>>, graph: &SpecGraph) -> Option<String> {
    if let Some(Field { owner, module, field }) = field {
        let states = graph.states_of(module, field);
        if !states.is_empty() {
            if states.iter().any(|state| state == word) {
                return None;
            }
            return Some(format!(
                "`{word}` is no state of `{owner}.{}`, which declares {}{}",
                field.name,
                states.join(", "),
                hint(word, states.iter().map(String::as_str))
            ));
        }
    }
    let declared = graph.declared_states();
    if declared.contains(word) {
        return None;
    }
    Some(format!(
        "`{word}` is nobody in this journey and no state the spec declares{}",
        hint(word, declared.iter().copied())
    ))
}

/// ` — did you mean `x`?` for the nearest of `candidates`, when one is near.
pub(crate) fn hint<'a>(word: &str, candidates: impl Iterator<Item = &'a str>) -> String {
    nearest(word, candidates).map_or_else(String::new, |near| format!(" — did you mean `{near}`?"))
}

/// The candidate fewest edits from `word`, when it is close enough to be a
/// slip rather than a different word: a third of the word's length, and at
/// least one. Ties go to the first in order, so the answer is the same every
/// time it is asked.
fn nearest<'a>(word: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let allowed = (word.chars().count() / 3).max(1);
    let unique: BTreeSet<&str> = candidates.collect();
    unique
        .into_iter()
        .map(|candidate| (distance(word, candidate), candidate))
        .filter(|(edits, _)| *edits <= allowed)
        .min_by_key(|(edits, _)| *edits)
        .map(|(_, candidate)| candidate)
}

/// Edits between `a` and `b`, a swap of two neighbours counting as one —
/// `chnaged` is one slip from `changed`, not two.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let width = b.len() + 1;
    let mut table = vec![0usize; (a.len() + 1) * width];
    let at = |i: usize, j: usize| i * width + j;
    for i in 0..=a.len() {
        table[at(i, 0)] = i;
    }
    for j in 0..=b.len() {
        table[at(0, j)] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (table[at(i - 1, j)] + 1)
                .min(table[at(i, j - 1)] + 1)
                .min(table[at(i - 1, j - 1)] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(table[at(i - 2, j - 2)] + 1);
            }
            table[at(i, j)] = best;
        }
    }
    table[at(a.len(), b.len())]
}

#[cfg(test)]
mod tests {
    use inspect_model::{
        Node, NodeKind,
        graph::{EntityDetail, EntityKind, EnumDetail, NodeDetail},
    };

    use super::*;

    fn field(name: &str, type_expr: &str, states: &[&str]) -> EntityField {
        EntityField {
            name: name.to_owned(),
            type_expr: type_expr.to_owned(),
            enum_values: states.iter().map(|state| (*state).to_owned()).collect(),
            derived: false,
            relationship: false,
            when: None,
            note: Vec::new(),
        }
    }

    fn entity(name: &str, fields: Vec<EntityField>) -> Node {
        let mut node = Node::new("m", NodeKind::Entity, name);
        node.detail = NodeDetail::Entity(EntityDetail {
            kind: EntityKind::Internal,
            fields,
            transitions: Vec::new(),
            parent: None,
        });
        node
    }

    /// A loan with an inline status, a copy it points at, and an enumeration
    /// of media one of the copy's fields is typed by.
    fn graph() -> SpecGraph {
        let mut graph = SpecGraph::new("test");
        graph.nodes.push(entity(
            "Loan",
            vec![
                field("status", "open | returned", &["open", "returned"]),
                field("copy", "Copy?", &[]),
            ],
        ));
        graph.nodes.push(entity(
            "Copy",
            vec![field("medium", "m/Medium", &[]), field("tags", "Set<String>", &[])],
        ));
        let mut medium = Node::new("m", NodeKind::Enum, "Medium");
        medium.detail =
            NodeDetail::Enum(EnumDetail { values: vec!["print".to_owned(), "audio".to_owned()] });
        graph.nodes.push(medium);
        graph
    }

    fn word(text: &str) -> Term {
        Term::Path(Path { root: text.to_owned(), segments: Vec::new() })
    }

    fn notes_for(term: &Term, owner: &str, path: &[&str]) -> Vec<Note> {
        let graph = graph();
        let segments: Vec<String> = path.iter().map(|segment| (*segment).to_owned()).collect();
        let into = field_at(&graph, owner, &segments);
        let mut notes = Vec::new();
        struct Ada;
        impl Knows for Ada {
            fn now(&self, name: &str) -> bool {
                name == "ada"
            }
            fn type_of(&self, _name: &str) -> Option<&str> {
                None
            }
        }
        check(Written { value: term, into, line: 7 }, &Ada, &graph, &mut notes);
        notes
    }

    #[test]
    fn a_swap_of_neighbours_is_one_edit() {
        assert_eq!(distance("chnaged", "changed"), 1);
        assert_eq!(distance("opne", "open"), 1);
        assert_eq!(distance("availble", "available"), 1);
        assert_eq!(distance("", "open"), 4);
        assert_eq!(distance("open", "open"), 0);
        assert_eq!(distance("kitten", "sitting"), 3);
        // A letter added, and a letter dropped, each one edit either way round.
        assert_eq!(distance("ope", "open"), 1);
        assert_eq!(distance("open", "ope"), 1);
    }

    #[test]
    fn only_a_near_word_is_suggested() {
        let states = ["open", "returned", "overdue"];
        assert_eq!(nearest("opne", states.into_iter()), Some("open"));
        assert_eq!(nearest("overdeu", states.into_iter()), Some("overdue"));
        assert_eq!(nearest("banana", states.into_iter()), None);
        // Five letters allow one edit, and `opxxn` is two from `open`.
        assert_eq!(nearest("opxxn", states.into_iter()), None);
        assert_eq!(hint("banana", states.into_iter()), "");
    }

    /// Two candidates as near as each other: the first in order, every time.
    #[test]
    fn a_tie_goes_to_the_first_in_order() {
        assert_eq!(nearest("bat", ["cat", "bad"].into_iter()), Some("bad"));
        assert_eq!(nearest("bat", ["bad", "cat"].into_iter()), Some("bad"));
    }

    #[test]
    fn the_field_is_followed_through_each_declared_hop() {
        let graph = graph();
        let path = |segments: &[&str]| -> Vec<String> {
            segments.iter().map(|segment| (*segment).to_owned()).collect()
        };
        let found = field_at(&graph, "Loan", &path(&["copy", "medium"])).expect("a field");
        assert_eq!((found.owner, found.field.name.as_str()), ("Copy", "medium"));
        assert!(field_at(&graph, "Loan", &path(&["copy", "tags", "count"])).is_none());
        assert!(field_at(&graph, "Loan", &path(&["nothing"])).is_none());
        assert!(field_at(&graph, "Ghost", &path(&["status"])).is_none());
        assert!(field_at(&graph, "Loan", &[]).is_none());
    }

    /// Two modules each declare an `Item`. A hop is read in the module of the
    /// entity declaring the field: bare, that module's own; `alias/X`, through
    /// its imports; and by name alone only when the set has one.
    #[test]
    fn a_hop_is_read_in_the_module_declaring_the_field() {
        let mut graph = SpecGraph::new("test");
        let at = |module: &str, name: &str, fields: Vec<EntityField>| {
            let mut node = entity(name, fields);
            node.module = module.to_owned();
            node.id = inspect_model::NodeId::new(module, NodeKind::Entity, name);
            node
        };
        graph.nodes.push(at("d", "Item", vec![field("stage", "draft | live", &["draft", "live"])]));
        graph.nodes.push(at(
            "q",
            "Item",
            vec![field("stage", "open | closed", &["open", "closed"])],
        ));
        graph.nodes.push(at(
            "q",
            "Issue",
            vec![
                field("item", "Item?", &[]),
                field("draft", "d/Item", &[]),
                field("page", "Page", &[]),
                field("ghost", "Ghost", &[]),
            ],
        ));
        graph.nodes.push(at("d", "Page", vec![field("stage", "draft | live", &["draft", "live"])]));
        graph.nodes.push(at("z", "Lone", vec![field("item", "Item", &[])]));
        let path = |segments: &[&str]| -> Vec<String> {
            segments.iter().map(|segment| (*segment).to_owned()).collect()
        };
        let module_of = |type_expr: &str, segments: &[&str]| {
            field_at(&graph, type_expr, &path(segments)).map(|found| found.module)
        };
        assert_eq!(module_of("q/Issue", &["item", "stage"]), Some("q"), "its own module's");
        assert_eq!(module_of("q/Issue", &["draft", "stage"]), Some("d"), "the module it names");
        assert_eq!(module_of("q/Issue", &["page", "stage"]), Some("d"), "the only `Page`");
        assert_eq!(module_of("q/Issue", &["ghost", "stage"]), None);
        // `z` declares no `Item`, and two modules do: which is a guess.
        assert_eq!(module_of("z/Lone", &["item", "stage"]), None);
    }

    #[test]
    fn a_word_the_field_declares_is_not_reported_and_one_it_does_not_is() {
        assert!(notes_for(&word("open"), "Loan", &["status"]).is_empty());
        let notes = notes_for(&word("print"), "Loan", &["status"]);
        assert_eq!(notes.len(), 1, "{notes:#?}");
        assert_eq!(notes[0].line, 7);
        assert_eq!(notes[0].verdict, Verdict::Unspecified);
        assert_eq!(
            notes[0].message,
            "`print` is no state of `Loan.status`, which declares open, returned"
        );
    }

    /// The enumeration a field is typed by declares its states.
    #[test]
    fn an_enumeration_typed_field_takes_the_enumerations_values() {
        assert!(notes_for(&word("audio"), "Copy", &["medium"]).is_empty());
        let notes = notes_for(&word("audoi"), "Loan", &["copy", "medium"]);
        assert_eq!(
            notes[0].message,
            "`audoi` is no state of `Copy.medium`, which declares print, audio — did you mean `audio`?"
        );
    }

    /// No field, or one declaring no states: any state the spec declares will
    /// do, and only a word nothing declares is reported.
    #[test]
    fn without_states_to_check_against_any_declared_state_will_do() {
        assert!(notes_for(&word("returned"), "Copy", &["tags"]).is_empty());
        assert!(notes_for(&word("audio"), "Ghost", &["x"]).is_empty());
        let notes = notes_for(&word("retruned"), "Copy", &["tags"]);
        assert_eq!(
            notes[0].message,
            "`retruned` is nobody in this journey and no state the spec declares — did you mean \
             `returned`?"
        );
    }

    /// Somebody bound, a path, a capitalised collection and a literal are not
    /// words, and every member of a set is one.
    #[test]
    fn only_unbound_lowercase_single_words_are_checked() {
        assert!(notes_for(&word("ada"), "Loan", &["status"]).is_empty());
        assert!(notes_for(&word("Loans"), "Loan", &["status"]).is_empty());
        let path = Term::Path(Path { root: "ada".to_owned(), segments: vec!["x".to_owned()] });
        assert!(notes_for(&path, "Loan", &["status"]).is_empty());
        let unbound = Term::Path(Path { root: "bob".to_owned(), segments: vec!["x".to_owned()] });
        assert!(notes_for(&unbound, "Loan", &["status"]).is_empty());
        assert!(
            notes_for(&Term::Literal(inspect_sim::Value::Int(1)), "Loan", &["status"]).is_empty()
        );
        let set = Term::Set(vec![word("open"), word("opne"), word("ada")]);
        let notes = notes_for(&set, "Loan", &["status"]);
        assert_eq!(notes.len(), 1, "{notes:#?}");
        assert!(notes[0].message.starts_with("`opne`"));
    }
}

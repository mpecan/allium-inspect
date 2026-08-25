//! What the parser upstream actually does, recorded rather than believed.
//!
//! Stipulation 5 says a library is preferred because an upstream shape change
//! becomes a compile error. That covers shapes. It does not cover a shape that
//! is *wrong* — the types still line up, and this crate reports the tree it was
//! handed, faithfully and uselessly.
//!
//! So a workaround for an upstream defect gets a test that asserts the defect
//! is still there. The day it is fixed, this fails, and the workaround it
//! guards becomes dead code loudly instead of quietly.
//!
//! ## The defect
//!
//! `allium-parser` 3.5.3 reads the module separator in
//! `exists membership/Membership{group: g}` as *division*:
//!
//! ```text
//! BinaryOp {
//!     left:  Exists(Ident "membership"),
//!     op:    Div,
//!     right: JoinLookup { entity: Ident "Membership", … },
//! }
//! ```
//!
//! The same qualified name in type position — `owner: membership/Member` —
//! parses correctly, so it is this path alone. Reported upstream; the recovery
//! lives in `eval::collections::misparsed_qualified_lookup`.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use allium_parser::ast::{BinaryOp, BlockItemKind, Decl, Expr};

/// The smallest spec that shows it.
const SOURCE: &str = "\
-- allium: 3
use \"./other.allium\" as other

entity Thing {
    owner: other/Person
    status: draft | live
}

rule Qualified {
    when: SomebodyActs(person)
    requires: exists other/Person{name: person}
    ensures: ThingHappened(person: person)
}

rule Bare {
    when: SomebodyElseActs(person)
    requires: exists Thing{owner: person}
    ensures: ThingHappened(person: person)
}
";

/// The `requires` expression of the named rule.
fn requires(rule: &str) -> Expr {
    let module = allium_parser::parse(SOURCE).module;

    for declaration in &module.declarations {
        let Decl::Block(block) = declaration else { continue };
        if block.name.as_ref().is_none_or(|name| name.name != rule) {
            continue;
        }
        for item in &block.items {
            if let BlockItemKind::Clause { keyword, value } = &item.kind
                && keyword == "requires"
            {
                return value.clone();
            }
        }
    }
    panic!("no `requires` on rule `{rule}`");
}

/// The bug. When this fails, the workaround can go.
#[test]
fn a_qualified_join_lookup_is_still_misparsed_as_a_division() {
    let expr = requires("Qualified");

    let Expr::BinaryOp { left, op, right, .. } = &expr else {
        panic!(
            "upstream now parses `exists other/Person{{…}}` as {expr:?}.\n\
             If this is an `Exists` over a `JoinLookup`, the defect is fixed: delete\n\
             `eval::collections::misparsed_qualified_lookup`, its call site in\n\
             `eval::eval`, and this test."
        );
    };

    assert_eq!(*op, BinaryOp::Div, "the separator is no longer read as division");
    assert!(matches!(left.as_ref(), Expr::Exists { .. }), "left is {left:?}");
    assert!(matches!(right.as_ref(), Expr::JoinLookup { .. }), "right is {right:?}");
}

/// The half that works, which is what makes the other half a defect rather
/// than a design.
#[test]
fn an_unqualified_join_lookup_parses_correctly() {
    let expr = requires("Bare");
    let Expr::Exists { operand, .. } = &expr else { panic!("expected an `exists`, got {expr:?}") };
    assert!(matches!(operand.as_ref(), Expr::JoinLookup { .. }), "operand is {operand:?}");
}

/// And the same qualified name in type position, which also works.
#[test]
fn a_qualified_name_in_type_position_parses_correctly() {
    let printed = format!("{:?}", allium_parser::parse(SOURCE).module);

    assert!(
        printed.contains("QualifiedName"),
        "`owner: other/Person` no longer produces a QualifiedName"
    );
}

//! The writing around a declaration, which is most of a specification.
//!
//! `friend-mesh` is thousands of lines of Allium and more than half of them are
//! comment. Most of its entities open with a paragraph saying why they exist;
//! many of its fields carry one; a hundred-odd rules have a `@guidance` block.
//! Everything the tool showed until now was the *what* — the
//! fields, the clauses, the states — and the *why* sat four lines above it in
//! the file, unread.
//!
//! Two different things, gathered two different ways:
//!
//! - a `--` comment written immediately above a declaration. The parser drops
//!   comments entirely, so this is sliced out of the source by walking back
//!   from the declaration's own span.
//! - the body of a `@guidance` block, which the parser *does* keep, already
//!   split into lines with the marker removed.
//!
//! Contiguity is what makes the first one safe. A comment block belongs to the
//! declaration directly beneath it, with no blank line between — which is the
//! convention every file in the set already follows, and the only reading that
//! cannot attach one construct's paragraph to another.

use serde_json::Value;

use crate::{
    graph::{NodeId, Prose, SpecGraph},
    ingest::json,
    span::Span,
};

/// Read the comment written immediately above the byte at `start`.
///
/// Returned in the order written, with the `--` marker and one following space
/// removed. A bare `--` line separates paragraphs and comes back as an empty
/// string, because that is what it is doing.
///
/// Byte offsets throughout: the parser counts bytes, and a spec with an em-dash
/// in a comment — which is every real spec — puts a character-counted answer on
/// the wrong line.
#[must_use]
pub fn note_above(source: &str, start: usize) -> Vec<String> {
    let Some(head) = source.get(..start.min(source.len())) else { return Vec::new() };
    // Everything above the line the declaration begins on. `rfind` gives the
    // newline that ends the previous line, so the declaration's own partial
    // line is dropped along with it.
    let above = match head.rfind('\n') {
        Some(at) => &head[..at],
        None => return Vec::new(),
    };

    let mut lines = Vec::new();
    // `split` rather than `lines`: a blank line contributes only its newline,
    // and `lines` drops the empty segment it leaves behind. That made a blank
    // line between a section banner and the declaration below it invisible, so
    // the first rule under `-- Rules` was given the file's table of contents as
    // its note.
    for line in above.split('\n').rev() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("--") else { break };
        // `---` and longer are a horizontal rule rather than a sentence, and a
        // reader does not want one in the middle of a paragraph.
        lines.push(rest.trim_start_matches('-').trim().to_owned());
    }
    lines.reverse();

    // A block of nothing but separators is a rule under the previous
    // declaration, not a note about this one.
    if lines.iter().all(String::is_empty) {
        return Vec::new();
    }
    trim_blank_edges(lines)
}

/// The body of every `@guidance` block among `block`'s items.
///
/// `@guarantee` is deliberately not read here: it is a *named* promise the
/// surface panel already shows as one, and repeating it as loose prose would
/// tell a reader there are two of them.
#[must_use]
pub fn guidance(block: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    for item in json::array(block, "items") {
        let Some(kind) = item.get("kind") else { continue };
        let Some(annotation) = kind.get("Annotation") else { continue };
        if json::string_or_empty(annotation, "kind") != "Guidance" {
            continue;
        }
        if !lines.is_empty() {
            // Two guidance blocks on one construct are two paragraphs.
            lines.push(String::new());
        }
        lines.extend(
            json::array(annotation, "body")
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .map(ToOwned::to_owned),
        );
    }
    trim_blank_edges(lines)
}

/// Give the node with `id` the writing that belongs to it.
pub fn attach(block: &Value, id: &NodeId, span: Option<Span>, source: &str, graph: &mut SpecGraph) {
    let prose = Prose {
        note: span.map(|span| note_above(source, span.start)).unwrap_or_default(),
        guidance: guidance(block),
    };
    if prose.is_empty() {
        return;
    }
    if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == *id) {
        node.prose = prose;
    }
}

/// Drop leading and trailing separators, which are spacing rather than text.
fn trim_blank_edges(mut lines: Vec<String>) -> Vec<String> {
    while lines.first().is_some_and(String::is_empty) {
        lines.remove(0);
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The byte offset of `needle` in `source`, which is what the parser reports.
    fn at(source: &str, needle: &str) -> usize {
        source.find(needle).expect("the fixture contains it")
    }

    #[test]
    fn a_comment_block_above_a_declaration_is_the_declarations_note() {
        let source = "\
-- What this device has said and has not yet got rid of.
--
-- The sender's half of delivery.
entity OutboxEntry {
";
        assert_eq!(
            note_above(source, at(source, "entity OutboxEntry")),
            [
                "What this device has said and has not yet got rid of.",
                "",
                "The sender's half of delivery.",
            ]
        );
    }

    #[test]
    fn a_blank_line_ends_the_block() {
        // Which is what stops one construct's paragraph being read as another's.
        // Every file in the set writes the note directly above the thing.
        let source = "\
-- About the entity above this one.

entity OutboxEntry {
";
        assert!(note_above(source, at(source, "entity")).is_empty());
    }

    #[test]
    fn the_previous_declarations_body_is_not_a_comment() {
        let source = "\
entity Hub {
    is_personal: serves_identity != null
}
entity OutboxEntry {
";
        assert!(note_above(source, at(source, "entity OutboxEntry")).is_empty());
    }

    #[test]
    fn a_section_banner_is_not_the_note_of_the_thing_below_it() {
        // A file's table of contents, separated from the first declaration by a
        // blank line. `-- Rules` is about the next hundred lines, not about the
        // rule that happens to come first.
        let source = "\
------------------------------------------------------------
-- Rules
------------------------------------------------------------

rule AdoptHub {
";
        assert!(note_above(source, at(source, "rule AdoptHub")).is_empty());
    }

    #[test]
    fn a_bare_marker_is_a_paragraph_break_rather_than_a_line() {
        let source = "-- one\n--\n-- two\nentity X {\n";
        assert_eq!(note_above(source, at(source, "entity")), ["one", "", "two"]);
    }

    #[test]
    fn a_rule_of_dashes_is_spacing_and_not_a_sentence() {
        // `-----------` is a divider somebody drew. Reading it as a line of
        // prose puts a row of nothing in the middle of a paragraph.
        let source = "-- one\n------------\n-- two\nentity X {\n";
        assert_eq!(note_above(source, at(source, "entity")), ["one", "", "two"]);
    }

    #[test]
    fn a_block_of_nothing_but_dividers_is_not_a_note() {
        let source = "------------\n------------\nentity X {\n";
        assert!(note_above(source, at(source, "entity")).is_empty());
    }

    #[test]
    fn the_separators_around_a_note_are_dropped() {
        let source = "--\n-- the note\n--\nentity X {\n";
        assert_eq!(note_above(source, at(source, "entity")), ["the note"]);
    }

    #[test]
    fn indentation_does_not_hide_a_field_note() {
        // Field comments are indented inside the entity, and the marker is what
        // identifies them rather than the column they start at.
        let source = "entity X {\n    -- why there is no expired\n    status: queued\n}\n";
        assert_eq!(note_above(source, at(source, "status:")), ["why there is no expired"]);
    }

    #[test]
    fn a_declaration_with_nothing_above_it_has_no_note() {
        let source = "entity X {\n}\n";
        assert!(note_above(source, 0).is_empty());
        assert!(note_above(source, at(source, "entity")).is_empty());
    }

    #[test]
    fn the_offset_is_counted_in_bytes() {
        // The parser counts bytes and every real spec has an em-dash in a
        // comment somewhere. A character-counted offset lands mid-block and
        // takes the wrong half of the paragraph.
        let source = "-- lost — while the screen showed it\n-- among the sent\nentity X {\n";
        let start = at(source, "entity X");
        assert!(source.is_char_boundary(start));
        assert_eq!(
            note_above(source, start),
            ["lost — while the screen showed it", "among the sent"]
        );
    }

    #[test]
    fn an_offset_past_the_end_is_survived() {
        assert!(note_above("entity X {\n", 9_000).is_empty());
    }

    fn annotated(items: Vec<serde_json::Value>) -> serde_json::Value {
        json!({ "kind": "Rule", "items": items })
    }

    fn annotation(kind: &str, body: &[&str]) -> serde_json::Value {
        json!({ "kind": { "Annotation": { "kind": kind, "name": null, "body": body } } })
    }

    #[test]
    fn a_guidance_block_is_read_as_written() {
        let block = annotated(vec![annotation("Guidance", &["A hub never pushes.", "It asks."])]);
        assert_eq!(guidance(&block), ["A hub never pushes.", "It asks."]);
    }

    #[test]
    fn two_guidance_blocks_are_two_paragraphs() {
        let block = annotated(vec![
            annotation("Guidance", &["first"]),
            annotation("Guidance", &["second"]),
        ]);
        assert_eq!(guidance(&block), ["first", "", "second"]);
    }

    #[test]
    fn a_guarantee_is_left_to_the_panel_that_names_it() {
        // `@guarantee` is a *named* promise the surface panel already shows as
        // one. Repeating it here as loose prose says there are two of them.
        let block = annotated(vec![annotation("Guarantee", &["nothing here is a choice"])]);
        assert!(guidance(&block).is_empty());
    }

    #[test]
    fn a_block_with_no_annotations_has_no_guidance() {
        assert!(guidance(&annotated(Vec::new())).is_empty());
        assert!(guidance(&json!({})).is_empty());
    }

    #[test]
    fn attaching_nothing_leaves_the_node_alone() {
        let mut graph = SpecGraph::new("v");
        graph.nodes.push(crate::graph::Node::new("m", crate::graph::NodeKind::Entity, "X"));
        let id = NodeId::new("m", crate::graph::NodeKind::Entity, "X");
        attach(&json!({}), &id, None, "", &mut graph);
        assert!(graph.nodes[0].prose.is_empty());
    }

    #[test]
    fn attaching_finds_the_node_by_id() {
        let mut graph = SpecGraph::new("v");
        graph.nodes.push(crate::graph::Node::new("m", crate::graph::NodeKind::Entity, "X"));
        let source = "-- why X exists\nentity X {\n";
        let id = NodeId::new("m", crate::graph::NodeKind::Entity, "X");
        let span = Span { start: at(source, "entity X"), end: source.len() };
        attach(
            &annotated(vec![annotation("Guidance", &["and how to use it"])]),
            &id,
            Some(span),
            source,
            &mut graph,
        );
        assert_eq!(graph.nodes[0].prose.note, ["why X exists"]);
        assert_eq!(graph.nodes[0].prose.guidance, ["and how to use it"]);
    }
}

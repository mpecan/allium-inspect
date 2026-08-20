//! Turning a span of spec source into something that fits on one line.
//!
//! Clause and expression text is sliced from the file rather than printed back
//! from the AST, so the reader sees what the author wrote. What the author wrote
//! also includes their comments, and an Allium spec is heavily commented — the
//! language is designed for prose to sit beside the declarations.
//!
//! That is right in the file and wrong in a panel. A surface's `exposes` block
//! can be forty lines of which thirty are an argument about why a field is not
//! shown; collapsing that verbatim produces a paragraph of run-together prose
//! where a list of field names belongs. So comments come out, and the full text
//! stays reachable through the construct's span in the source strip.

/// One line of `source`, with any trailing comment removed.
///
/// Allium comments run from `--` to the end of the line. A `--` inside a string
/// literal would be cut too; the consequence is a display line that stops early,
/// which is a great deal better than the alternative failure — and the untouched
/// text is one click away in the source strip either way.
fn without_comment(line: &str) -> &str {
    match line.find("--") {
        Some(at) => &line[..at],
        None => line,
    }
}

/// Squeeze a multi-line span onto one line, dropping comments and runs of
/// whitespace.
///
/// Multi-line clauses are common and their indentation is meaningful in the file
/// and meaningless in a 200-pixel node.
#[must_use]
pub fn one_line(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let content = without_comment(line).trim();
        if content.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&squeeze(content));
    }
    out
}

/// Collapse internal runs of whitespace to single spaces.
fn squeeze(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut spaced = false;
    for character in text.chars() {
        if character.is_whitespace() {
            if !spaced && !out.is_empty() {
                out.push(' ');
                spaced = true;
            }
        } else {
            out.push(character);
            spaced = false;
        }
    }
    out.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_line_is_left_alone() {
        assert_eq!(one_line("copy.status = available"), "copy.status = available");
    }

    #[test]
    fn a_multi_line_clause_becomes_one_line() {
        let text = "Loan.created(\n    copy: copy,\n    member: member\n)";
        assert_eq!(one_line(text), "Loan.created( copy: copy, member: member )");
    }

    #[test]
    fn a_trailing_comment_is_dropped() {
        assert_eq!(
            one_line("copy.status = available  -- only if it is on the shelf"),
            "copy.status = available"
        );
    }

    #[test]
    fn a_whole_line_of_comment_is_dropped() {
        // The case that motivated this. A surface's `exposes` block is mostly
        // prose in a real spec, and collapsing it verbatim buries the field
        // list in an essay.
        let text = "Loan.status\n-- The history itself, and the reduced set of it\n-- an archive can honestly show.\nMember.open_loan_count";
        assert_eq!(one_line(text), "Loan.status Member.open_loan_count");
    }

    #[test]
    fn a_comment_at_the_start_of_a_line_leaves_the_indentation_behind() {
        assert_eq!(one_line("    -- just a note\n    Loan.status"), "Loan.status");
    }

    #[test]
    fn text_that_is_only_comments_collapses_to_nothing() {
        assert_eq!(one_line("-- one\n-- two\n"), "");
    }

    #[test]
    fn whitespace_only_text_collapses_to_nothing() {
        assert_eq!(one_line("   \n\t \n"), "");
        assert_eq!(one_line(""), "");
    }

    #[test]
    fn runs_of_whitespace_become_single_spaces() {
        assert_eq!(one_line("a     b\t\tc"), "a b c");
    }

    #[test]
    fn blank_lines_do_not_leave_double_spaces() {
        assert_eq!(one_line("a\n\n\nb"), "a b");
    }

    #[test]
    fn a_comment_marker_inside_a_string_truncates_the_line() {
        // Documented rather than solved. Recognising it needs a lexer, the
        // consequence is a display line that stops early, and the untouched
        // text is one click away in the source strip.
        assert_eq!(one_line(r#"label = "a -- b""#), r#"label = "a"#);
    }

    #[test]
    fn an_arrow_is_not_mistaken_for_a_comment() {
        // `->` appears in projections and transitions and shares no prefix with
        // `--`, but a looser match on a single dash would eat both.
        assert_eq!(
            one_line("receipts where kind = read -> reporter"),
            "receipts where kind = read -> reporter"
        );
    }
}

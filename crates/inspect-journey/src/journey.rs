//! What a journey is, once it has been read.
//!
//! A journey is one or more actors' paths through acts the spec permits, over a
//! world that has been said to exist, asserting both what becomes true and what
//! each person can see. It is deliberately linear: a branch is a second
//! journey, which is what keeps a journey something you can read aloud.
//!
//! Everything here carries the line it was written on. A journey is a demand
//! made of a specification, and the first thing anyone does with the answer is
//! go back to the line that made the demand.

use inspect_sim::Value;

/// One journey, as written.
#[derive(Debug, Clone, PartialEq)]
pub struct Journey {
    pub name: String,
    /// What the journey is for, in the actor's terms. Never load-bearing.
    pub goal: Vec<String>,
    pub cast: Vec<Cast>,
    /// The ways this journey should be shown. Empty means "however it is".
    pub shows: Vec<Axis>,
    pub given: Vec<Given>,
    pub steps: Vec<Step>,
    /// The outcome, in words.
    pub ends: Vec<String>,
    pub line: usize,
}

/// One way this journey should be shown, and the answers it expects.
///
/// `theme: dark, light` — a question a picture can answer, and the answers
/// worth having. Declaring them turns evidence from something a harness
/// happens to produce into something the journey *asks for*: the panel offers
/// the control before any picture exists, and a tag outside the declaration is
/// reported rather than quietly becoming a second axis nobody meant.
///
/// A journey that declares nothing constrains nothing, and its axes are read
/// off whatever the pictures carry. Declaring one is opting in to being told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Axis {
    pub key: String,
    pub values: Vec<String>,
    pub line: usize,
}

/// One party to a journey, or something a step caught.
///
/// Instances rather than roles: two people of the same kind, with different
/// preconditions, is the ordinary case rather than the interesting one.
#[derive(Debug, Clone, PartialEq)]
pub struct Cast {
    pub name: String,
    /// The construct as written, which may be qualified: `identity/Identity`.
    pub type_expr: String,
    pub line: usize,
}

/// The world before anything happens.
#[derive(Debug, Clone, PartialEq)]
pub enum Given {
    /// `note: messaging/Message { author: ada, body: "…" }`
    Instance { name: String, type_expr: String, fields: Vec<(String, Term)>, line: usize },
    /// `ada.status = active`
    Assign { path: Path, value: Term, line: usize },
}

impl Given {
    #[must_use]
    pub fn line(&self) -> usize {
        match self {
            Given::Instance { line, .. } | Given::Assign { line, .. } => *line,
        }
    }
}

/// One numbered step, and the clauses under it.
///
/// The number is required and never renumbered. `friend-mesh` refers to journey
/// steps by number from other documents, and a number that moves when somebody
/// inserts a step is a citation that quietly starts pointing elsewhere.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub number: u32,
    /// The step as a sentence in a person's terms.
    pub title: String,
    pub clauses: Vec<Clause>,
    pub line: usize,
}

/// One line under a step.
#[derive(Debug, Clone, PartialEq)]
pub enum Clause {
    /// `ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan`
    Does {
        actor: String,
        trigger: String,
        arguments: Vec<Term>,
        surface: String,
        /// What the step caught, named by the writer rather than numbered.
        creating: Option<Cast>,
        line: usize,
    },
    /// `after 21.days`
    After { duration: Value, text: String, line: usize },
    /// `then loan.status = returned`
    Then { assertion: Assertion, line: usize },
    /// `ada sees loan.status on MemberShelf`, or `cannot see`.
    Sees { actor: String, path: Path, surface: String, negated: bool, line: usize },
    /// `stipulate ada.is_at_limit = false`
    Stipulate { path: Path, value: Term, line: usize },
}

impl Clause {
    #[must_use]
    pub fn line(&self) -> usize {
        match self {
            Clause::Does { line, .. }
            | Clause::After { line, .. }
            | Clause::Then { line, .. }
            | Clause::Sees { line, .. }
            | Clause::Stipulate { line, .. } => *line,
        }
    }
}

/// What must be true after the step above it.
///
/// Deliberately small. Anything an assertion cannot say is something the *spec*
/// should be saying — as an invariant, which is checked on every step anyway —
/// and a second, larger grammar for Allium expressions in this repository would
/// be a second implementation of the language that could disagree with the
/// first.
#[derive(Debug, Clone, PartialEq)]
pub enum Assertion {
    /// `loan.status = open`, `intent.targets.count > 0`
    Compare { left: Path, operator: Comparison, right: Term },
    /// `his_phone in entry.awaiting`
    Within { needle: Term, haystack: Path },
    /// `BorrowCopy fires`, `BorrowCopy does not fire`
    Fires { rule: String, negated: bool },
    /// `loan exists`, `loan does not exist`
    Exists { path: Path, negated: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl Comparison {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Comparison::Equal => "=",
            Comparison::NotEqual => "!=",
            Comparison::Less => "<",
            Comparison::LessOrEqual => "<=",
            Comparison::Greater => ">",
            Comparison::GreaterOrEqual => ">=",
        }
    }
}

/// A name and the fields walked from it: `loan.copy.status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    /// A cast name, a `given` instance, something a step caught, or `config`.
    pub root: String,
    pub segments: Vec<String>,
}

impl Path {
    #[must_use]
    pub fn as_written(&self) -> String {
        if self.segments.is_empty() {
            return self.root.clone();
        }
        format!("{}.{}", self.root, self.segments.join("."))
    }
}

/// A value, or somewhere to read one from.
#[derive(Debug, Clone, PartialEq)]
pub enum Term {
    Literal(Value),
    Path(Path),
    /// `{note, other}` — a set of things already named.
    Set(Vec<Term>),
}

impl Term {
    #[must_use]
    pub fn as_written(&self) -> String {
        match self {
            Term::Literal(value) => value.render(),
            Term::Path(path) => path.as_written(),
            Term::Set(items) => {
                let inside: Vec<String> = items.iter().map(Term::as_written).collect();
                format!("{{{}}}", inside.join(", "))
            }
        }
    }
}

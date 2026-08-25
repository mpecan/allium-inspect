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
    ///
    /// A call as well as a path, because a surface may expose one:
    /// `exposes: announces_reads(owner)` shows a person whether they announce
    /// reads, and there is no field to name for it.
    Sees { actor: String, subject: Subject, surface: String, negated: bool, line: usize },
    /// `stipulate ada.is_at_limit = false`, or `stipulate may_invite(g, a) = true`
    Stipulate { subject: Subject, value: Term, line: usize },
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

/// What a `stipulate` or a `sees` line is about.
///
/// A path writes into the world. A **call** cannot: `may_invite(group, issuer)`
/// is a function the specification names and never defines, deliberately —
/// the policy has not been decided — so there is nothing to write to and
/// nothing this simulator could ever work out. It stays undecided forever, and
/// forever is what `stipulate` is for.
#[derive(Debug, Clone, PartialEq)]
pub enum Subject {
    /// `ada.is_at_limit = false`
    Path(Path),
    /// `may_invite(chat, she) = true`
    Call { name: String, arguments: Vec<Term> },
}

impl Subject {
    #[must_use]
    pub fn as_written(&self) -> String {
        match self {
            Subject::Path(path) => path.as_written(),
            Subject::Call { name, arguments } => {
                let inside: Vec<String> = arguments.iter().map(Term::as_written).collect();
                format!("{name}({})", inside.join(", "))
            }
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
    /// `now`, `now + 1.day`, `now - 2.hours`.
    ///
    /// The only way a journey can name a *moment*. Every other literal form is
    /// absolute, and a timestamp written absolutely is unreadable and wrong the
    /// day after it is written: what a journey means is "this expires an hour
    /// from where the clock is", not "this expires at 2038-01-19T03:14:07Z".
    ///
    /// It is also the form the specifications need. A rule guarded by
    /// `requires: invitation.expires_at > now` cannot be reached from a world
    /// where `expires_at` holds an integer — the comparison is between an
    /// integer and a timestamp and is refused, correctly — so before this
    /// existed, every rule with a deadline in it was unreachable in a walk.
    Clock {
        /// Milliseconds either side of the world's clock. Zero for bare `now`.
        offset: i64,
        /// As the author wrote it, for the report.
        written: String,
    },
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
            Term::Clock { written, .. } => written.clone(),
        }
    }
}

//! Turning a list of paths into the two sets the command needs.
//!
//! One list in, two out. `allium` takes a bare `<path>...` and works out what
//! it is looking at from the file, so this does the same: a `.allium` file is a
//! spec, a `.journey` file is a journey, and a directory is searched
//! recursively for both. That is what lets `allium-journey walk specs/` work
//! when the journeys live under the spec set, and
//! `allium-journey walk specs/ journeys/` when they do not.
//!
//! Recursive, unlike `allium-inspect`, which searches one level on purpose so
//! that pointing it at a checkout does not sweep in a `target` directory. A
//! command run against named paths is being told where to look, and allium's
//! own commands recurse.

use std::path::{Path, PathBuf};

/// The spec files and journey files among `paths`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub specs: Vec<PathBuf>,
    pub journeys: Vec<PathBuf>,
}

impl Found {
    /// Whether there is enough here to say anything.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        !self.specs.is_empty() && !self.journeys.is_empty()
    }

    /// What is missing, for the message that says so.
    #[must_use]
    pub fn missing(&self) -> &'static str {
        match (self.specs.is_empty(), self.journeys.is_empty()) {
            (true, true) => "no .allium specs and no .journey files",
            (true, false) => "no .allium specs",
            (false, true) => "no .journey files",
            (false, false) => "nothing",
        }
    }
}

/// Every spec and journey under `paths`, sorted.
///
/// Sorted because ingestion order decides nothing but the answer is compared
/// between runs, and an unsorted directory read is a different order on a
/// different machine.
#[must_use]
pub fn resolve(paths: &[PathBuf]) -> Found {
    let mut found = Found::default();
    for path in paths {
        collect(path, &mut found);
    }
    found.specs.sort();
    found.specs.dedup();
    found.journeys.sort();
    found.journeys.dedup();
    found
}

fn collect(path: &Path, into: &mut Found) {
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else { return };
        for entry in entries.flatten() {
            collect(&entry.path(), into);
        }
        return;
    }
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("allium") => into.specs.push(path.to_owned()),
        Some("journey") => into.journeys.push(path.to_owned()),
        // A path named outright that is neither is not an error: `walk .` is
        // an ordinary thing to type, and most of what is under it is neither.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree under a directory that removes itself.
    struct Tree(PathBuf);

    impl Tree {
        fn of(files: &[&str]) -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let root = std::env::temp_dir().join(format!(
                "allium-journey-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            for file in files {
                let path = root.join(file);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).expect("a directory");
                }
                std::fs::write(&path, "").expect("a file");
            }
            Self(root)
        }

        fn path(&self) -> PathBuf {
            self.0.clone()
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|path| path.file_name().unwrap_or_default().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_directory_is_searched_recursively() {
        // What `allium` does, and the reason this differs from
        // `allium-inspect`: a command given a path is being told where to look.
        let tree = Tree::of(&["a.allium", "deep/nested/b.journey", "deep/c.allium"]);
        let found = resolve(&[tree.path()]);
        assert_eq!(names(&found.specs), ["a.allium", "c.allium"]);
        assert_eq!(names(&found.journeys), ["b.journey"]);
    }

    #[test]
    fn specs_and_journeys_are_told_apart_by_extension() {
        // One `<path>...` list rather than two flags, so the two can live in
        // the same directory or in different ones without the caller saying
        // which is which.
        let tree = Tree::of(&["together/x.allium", "together/x.journey"]);
        let found = resolve(&[tree.path()]);
        assert_eq!(names(&found.specs), ["x.allium"]);
        assert_eq!(names(&found.journeys), ["x.journey"]);
    }

    #[test]
    fn a_file_named_outright_is_taken_whatever_it_sits_beside() {
        let tree = Tree::of(&["a.allium", "b.journey"]);
        let found = resolve(&[tree.path().join("a.allium"), tree.path().join("b.journey")]);
        assert!(found.is_usable());
    }

    #[test]
    fn anything_that_is_neither_is_passed_over_rather_than_refused() {
        // `walk .` is an ordinary thing to type and most of what is under a
        // checkout is neither a spec nor a journey.
        let tree = Tree::of(&["a.allium", "b.journey", "README.md", "target/debug/thing"]);
        let found = resolve(&[tree.path()]);
        assert_eq!(found.specs.len(), 1);
        assert_eq!(found.journeys.len(), 1);
    }

    #[test]
    fn the_same_path_twice_is_read_once() {
        let tree = Tree::of(&["a.allium", "b.journey"]);
        let found = resolve(&[tree.path(), tree.path().join("a.allium")]);
        assert_eq!(found.specs.len(), 1, "{:?}", found.specs);
    }

    #[test]
    fn the_order_does_not_depend_on_the_filesystem() {
        // Every run is compared against the last one; an unsorted directory
        // read is a different order on a different machine.
        let tree = Tree::of(&["z.allium", "a.allium", "m.allium", "j.journey"]);
        assert_eq!(names(&resolve(&[tree.path()]).specs), ["a.allium", "m.allium", "z.allium"]);
    }

    #[test]
    fn half_an_answer_is_not_usable_and_says_which_half() {
        let specs = Tree::of(&["a.allium"]);
        let only_specs = resolve(&[specs.path()]);
        assert!(!only_specs.is_usable());
        assert_eq!(only_specs.missing(), "no .journey files");

        let journeys = Tree::of(&["b.journey"]);
        let only_journeys = resolve(&[journeys.path()]);
        assert!(!only_journeys.is_usable());
        assert_eq!(only_journeys.missing(), "no .allium specs");

        assert_eq!(resolve(&[]).missing(), "no .allium specs and no .journey files");
    }

    #[test]
    fn a_path_that_is_not_there_is_not_a_crash() {
        let found = resolve(&[PathBuf::from("/nowhere/at/all")]);
        assert!(!found.is_usable());
    }
}

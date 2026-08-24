//! Reading a run's pictures off the disk, and serving them.
//!
//! Two halves, and both are small on purpose. Reading is the same job
//! `allium-journey evidence` does, done again here because the browser reloads
//! when a file changes and a manifest sealed after the last reload should
//! appear without restarting anything. Serving is one route with one rule.
//!
//! **The manifest is the allowlist.** A request names a frame by its image
//! name, and the only names that resolve are ones a sealed manifest already
//! carries. Nothing a client sends is ever joined to a path — which is the
//! whole of the traversal defence, and why it is a lookup rather than a
//! sanitiser. A sanitiser has to be right about every spelling of `..`; a
//! lookup has to be right about nothing.

use std::path::{Path, PathBuf};

use inspect_journey::{Claim, Manifest, claims};

/// What a run left behind, and what the code says about it.
///
/// Both optional. A spec set with no pictures and no markers is the ordinary
/// starting state, and the panel says so rather than hiding the section.
#[derive(Debug, Default, Clone)]
pub struct Evidence {
    pub manifest: Option<Manifest>,
    pub claims: Vec<Claim>,
    /// Where the pictures are, for the route that serves them.
    pub directory: Option<PathBuf>,
}

impl Evidence {
    /// Read a sealed manifest and scan source for markers.
    ///
    /// Anything unreadable is *nothing* rather than an error: a browser that
    /// refused to start because a manifest was mid-write would be worse than
    /// one whose evidence section is empty for a second. The command is where
    /// a malformed manifest is a failure — see `allium-journey evidence check`.
    #[must_use]
    pub fn read(directory: Option<&Path>, code: &[PathBuf]) -> Self {
        let manifest = directory
            .map(|dir| dir.join("manifest.json"))
            .filter(|path| path.is_file())
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<Manifest>(&text).ok())
            .filter(|manifest| manifest.version == inspect_journey::evidence::manifest::VERSION);

        Self {
            manifest,
            claims: claims(&sources(code)),
            directory: directory.map(Path::to_path_buf),
        }
    }

    /// The file behind an image name, if a sealed frame names it.
    #[must_use]
    pub fn picture(&self, image: &str) -> Option<PathBuf> {
        let directory = self.directory.as_ref()?;
        let manifest = self.manifest.as_ref()?;
        manifest
            .frames
            .iter()
            .any(|frame| frame.image == image)
            .then(|| directory.join(image))
            .filter(|path| path.is_file())
    }
}

/// Directories whose contents nobody wrote.
const GENERATED: [&str; 4] = ["target", "node_modules", ".git", "dist"];

/// A backstop for a link that points at one of its own parents.
const DEEPEST: usize = 64;

fn sources(paths: &[PathBuf]) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for path in paths {
        gather(path, &mut files, 0);
    }
    files.sort();
    files.dedup();
    files
}

fn gather(path: &Path, into: &mut Vec<(String, String)>, depth: usize) {
    if path.is_dir() {
        if depth >= DEEPEST {
            return;
        }
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        if GENERATED.contains(&name) {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else { return };
        for entry in entries.flatten() {
            gather(&entry.path(), into, depth + 1);
        }
        return;
    }
    if let Ok(text) = std::fs::read_to_string(path) {
        into.push((path.display().to_string(), text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("inspect-server-evidence-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch directory");
        root
    }

    fn manifest_naming(image: &str) -> String {
        format!(
            r#"{{"version":1,"sealed_at":"now","walk":null,"frames":[{{"step":"R.1","image":"{image}","caption":null,"passed":true,"taken_at":"t","source":null,"said":"1. a step","tags":{{"theme":"dark"}}}}]}}"#
        )
    }

    #[test]
    fn reads_a_sealed_manifest_and_the_markers_beside_it() {
        let root = scratch("read");
        std::fs::write(root.join("manifest.json"), manifest_naming("01.png")).expect("a manifest");
        std::fs::write(root.join("walk.ts"), "// journey: R.1\n").expect("a marked file");

        let evidence = Evidence::read(Some(&root), std::slice::from_ref(&root));

        assert_eq!(evidence.manifest.map(|m| m.frames.len()), Some(1));
        assert_eq!(evidence.claims.len(), 1);
    }

    #[test]
    fn nothing_at_all_is_not_an_error() {
        let evidence = Evidence::read(None, &[]);
        assert!(evidence.manifest.is_none());
        assert!(evidence.claims.is_empty());
        assert!(evidence.picture("01.png").is_none());
    }

    /// A manifest half written by a seal that is still running.
    #[test]
    fn a_manifest_that_will_not_parse_is_nothing_rather_than_a_failure() {
        let root = scratch("torn");
        std::fs::write(root.join("manifest.json"), "{\"version\":1,\"frames\":[").expect("half");

        assert!(Evidence::read(Some(&root), &[]).manifest.is_none());
    }

    #[test]
    fn a_manifest_from_a_version_this_does_not_read_is_ignored() {
        let root = scratch("version");
        std::fs::write(
            root.join("manifest.json"),
            r#"{"version":99,"sealed_at":"now","walk":null,"frames":[]}"#,
        )
        .expect("a manifest from the future");

        assert!(Evidence::read(Some(&root), &[]).manifest.is_none());
    }

    #[test]
    fn a_picture_a_frame_names_resolves() {
        let root = scratch("serve");
        std::fs::write(root.join("manifest.json"), manifest_naming("01.png")).expect("a manifest");
        std::fs::write(root.join("01.png"), "a picture").expect("a picture");

        assert_eq!(Evidence::read(Some(&root), &[]).picture("01.png"), Some(root.join("01.png")));
    }

    /// The whole of the traversal defence, stated as the test that would catch
    /// its removal: a name no frame carries resolves to nothing, whatever it
    /// spells.
    #[test]
    fn a_name_no_frame_carries_resolves_to_nothing() {
        let root = scratch("traversal");
        std::fs::write(root.join("manifest.json"), manifest_naming("01.png")).expect("a manifest");
        std::fs::write(root.join("01.png"), "a picture").expect("a picture");
        std::fs::write(root.join("secret.txt"), "not for you").expect("a neighbour");

        let evidence = Evidence::read(Some(&root), &[]);
        for name in [
            "secret.txt",
            "../../../etc/passwd",
            "..%2F..%2Fetc%2Fpasswd",
            "./01.png",
            "01.png/../secret.txt",
            "/etc/passwd",
            "",
        ] {
            assert!(evidence.picture(name).is_none(), "`{name}` must not resolve");
        }
    }

    /// A frame naming a picture somebody has since deleted.
    #[test]
    fn a_named_picture_that_is_gone_resolves_to_nothing() {
        let root = scratch("gone");
        std::fs::write(root.join("manifest.json"), manifest_naming("01.png")).expect("a manifest");

        assert!(Evidence::read(Some(&root), &[]).picture("01.png").is_none());
    }

    #[test]
    fn a_generated_directory_is_not_scanned_for_markers() {
        let root = scratch("generated");
        let generated = root.join("node_modules");
        std::fs::create_dir_all(&generated).expect("a generated directory");
        std::fs::write(generated.join("dep.js"), "// journey: R.1\n").expect("a file");

        assert!(Evidence::read(None, std::slice::from_ref(&root)).claims.is_empty());
    }
}

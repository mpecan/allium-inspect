//! Re-reading the spec set when it changes on disk.
//!
//! The normal way to use this tool is with the spec open in an editor beside
//! it: change a rule, look at what moved. Making that require a restart would
//! turn a two-second loop into a ten-second one, which is the difference
//! between a tool you keep open and one you close.
//!
//! Two decisions worth stating.
//!
//! *A failed reload does not replace what is on screen.* If the spec stops
//! parsing mid-edit — which it does, constantly, because that is what typing is
//! — the graph you were reading stays put and the error is recorded alongside
//! it. Blanking the screen on every keystroke would make the tool unusable for
//! the activity it exists for.
//!
//! *Changes are coalesced.* One save produces several filesystem events, and
//! running the CLI four times over the same edit is three runs of wasted work
//! and three redraws the reader did not ask for.

use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

use inspect_model::ProcessRunner;
use inspect_server::{AppState, Inspection};
use notify::{Event, RecursiveMode, Watcher};

/// How long to wait for a burst of filesystem events to settle.
///
/// Long enough that one save is one reload, short enough that the redraw still
/// feels like a consequence of hitting save.
const SETTLE: Duration = Duration::from_millis(150);

/// Watch `paths` and rebuild `state` whenever any of them changes.
///
/// Runs until the process ends. Returns an error only if a watcher could not be
/// established at all — after that, failures are reported through the state and
/// the loop keeps going, because a spec that stops parsing is the expected case
/// rather than a reason to give up watching.
///
/// # Errors
///
/// Returns a message when no watcher could be set up.
pub fn watch(
    paths: Vec<PathBuf>,
    allium: PathBuf,
    state: AppState,
) -> Result<std::thread::JoinHandle<()>, String> {
    let (sender, receiver) = mpsc::channel::<()>();

    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        // Any change to a watched file is a reason to re-read the whole set:
        // one module's edit routinely changes another's graph through an
        // import, so a partial reload would show a half-updated picture.
        if event.is_ok() {
            let _ = sender.send(());
        }
    })
    .map_err(|error| format!("could not watch for changes: {error}"))?;

    // The directory, not the file. Editors write a new file and rename it over
    // the old one, which destroys the inode a file watch is attached to — the
    // watch then stops firing after the first save, silently.
    let mut watched = Vec::new();
    for path in &paths {
        let Some(directory) = path.parent() else { continue };
        if watched.contains(&directory.to_path_buf()) {
            continue;
        }
        watcher
            .watch(directory, RecursiveMode::NonRecursive)
            .map_err(|error| format!("could not watch {}: {error}", directory.display()))?;
        watched.push(directory.to_path_buf());
    }

    let handle = std::thread::spawn(move || {
        // The watcher is moved in so it lives as long as the loop; dropping it
        // stops delivery, and a watcher that has quietly stopped is worse than
        // one that was never started.
        let _watcher = watcher;
        let runner = ProcessRunner::new(&allium);

        while receiver.recv().is_ok() {
            // Drain the rest of the burst. One save is several events, and
            // running the CLI once per event is wasted work and a flicker.
            let deadline = Instant::now() + SETTLE;
            while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                if receiver.recv_timeout(remaining).is_err() {
                    break;
                }
            }

            match Inspection::build(&runner, &paths) {
                Ok(inspection) => {
                    let graph = &inspection.graph;
                    println!(
                        "reloaded  {} constructs, {} edges",
                        graph.nodes.len(),
                        graph.edges.len()
                    );
                    state.replace(inspection);
                }
                Err(error) => {
                    // The spec stopped parsing. Keep showing the last good
                    // picture and record why the new one is not there.
                    eprintln!("reload failed: {error}");
                    state.set_error(Some(error.to_string()));
                }
            }
        }
    });

    Ok(handle)
}

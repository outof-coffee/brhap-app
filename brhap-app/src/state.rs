//! What the app knows, and the questions it can answer about itself.
//!
//! Nothing here builds widgets. The view modules ask this for a value and turn
//! it into an element, so the two can change independently.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use brhap_core::{Core, Event, LaunchOptions, LaunchPlan, Overrides, Profiles, Resolved, Snapshot};
use iced::Task;

use crate::events::Outbox;
use crate::message::Message;
use crate::work;

pub(crate) struct Brhap {
    pub(crate) core: Arc<Core>,
    pub(crate) outbox: Outbox,

    /// Installed mods, in the order the core returned them.
    pub(crate) mod_ids: Vec<String>,
    /// Everything known about any id, installed or merely referenced.
    pub(crate) items: BTreeMap<String, Resolved>,
    /// Which mods to launch with. Order comes from `mod_ids`, not from here.
    pub(crate) selected: BTreeSet<String>,
    /// Ids queued or running inside a staggered refetch.
    pub(crate) pending: BTreeSet<String>,
    /// Last failure per id, cleared when that id is retried.
    pub(crate) errors: BTreeMap<String, String>,
    /// Startup parameters, owned here and handed to the core on launch.
    pub(crate) options: LaunchOptions,
    /// Per-mod replacement directories, keyed by workshop id.
    pub(crate) overrides: Overrides,
    /// What a launch would do, as the core describes it. Recomputed whenever
    /// an input changes, so it can never disagree with what launching runs.
    pub(crate) plan: Option<LaunchPlan>,
    pub(crate) show_preview: bool,
    pub(crate) api_available: bool,
    pub(crate) load_error: String,
    /// True while a refresh is in flight, which disables the toolbar.
    pub(crate) rescanning: bool,
    pub(crate) rescan_note: String,

    /// Mirrors the status line the web UI shows, so the two read the same.
    pub(crate) status: String,
    pub(crate) pid: Option<u32>,
    /// Comes from the session stream, so an exit nobody asked for still lands.
    pub(crate) running: bool,
    pub(crate) launch_error: String,

    pub(crate) screen: Screen,
    pub(crate) store: Profiles,
    pub(crate) profile_name: String,
    pub(crate) profile_error: String,
    /// What the last applied profile did, shown in the header.
    pub(crate) apply_note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Screen {
    Launch,
    Profiles,
}

/// What a row has to say about itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Pending,
    Failed,
    Unknown,
    Ready,
    Unmet,
    /// Nothing worth saying: unselected, with requirements already known.
    Quiet,
}

/// One line of the mod table. `table()` hands the view closures an owned value,
/// so everything a cell needs is gathered here first.
#[derive(Clone)]
pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) selected: bool,
    pub(crate) status: Status,
    pub(crate) over: Option<std::path::PathBuf>,
}

/// One startup parameter.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Flag {
    NoSplash,
    SkipIntro,
    EmptyWorld,
    IntelMode,
    SteamOverlay,
}

impl Flag {
    /// What the flag puts on the command line, or what it does when it does
    /// not put anything there.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::NoSplash => "-noSplash",
            Self::SkipIntro => "-skipIntro",
            Self::EmptyWorld => "-world=empty",
            Self::IntelMode => "Intel Mode",
            Self::SteamOverlay => "Steam Overlay",
        }
    }

    pub(crate) fn get(self, options: &LaunchOptions) -> bool {
        match self {
            Self::NoSplash => options.no_splash,
            Self::SkipIntro => options.skip_intro,
            Self::EmptyWorld => options.empty_world,
            Self::IntelMode => options.intel_mode,
            Self::SteamOverlay => options.steam_overlay,
        }
    }

    pub(crate) fn set(self, options: &mut LaunchOptions, value: bool) {
        match self {
            Self::NoSplash => options.no_splash = value,
            Self::SkipIntro => options.skip_intro = value,
            Self::EmptyWorld => options.empty_world = value,
            Self::IntelMode => options.intel_mode = value,
            Self::SteamOverlay => options.steam_overlay = value,
        }
    }
}

impl Brhap {
    pub(crate) fn new() -> Self {
        let outbox = Outbox::default();
        // The listener is built before the subscription exists; see events.rs.
        let core = Arc::new(Core::new(outbox.listener()));

        Self {
            core,
            outbox,
            mod_ids: Vec::new(),
            items: BTreeMap::new(),
            selected: BTreeSet::new(),
            pending: BTreeSet::new(),
            errors: BTreeMap::new(),
            options: LaunchOptions::default(),
            overrides: Overrides::new(),
            plan: None,
            show_preview: false,
            api_available: false,
            load_error: String::new(),
            rescanning: false,
            rescan_note: String::new(),
            status: "idle".to_string(),
            pid: None,
            running: false,
            launch_error: String::new(),
            screen: Screen::Launch,
            store: Profiles::default(),
            profile_name: String::new(),
            profile_error: String::new(),
            apply_note: String::new(),
        }
    }

    /// Follows `absorb` in web/src/App.svelte:207-249: referenced items are
    /// folded in alongside installed ones, so a dependency the user does not
    /// have installed can still be named.
    pub(crate) fn absorb_snapshot(&mut self, snapshot: Snapshot) {
        self.mod_ids = snapshot.mods.iter().map(|item| item.id.clone()).collect();
        self.api_available = snapshot.api_available;
        for item in snapshot.mods.into_iter().chain(snapshot.referenced) {
            self.items.insert(item.id.clone(), item);
        }
    }

    /// Take a fresh snapshot off the UI thread.
    pub(crate) fn reload(&self) -> Task<Message> {
        let core = Arc::clone(&self.core);
        Task::perform(work::blocking(move || core.snapshot()), Message::Loaded)
    }

    /// How many installed mods are selected. Counted against `mod_ids` rather
    /// than the set itself, so an id left over from an uninstalled mod does not
    /// inflate the number.
    pub(crate) fn selected_count(&self) -> usize {
        self.mod_ids.iter().filter(|id| self.selected.contains(*id)).count()
    }

    /// The selection in `mod_ids` order, which is the order `-mod=` gets.
    pub(crate) fn selected_ids(&self) -> Vec<String> {
        self.mod_ids.iter().filter(|id| self.selected.contains(*id)).cloned().collect()
    }

    /// Rebuild the plan from the current inputs.
    ///
    /// The core owns the argument shape, so the preview always reflects what a
    /// launch would actually run rather than a second guess at it. This is pure
    /// computation, no filesystem or network, so it runs inline.
    pub(crate) fn replan(&mut self) {
        let ids = self.selected_ids();
        self.plan = Some(self.core.preview(&ids, self.options, &self.overrides));
    }

    /// Replace what is known rather than merging into it.
    ///
    /// Follows `absorbSnapshot` in web/src/App.svelte:239-249. A rescan is the
    /// one moment a mod can vanish, so anything no longer installed drops out
    /// of the selection and the item table is rebuilt rather than added to.
    pub(crate) fn resync(&mut self, snapshot: Snapshot) {
        let known: BTreeSet<String> = snapshot.mods.iter().map(|item| item.id.clone()).collect();
        self.selected.retain(|id| known.contains(id));
        self.items.clear();
        self.absorb_snapshot(snapshot);
    }

    /// Current knowledge about an id, without claiming anything unresolved.
    pub(crate) fn item(&self, id: &str) -> Resolved {
        self.items.get(id).cloned().unwrap_or_else(|| Resolved {
            id: id.to_string(),
            name: None,
            installed: false,
            requires: None,
            source: brhap_core::Source::Unknown,
            fetched_at: String::new(),
        })
    }

    /// What one row reports. First match wins, so a queued refetch outranks a
    /// stale failure, and a failure outranks not knowing.
    ///
    /// Ready and Unmet describe whether this launch will work, which is only
    /// meaningful once the mod is selected. An unselected row stays quiet
    /// rather than colouring a list nobody has touched yet.
    pub(crate) fn status_of(&self, id: &str) -> Status {
        if self.pending.contains(id) {
            return Status::Pending;
        }
        if self.errors.contains_key(id) {
            return Status::Failed;
        }

        let item = self.item(id);
        let Some(requires) = item.requires.as_deref() else {
            return Status::Unknown;
        };
        if !self.selected.contains(id) {
            return Status::Quiet;
        }

        if requires.iter().all(|child| self.selected.contains(child)) {
            Status::Ready
        } else {
            Status::Unmet
        }
    }

    /// The table's rows, in `mod_ids` order.
    pub(crate) fn rows(&self) -> Vec<Row> {
        self.mod_ids
            .iter()
            .map(|id| Row {
                id: id.clone(),
                name: self.item(id).name.unwrap_or_else(|| id.clone()),
                selected: self.selected.contains(id),
                status: self.status_of(id),
                over: self.overrides.get(id).cloned(),
            })
            .collect()
    }

    /// Requirements of the selected mods that will not be loaded as things
    /// stand, following web/src/App.svelte:180-194.
    ///
    /// Only reports what has actually been resolved. An unresolved mod says
    /// nothing rather than claiming it has no requirements.
    pub(crate) fn unmet(&self) -> Vec<String> {
        let mut list = Vec::new();

        for id in self.mod_ids.iter().filter(|id| self.selected.contains(*id)) {
            let parent = self.item(id);
            let Some(requires) = parent.requires.as_deref() else {
                continue;
            };
            let parent_name = parent.name.clone().unwrap_or_else(|| id.clone());

            for child_id in requires.iter().filter(|child| !self.selected.contains(*child)) {
                let child = self.item(child_id);
                let reason = if child.installed {
                    "installed but not selected"
                } else {
                    "not installed"
                };
                let child_name = child.name.unwrap_or_else(|| child_id.clone());
                list.push(format!("{parent_name} requires {child_name}, {reason}"));
            }
        }

        list
    }

    /// Status wording follows web/src/App.svelte:65-82 so the two frontends
    /// describe the same run the same way.
    pub(crate) fn absorb_event(&mut self, event: Event) {
        match event {
            Event::State { running, pid } => {
                self.running = running;
                self.pid = pid;
                if !running && self.status.starts_with("running") {
                    self.status = "idle".to_string();
                }
            }
            Event::Linked { removed, created } => {
                self.status = format!("linked {created} mod(s), removed {removed} stale link(s)");
            }
            Event::Spawned { pid } => self.status = format!("running as pid {pid}"),
            Event::Exited { code } => {
                self.status = match code {
                    Some(code) => format!("exited with code {code}"),
                    None => "exited with code none".to_string(),
                };
            }
            Event::Error { message } => self.status = format!("error: {message}"),
        }
    }
}

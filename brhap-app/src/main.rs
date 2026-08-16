//! brhap (Bohemian Rhapsody), the native application.
//!
//! One self-contained binary. No webview, no bundled frontend, no HTTP. The
//! behaviour lives in ../brhap-core, the same crate the Tauri wrapper uses, so
//! the two frontends cannot drift apart on what an operation means.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod events;
mod work;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use brhap_core::{
    Core, Event, LaunchOptions, LaunchPlan, Launched, Overrides, Profiles, Resolved, Snapshot,
    WalkSummary,
};
use events::{Incoming, Outbox};
use iced::widget::{Column, Row, button, checkbox, column, row, scrollable, text, text_input};
use iced::{Center, Color, Element, Fill, Subscription, Task};

/// Matches the Tauri window in app-rs/tauri.conf.json.
const WINDOW: (f32, f32) = (900.0, 760.0);

/// The web UI's #a33, used for anything the user needs to notice.
const TROUBLE: Color = Color::from_rgb(0.667, 0.2, 0.2);

struct Brhap {
    core: Arc<Core>,
    outbox: Outbox,

    /// Installed mods, in the order the core returned them.
    mod_ids: Vec<String>,
    /// Everything known about any id, installed or merely referenced.
    items: BTreeMap<String, Resolved>,
    /// Which mods to launch with. Order comes from `mod_ids`, not from here.
    selected: BTreeSet<String>,
    /// Ids with a lookup in flight, so a second click cannot pile on.
    busy: BTreeSet<String>,
    /// Last failure per id, cleared when that id is retried.
    errors: BTreeMap<String, String>,
    /// Startup parameters, owned here and handed to the core on launch.
    options: LaunchOptions,
    /// Per-mod replacement directories, keyed by workshop id.
    overrides: Overrides,
    /// What a launch would do, as the core describes it. Recomputed whenever
    /// an input changes, so it can never disagree with what launching runs.
    plan: Option<LaunchPlan>,
    show_preview: bool,
    api_available: bool,
    load_error: String,
    rescanning: bool,
    rescan_note: String,
    walking: bool,
    walk_note: String,

    /// Mirrors the status line the web UI shows, so the two read the same.
    status: String,
    pid: Option<u32>,
    /// Comes from the session stream, so an exit nobody asked for still lands.
    running: bool,
    launch_error: String,

    screen: Screen,
    store: Profiles,
    profile_name: String,
    profile_error: String,
    /// What the last applied profile did, shown in the header.
    apply_note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Launch,
    Profiles,
}

/// One startup parameter. The web UI spreads these across two places: the
/// header carries the two platform switches, the body carries the argv flags.
#[derive(Debug, Clone, Copy)]
enum Flag {
    NoSplash,
    SkipIntro,
    EmptyWorld,
    IntelMode,
    SteamOverlay,
}

impl Flag {
    /// What the flag puts on the command line, or what it does when it does
    /// not put anything there.
    fn label(self) -> &'static str {
        match self {
            Self::NoSplash => "-noSplash",
            Self::SkipIntro => "-skipIntro",
            Self::EmptyWorld => "-world=empty",
            Self::IntelMode => "Intel Mode",
            Self::SteamOverlay => "Steam Overlay",
        }
    }

    fn get(self, options: &LaunchOptions) -> bool {
        match self {
            Self::NoSplash => options.no_splash,
            Self::SkipIntro => options.skip_intro,
            Self::EmptyWorld => options.empty_world,
            Self::IntelMode => options.intel_mode,
            Self::SteamOverlay => options.steam_overlay,
        }
    }

    fn set(self, options: &mut LaunchOptions, value: bool) {
        match self {
            Self::NoSplash => options.no_splash = value,
            Self::SkipIntro => options.skip_intro = value,
            Self::EmptyWorld => options.empty_world = value,
            Self::IntelMode => options.intel_mode = value,
            Self::SteamOverlay => options.steam_overlay = value,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Loaded(Snapshot),
    Toggled(String, bool),
    Flagged(Flag, bool),
    /// Ask the core about one id. `true` re-fetches something already known.
    Resolve(String, bool),
    Resolved(String, Result<Resolved, String>),
    TogglePreview,
    Launch,
    Launched(Result<Launched, String>),
    Stop,
    Stopped(Result<(), String>),
    Recheck,
    ResetCache,
    ResetConfirmed(bool),
    /// A rescan replaces what is known rather than adding to it, so it is kept
    /// apart from `Loaded`. Carries the note to show alongside it.
    Rescanned(Snapshot, String),
    Walk,
    Walked(Result<WalkSummary, String>),
    PickOverride(String),
    OverridePicked(String, Option<PathBuf>),
    ClearOverride(String),
    Show(Screen),
    ProfileName(String),
    SaveProfile,
    DeleteProfile(String),
    ApplyProfile(String),
    /// List and delete answer with the whole store, so the caller restates
    /// rather than patches.
    Stored(Result<Profiles, String>),
    /// Save answers the same way, but is kept separate because only a save
    /// that went through should clear the name field.
    Saved(Result<Profiles, String>),
    Session(Incoming),
}

fn boot() -> (Brhap, Task<Message>) {
    let outbox = Outbox::default();
    // The listener is built before the subscription exists; see events.rs.
    let core = Arc::new(Core::new(outbox.listener()));

    let state = Brhap {
        core,
        outbox,
        mod_ids: Vec::new(),
        items: BTreeMap::new(),
        selected: BTreeSet::new(),
        busy: BTreeSet::new(),
        errors: BTreeMap::new(),
        options: LaunchOptions::default(),
        overrides: Overrides::new(),
        plan: None,
        show_preview: false,
        api_available: false,
        load_error: String::new(),
        rescanning: false,
        rescan_note: String::new(),
        walking: false,
        walk_note: String::new(),
        status: "idle".to_string(),
        pid: None,
        running: false,
        launch_error: String::new(),
        screen: Screen::Launch,
        store: Profiles::default(),
        profile_name: String::new(),
        profile_error: String::new(),
        apply_note: String::new(),
    };

    let load = state.reload();
    (state, load)
}

fn update(state: &mut Brhap, message: Message) -> Task<Message> {
    match message {
        Message::Loaded(snapshot) => {
            state.absorb_snapshot(snapshot);
            state.replan();
            Task::none()
        }
        Message::Toggled(id, checked) => {
            if checked {
                state.selected.insert(id);
            } else {
                state.selected.remove(&id);
            }
            state.replan();
            Task::none()
        }
        Message::Flagged(flag, value) => {
            flag.set(&mut state.options, value);
            state.replan();
            Task::none()
        }
        Message::TogglePreview => {
            state.show_preview = !state.show_preview;
            Task::none()
        }
        Message::Launch => {
            state.launch_error.clear();

            let core = Arc::clone(&state.core);
            let ids = state.selected_ids();
            let options = state.options;
            let overrides = state.overrides.clone();
            Task::perform(
                work::blocking(move || core.launch(&ids, options, &overrides)),
                Message::Launched,
            )
        }
        Message::Stop => {
            state.launch_error.clear();

            let core = Arc::clone(&state.core);
            Task::perform(work::blocking(move || core.stop()), Message::Stopped)
        }
        // Success needs no handling: the running state and the status line both
        // arrive through the session stream instead, which is also how an exit
        // nobody asked for gets reported.
        Message::Launched(Err(message)) | Message::Stopped(Err(message)) => {
            state.launch_error = message;
            Task::none()
        }
        Message::Launched(Ok(_)) | Message::Stopped(Ok(())) => Task::none(),
        Message::Recheck => {
            state.rescanning = true;
            state.rescan_note.clear();

            let core = Arc::clone(&state.core);
            Task::perform(work::blocking(move || core.rescan()), |snapshot| {
                let note = format!("{} mod(s) installed.", snapshot.mods.len());
                Message::Rescanned(snapshot, note)
            })
        }
        // Throwing away the cache is worth a confirmation, as the web UI does.
        Message::ResetCache => Task::perform(
            async {
                rfd::AsyncMessageDialog::new()
                    .set_title("Reset dependency cache")
                    .set_description(
                        "Discard every cached dependency lookup? Installed mod names are not affected.",
                    )
                    .set_buttons(rfd::MessageButtons::OkCancel)
                    .show()
                    .await
            },
            |answer| Message::ResetConfirmed(matches!(answer, rfd::MessageDialogResult::Ok)),
        ),
        Message::ResetConfirmed(false) => Task::none(),
        Message::ResetConfirmed(true) => {
            state.rescanning = true;
            state.rescan_note.clear();

            let core = Arc::clone(&state.core);
            Task::perform(work::blocking(move || core.reset_cache()), |snapshot| {
                Message::Rescanned(snapshot, "Dependency cache cleared.".to_string())
            })
        }
        Message::Rescanned(snapshot, note) => {
            state.rescanning = false;
            state.rescan_note = note;
            state.resync(snapshot);
            state.replan();
            Task::none()
        }
        Message::Walk => {
            state.walking = true;
            state.walk_note.clear();

            let core = Arc::clone(&state.core);
            Task::perform(work::blocking(move || core.walk_all()), Message::Walked)
        }
        Message::Walked(Ok(summary)) => {
            state.walking = false;
            state.walk_note = format!(
                "Resolved {} items in {} API call(s).",
                summary.resolved, summary.calls
            );
            // The walk filled the cache; this is what reads it back out.
            state.reload()
        }
        Message::Walked(Err(message)) => {
            state.walking = false;
            state.walk_note = message;
            Task::none()
        }
        // Pointing a workshop id at a local directory, so a mod can be edited
        // in place and launched without reinstalling it.
        Message::PickOverride(id) => Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Choose a folder to use instead of the workshop copy")
                    .pick_folder()
                    .await
                    .map(|handle| handle.path().to_path_buf())
            },
            move |path| Message::OverridePicked(id.clone(), path),
        ),
        Message::OverridePicked(id, Some(path)) => {
            state.overrides.insert(id, path);
            state.replan();
            Task::none()
        }
        // Cancelled, so leave whatever was there alone.
        Message::OverridePicked(_, None) => Task::none(),
        Message::ClearOverride(id) => {
            state.overrides.remove(&id);
            state.replan();
            Task::none()
        }
        Message::Show(screen) => {
            state.screen = screen;
            if screen != Screen::Profiles {
                return Task::none();
            }
            state.profile_error.clear();

            let core = Arc::clone(&state.core);
            Task::perform(
                work::blocking(move || Ok::<_, String>(core.list_profiles())),
                Message::Stored,
            )
        }
        Message::ProfileName(name) => {
            state.profile_name = name;
            Task::none()
        }
        Message::SaveProfile => {
            state.profile_error.clear();

            let core = Arc::clone(&state.core);
            let name = state.profile_name.clone();
            Task::perform(work::blocking(move || core.save_profile(&name)), Message::Saved)
        }
        Message::DeleteProfile(name) => {
            state.profile_error.clear();

            let core = Arc::clone(&state.core);
            Task::perform(work::blocking(move || core.delete_profile(&name)), Message::Stored)
        }
        Message::Saved(Ok(store)) => {
            // A rejected save leaves what was typed alone to fix.
            state.profile_name.clear();
            state.store = store;
            Task::none()
        }
        Message::Stored(Ok(store)) => {
            state.store = store;
            Task::none()
        }
        Message::Stored(Err(message)) | Message::Saved(Err(message)) => {
            state.profile_error = message;
            Task::none()
        }
        Message::ApplyProfile(name) => {
            let Some(profile) =
                state.store.profiles.iter().find(|entry| entry.name == name).cloned()
            else {
                return Task::none();
            };

            // A profile can name mods that have since been uninstalled. Select
            // what is still there and say what was skipped, rather than
            // silently loading less than the profile promised. See
            // App.svelte:155-173. The stored profile is left alone.
            let (known, missing): (Vec<String>, Vec<String>) =
                profile.ids.iter().cloned().partition(|id| state.mod_ids.contains(id));

            state.selected = known.into_iter().collect();
            state.options = profile.options;
            state.overrides = profile.overrides.clone();
            state.apply_note = if missing.is_empty() {
                format!("Loaded \"{}\".", profile.name)
            } else {
                format!(
                    "Loaded \"{}\". Not installed, skipped: {}",
                    profile.name,
                    missing.join(", ")
                )
            };
            state.screen = Screen::Launch;
            state.replan();
            Task::none()
        }
        Message::Resolve(id, refresh) => {
            state.busy.insert(id.clone());
            state.errors.remove(&id);

            let core = Arc::clone(&state.core);
            let target = id.clone();
            Task::perform(
                work::blocking(move || core.resolve_item(&target, refresh)),
                move |result| Message::Resolved(id.clone(), result),
            )
        }
        Message::Resolved(id, result) => {
            state.busy.remove(&id);
            match result {
                Ok(item) => {
                    state.items.insert(item.id.clone(), item);
                    // The same page may have told the core names for the
                    // requirements too, so take a fresh snapshot rather than
                    // leaving those ids unnamed.
                    state.reload()
                }
                Err(message) => {
                    state.errors.insert(id, message);
                    Task::none()
                }
            }
        }
        Message::Session(incoming) => {
            state.outbox.accept(&incoming);
            if let Incoming::Emitted(event) = incoming {
                state.absorb_event(event);
            }
            Task::none()
        }
    }
}

impl Brhap {
    /// Follows `absorb` in web/src/App.svelte:207-249: referenced items are
    /// folded in alongside installed ones, so a dependency the user does not
    /// have installed can still be named.
    fn absorb_snapshot(&mut self, snapshot: Snapshot) {
        self.mod_ids = snapshot.mods.iter().map(|item| item.id.clone()).collect();
        self.api_available = snapshot.api_available;
        for item in snapshot.mods.into_iter().chain(snapshot.referenced) {
            self.items.insert(item.id.clone(), item);
        }
    }

    /// Take a fresh snapshot off the UI thread.
    fn reload(&self) -> Task<Message> {
        let core = Arc::clone(&self.core);
        Task::perform(work::blocking(move || core.snapshot()), Message::Loaded)
    }

    /// How many installed mods are selected. Counted against `mod_ids` rather
    /// than the set itself, so an id left over from an uninstalled mod does not
    /// inflate the number.
    fn selected_count(&self) -> usize {
        self.mod_ids.iter().filter(|id| self.selected.contains(*id)).count()
    }

    /// The selection in `mod_ids` order, which is the order `-mod=` gets.
    fn selected_ids(&self) -> Vec<String> {
        self.mod_ids.iter().filter(|id| self.selected.contains(*id)).cloned().collect()
    }

    /// Rebuild the plan from the current inputs.
    ///
    /// The core owns the argument shape, so the preview always reflects what a
    /// launch would actually run rather than a second guess at it. This is pure
    /// computation, no filesystem or network, so it runs inline.
    fn replan(&mut self) {
        let ids = self.selected_ids();
        self.plan = Some(self.core.preview(&ids, self.options, &self.overrides));
    }

    /// Replace what is known rather than merging into it.
    ///
    /// Follows `absorbSnapshot` in web/src/App.svelte:239-249. A rescan is the
    /// one moment a mod can vanish, so anything no longer installed drops out
    /// of the selection and the item table is rebuilt rather than added to.
    fn resync(&mut self, snapshot: Snapshot) {
        let known: BTreeSet<String> =
            snapshot.mods.iter().map(|item| item.id.clone()).collect();
        self.selected.retain(|id| known.contains(id));
        self.items.clear();
        self.absorb_snapshot(snapshot);
    }

    /// Current knowledge about an id, without claiming anything unresolved.
    fn item(&self, id: &str) -> Resolved {
        self.items.get(id).cloned().unwrap_or_else(|| Resolved {
            id: id.to_string(),
            name: None,
            installed: false,
            requires: None,
            source: brhap_core::Source::Unknown,
            fetched_at: String::new(),
        })
    }

    /// Requirements of the selected mods that will not be loaded as things
    /// stand, following web/src/App.svelte:180-194.
    ///
    /// Only reports what has actually been resolved. An unresolved mod says
    /// nothing rather than claiming it has no requirements.
    fn unmet(&self) -> Vec<String> {
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

    /// The dependency line under one mod, following web/src/App.svelte:360-393.
    ///
    /// An unresolved mod offers a lookup rather than claiming it has no
    /// requirements, which is the whole point of `requires` being optional.
    fn deps(&self, id: &str) -> Element<'_, Message> {
        let item = self.item(id);
        let busy = self.busy.contains(id);
        let mut parts: Vec<Element<Message>> = Vec::new();

        match item.requires.as_deref() {
            None => parts.push(action(
                if busy { "checking...".to_string() } else { "check dependencies".to_string() },
                (!busy).then(|| Message::Resolve(id.to_string(), false)),
            )),
            Some([]) => {
                parts.push(hint("no dependencies".to_string()));
                parts.push(link(
                    "refresh".to_string(),
                    (!busy).then(|| Message::Resolve(id.to_string(), true)),
                ));
            }
            Some(requires) => {
                parts.push(hint("requires:".to_string()));
                for child_id in requires {
                    let child = self.item(child_id);
                    let child_busy = self.busy.contains(child_id);
                    let installed = child.installed;
                    let deps_known = child.requires.is_some();

                    match child.name {
                        None => parts.push(action(
                            if child_busy { "fetching...".to_string() } else { child_id.clone() },
                            (!child_busy).then(|| Message::Resolve(child_id.clone(), false)),
                        )),
                        Some(name) if installed => parts.push(hint(name)),
                        Some(name) => {
                            parts.push(hint(format!("{name} (not installed)")));
                            if !deps_known {
                                parts.push(action(
                                    if child_busy {
                                        "fetching...".to_string()
                                    } else {
                                        "its dependencies".to_string()
                                    },
                                    (!child_busy)
                                        .then(|| Message::Resolve(child_id.clone(), false)),
                                ));
                            }
                        }
                    }
                }
                parts.push(link(
                    "refresh".to_string(),
                    (!busy).then(|| Message::Resolve(id.to_string(), true)),
                ));
            }
        }

        if let Some(error) = self.errors.get(id) {
            parts.push(hint(error.clone()));
            parts.push(action(
                "retry".to_string(),
                Some(Message::Resolve(id.to_string(), true)),
            ));
        }

        match self.overrides.get(id) {
            Some(path) => {
                parts.push(hint(format!("override: {}", path.display())));
                parts.push(link(
                    "clear override".to_string(),
                    Some(Message::ClearOverride(id.to_string())),
                ));
            }
            None => parts.push(link(
                "override".to_string(),
                Some(Message::PickOverride(id.to_string())),
            )),
        }

        Row::with_children(parts).spacing(6).align_y(Center).wrap().into()
    }

    /// Status wording follows web/src/App.svelte:65-82 so the two frontends
    /// describe the same run the same way.
    fn absorb_event(&mut self, event: Event) {
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

/// One-line summary of a saved launch, following `describe` in App.svelte:121.
fn describe(ids: &[String], options: &LaunchOptions) -> String {
    let flags: Vec<&str> = [
        (options.no_splash, Flag::NoSplash.label()),
        (options.skip_intro, Flag::SkipIntro.label()),
        (options.empty_world, Flag::EmptyWorld.label()),
    ]
    .into_iter()
    .filter_map(|(on, label)| on.then_some(label))
    .collect();

    if flags.is_empty() {
        format!("{} mod(s)", ids.len())
    } else {
        format!("{} mod(s), {}", ids.len(), flags.join(" "))
    }
}

/// A startup parameter switch.
fn flag(current: &LaunchOptions, flag: Flag) -> Element<'static, Message> {
    checkbox(flag.get(current))
        .label(flag.label())
        .size(14)
        .text_size(13)
        .on_toggle(move |value| Message::Flagged(flag, value))
        .into()
}

/// Muted supporting text, the equivalent of the web UI's `.hint`.
fn hint(label: String) -> Element<'static, Message> {
    text(label).size(12).into()
}

/// A button that disables itself when there is nothing to do, which is how a
/// lookup already in flight stops a second click.
fn action(label: String, message: Option<Message>) -> Element<'static, Message> {
    let mut control = button(text(label).size(12));
    if let Some(message) = message {
        control = control.on_press(message);
    }
    control.into()
}

/// A destructive action, matching `.destructive` in the web UI.
fn danger(label: String, message: Option<Message>) -> Element<'static, Message> {
    let mut control = button(text(label).size(12)).style(button::danger);
    if let Some(message) = message {
        control = control.on_press(message);
    }
    control.into()
}

/// The same thing drawn as a link, matching `.link` in the web UI.
fn link(label: String, message: Option<Message>) -> Element<'static, Message> {
    let mut control = button(text(label).size(12)).style(button::text);
    if let Some(message) = message {
        control = control.on_press(message);
    }
    control.into()
}

fn subscription(_state: &Brhap) -> Subscription<Message> {
    events::subscription().map(Message::Session)
}

/// Shown on both screens, following App.svelte:303-322.
fn header(state: &Brhap) -> Column<'_, Message> {
    let loaded = if state.apply_note.is_empty() {
        "none".to_string()
    } else {
        state.apply_note.clone()
    };

    column![
        text("brhap").size(26),
        text("Bohemian Rhapsody").size(14),
        row![
            action("home".to_string(), Some(Message::Show(Screen::Launch))),
            action("profiles".to_string(), Some(Message::Show(Screen::Profiles))),
        ]
        .spacing(6),
        text(format!("Loaded: {loaded}")).size(12),
        // The web UI keeps these two in the header, away from the argv flags,
        // because they change how the game is started rather than what it is
        // passed. See App.svelte:314-318.
        row![
            flag(&state.options, Flag::IntelMode),
            flag(&state.options, Flag::SteamOverlay),
        ]
        .spacing(16),
    ]
    .spacing(4)
}

fn view(state: &Brhap) -> Element<'_, Message> {
    let body = match state.screen {
        Screen::Launch => launch_screen(state),
        Screen::Profiles => profiles_screen(state),
    };

    column![header(state), body].spacing(10).padding(20).into()
}

fn launch_screen(state: &Brhap) -> Column<'_, Message> {
    let rows = state.mod_ids.iter().map(|id| {
        let item = state.item(id);
        let name = item.name.unwrap_or_else(|| id.clone());
        let key = id.clone();
        column![
            row![
                checkbox(state.selected.contains(id))
                    .label(name)
                    .on_toggle(move |checked| Message::Toggled(key.clone(), checked)),
                text(id.clone()).size(12),
            ]
            .spacing(8)
            .align_y(Center),
            state.deps(id),
        ]
        .spacing(2)
        .into()
    });

    let mut page = Column::new().spacing(10);

    if !state.load_error.is_empty() {
        page = page.push(text(format!("Could not load: {}", state.load_error)));
    }

    page = page
        .push(
            text(format!(
                "{} of {} selected. Dependency lookups happen only when you click.",
                state.selected_count(),
                state.mod_ids.len()
            ))
            .size(13),
        )
        .push(
            row![
                action(
                    if state.rescanning {
                        "rechecking...".to_string()
                    } else {
                        "recheck installed mods".to_string()
                    },
                    (!state.rescanning).then_some(Message::Recheck),
                ),
                action(
                    "reset dependency cache".to_string(),
                    (!state.rescanning).then_some(Message::ResetCache),
                ),
                hint(state.rescan_note.clone()),
            ]
            .spacing(6)
            .align_y(Center),
        );

    // The batched walk needs STEAM_KEY, so it is offered only when the core
    // says the key is there. See App.svelte:341-348.
    if state.api_available {
        page = page.push(
            row![
                action(
                    if state.walking {
                        "resolving...".to_string()
                    } else {
                        "resolve all via Steam API".to_string()
                    },
                    (!state.walking).then_some(Message::Walk),
                ),
                hint(state.walk_note.clone()),
            ]
            .spacing(6)
            .align_y(Center),
        );
    }

    page = page.push(scrollable(Column::with_children(rows).spacing(6)).height(Fill));

    let unmet = state.unmet();
    if !unmet.is_empty() {
        page = page.push(text("Unmet requirements").size(16));
        for entry in unmet {
            page = page.push(text(entry).size(12).color(TROUBLE));
        }
    }

    page = page
        .push(text("Startup parameters").size(16))
        .push(
            row![
                flag(&state.options, Flag::NoSplash),
                flag(&state.options, Flag::SkipIntro),
                flag(&state.options, Flag::EmptyWorld),
            ]
            .spacing(16),
        );

    // Follows App.svelte:424-438: no plan, nothing to say about launching.
    if let Some(plan) = &state.plan {
        let pid = match state.pid {
            Some(pid) => format!(" (pid {pid})"),
            None => String::new(),
        };

        page = page
            .push(text("Launch").size(16))
            .push(
                text(format!(
                    "{} symlink(s) will be created in the game directory. Status: {}{}",
                    plan.symlinks.len(),
                    state.status,
                    pid
                ))
                .size(13),
            )
            .push(
                row![
                    action("launch".to_string(), (!state.running).then_some(Message::Launch)),
                    danger("stop".to_string(), state.running.then_some(Message::Stop)),
                ]
                .spacing(8)
                .align_y(Center),
            )
            .push(link(
                if state.show_preview { "hide details".to_string() } else { "details".to_string() },
                Some(Message::TogglePreview),
            ));

        if !state.launch_error.is_empty() {
            page = page.push(text(state.launch_error.clone()).size(12).color(TROUBLE));
        }

        if state.show_preview {
            page = page.push(text(plan.preview.clone()).size(11));
        }
    }

    page
}

/// The profiles screen, following App.svelte:439-472.
fn profiles_screen(state: &Brhap) -> Column<'_, Message> {
    let mut page = Column::new().spacing(10).push(text("Save the last launch").size(16));

    page = match &state.store.last_launch {
        Some(last) => page
            .push(
                text(format!("Last launched: {}", describe(&last.ids, &last.options))).size(13),
            )
            .push(
                row![
                    text_input("profile name", &state.profile_name)
                        .on_input(Message::ProfileName)
                        .on_submit(Message::SaveProfile)
                        .size(13)
                        .width(260),
                    action(
                        "save".to_string(),
                        (!state.profile_name.trim().is_empty()).then_some(Message::SaveProfile),
                    ),
                ]
                .spacing(6)
                .align_y(Center),
            ),
        None => page.push(
            text("Launch the game once, and what you selected shows up here to name.").size(13),
        ),
    };

    if !state.profile_error.is_empty() {
        page = page.push(text(state.profile_error.clone()).size(12).color(TROUBLE));
    }

    page = page.push(text("Saved profiles").size(16));

    if state.store.profiles.is_empty() {
        return page.push(text("Nothing saved yet.").size(13));
    }

    for profile in &state.store.profiles {
        page = page.push(
            row![
                link(profile.name.clone(), Some(Message::ApplyProfile(profile.name.clone()))),
                hint(describe(&profile.ids, &profile.options)),
                danger(
                    "delete".to_string(),
                    Some(Message::DeleteProfile(profile.name.clone())),
                ),
            ]
            .spacing(8)
            .align_y(Center),
        );
    }

    page
}

fn main() -> iced::Result {
    iced::application(boot, update, view)
        .subscription(subscription)
        .title("brhap")
        .window_size(WINDOW)
        .run()
}

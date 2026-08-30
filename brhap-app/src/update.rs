//! Every state transition, one arm per message.

use std::sync::Arc;
use std::time::Duration;

use iced::Task;

use crate::events::Incoming;
use crate::message::Message;
use crate::state::{Brhap, Screen};
use crate::work;

pub(crate) fn update(state: &mut Brhap, message: Message) -> Task<Message> {
    match message {
        Message::Loaded(snapshot) => {
            state.absorb_snapshot(snapshot);
            state.replan();
            Task::none()
        }
        Message::Toggled(id, checked) => {
            if !checked {
                state.selected.remove(&id);
                state.replan();
                return Task::none();
            }

            state.selected.insert(id.clone());
            state.replan();

            // Selecting is a local act and always succeeds. The lookup is a
            // side effect: if it fails the mod stays selected and the row just
            // reports it. Only a direct click gets here, so applying a profile
            // reads the cache and fetches nothing.
            if state.item(&id).requires.is_some() || state.pending.contains(&id) {
                return Task::none();
            }

            state.pending.insert(id.clone());
            let core = Arc::clone(&state.core);
            let target = id.clone();
            Task::perform(
                work::blocking(move || core.resolve_item(&target, false)),
                move |result| Message::Resolved(id.clone(), result),
            )
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
        Message::Refresh => {
            state.rescanning = true;
            state.rescan_note.clear();

            let core = Arc::clone(&state.core);
            Task::perform(
                work::blocking(move || {
                    // Rescan first. It picks up newly subscribed mods, and it
                    // drops the cached requires of anything that used to be a
                    // known dependency and is now installed
                    // (brhap-server/src/cache.rs:85-88). The walk is what puts
                    // those back, so doing it second self-heals.
                    let scanned = core.rescan();
                    let installed = format!("{} mod(s) installed.", scanned.mods.len());

                    if !scanned.api_available {
                        return (scanned, installed);
                    }

                    match core.walk_all() {
                        Ok(summary) => (
                            core.snapshot(),
                            format!(
                                "{installed} Resolved {} items in {} API call(s).",
                                summary.resolved, summary.calls
                            ),
                        ),
                        Err(message) => (scanned, format!("{installed} {message}")),
                    }
                }),
                |(snapshot, note)| Message::Rescanned(snapshot, note),
            )
        }
        // Throwing away the cache is worth a confirmation, as the web UI does.
        Message::Refetch => {
            let ids = state.selected_ids();
            if ids.is_empty() {
                return Task::none();
            }

            state.pending.extend(ids.iter().cloned());
            for id in &ids {
                state.errors.remove(id);
            }

            // One Workshop page fetch per mod. Steam rate limits a burst of
            // those, so they go out 3 seconds apart: the first immediately,
            // the second after 3s, and so on. `work::blocking` already runs
            // each on its own thread, so the delay is just a sleep.
            Task::batch(ids.into_iter().enumerate().map(|(index, id)| {
                let core = Arc::clone(&state.core);
                let target = id.clone();
                Task::perform(
                    work::blocking(move || {
                        std::thread::sleep(Duration::from_secs(3 * index as u64));
                        core.resolve_item(&target, true)
                    }),
                    move |result| Message::Resolved(id.clone(), result),
                )
            }))
        }
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
            match screen {
                Screen::Launch => Task::none(),
                Screen::Profiles => {
                    state.profile_error.clear();

                    let core = Arc::clone(&state.core);
                    Task::perform(
                        work::blocking(move || Ok::<_, String>(core.list_profiles())),
                        Message::Stored,
                    )
                }
                Screen::Settings => {
                    state.settings_error.clear();

                    let core = Arc::clone(&state.core);
                    Task::perform(work::blocking(move || core.settings()), Message::SettingsLoaded)
                }
            }
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
        // The editor opens empty rather than holding the stored key, so what is
        // typed is always the whole of what gets saved.
        Message::EditSteamKey => {
            state.settings_error.clear();
            state.key_input.clear();
            state.reveal_key = false;
            state.editing_key = true;
            Task::none()
        }
        Message::KeyInput(key) => {
            state.key_input = key;
            Task::none()
        }
        Message::ToggleReveal => {
            state.reveal_key = !state.reveal_key;
            Task::none()
        }
        Message::CancelKeyEdit => {
            state.editing_key = false;
            state.reveal_key = false;
            state.key_input.clear();
            Task::none()
        }
        Message::CommitSteamKey => {
            state.settings_error.clear();

            let core = Arc::clone(&state.core);
            let key = state.key_input.clone();
            Task::perform(
                work::blocking(move || core.save_steam_key(&key)),
                Message::SettingsSaved,
            )
        }
        Message::SettingsLoaded(settings) => {
            state.settings = settings;
            Task::none()
        }
        // A saved key is one of the two sources `api_available` reports on, so
        // the snapshot is retaken rather than left until the next refresh.
        Message::SettingsSaved(Ok(settings)) => {
            state.settings = settings;
            state.editing_key = false;
            state.reveal_key = false;
            state.key_input.clear();
            state.reload()
        }
        // A rejected save leaves what was typed alone to fix, the way
        // `Message::Saved` treats the profile name.
        Message::SettingsSaved(Err(message)) => {
            state.settings_error = message;
            Task::none()
        }
        Message::ApplyProfile(name) => {
            let Some(profile) = state.store.profiles.iter().find(|entry| entry.name == name).cloned()
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
        Message::Resolved(id, result) => {
            state.pending.remove(&id);
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

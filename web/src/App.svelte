<script lang="ts">
  import {
    deleteProfile,
    getPreview,
    getSnapshot,
    launch,
    listProfiles,
    mode,
    rescan,
    resetCache,
    resolveItem,
    saveProfile,
    stop,
    subscribe,
    walkAll,
    ResolveError,
    type Item,
    type LaunchEvent,
    type LaunchOptions,
    type LaunchPlan,
    type Profile,
    type Profiles,
  } from './lib/api'
  // The desktop app's own icon, copied from app-rs/icons so the frontend
  // bundles it rather than reaching across packages.
  import icon from './assets/icon.png'

  let items = $state<Record<string, Item>>({})
  let modIds = $state<string[]>([])
  let selected = $state<Record<string, boolean>>({})
  let busy = $state<Record<string, boolean>>({})
  let errors = $state<Record<string, string>>({})
  let loadError = $state('')
  let apiAvailable = $state(false)
  let walking = $state(false)
  let walkNote = $state('')
  let rescanning = $state(false)
  let rescanNote = $state('')

  let plan = $state<LaunchPlan | null>(null)
  let options = $state<LaunchOptions>({
    noSplash: false,
    skipIntro: false,
    emptyWorld: false,
    intelMode: false,
    steamOverlay: false,
  })
  let running = $state(false)
  let pid = $state<number | null>(null)
  let status = $state('idle')
  let launchError = $state('')

  // Profiles live in the Rust backend, so this whole view only exists on the
  // desktop. The nav below is what keeps browser mode from ever calling it.
  let view = $state<'launch' | 'profiles'>('launch')
  let store = $state<Profiles>({ lastLaunch: null, profiles: [] })
  let profileName = $state('')
  let profileError = $state('')
  let applyNote = $state('')

  // Status comes from the server's stream, so an exit the page did not ask for
  // still shows up here.
  $effect(() => {
    const unsubscribe = subscribe((event: LaunchEvent) => {
      if (event.type === 'state') {
        running = event.running
        pid = event.pid
        if (!event.running && status.startsWith('running')) status = 'idle'
      } else if (event.type === 'linked') {
        status = `linked ${event.created} mod(s), removed ${event.removed} stale link(s)`
      } else if (event.type === 'spawned') {
        status = `running as pid ${event.pid}`
      } else if (event.type === 'exited') {
        status = `exited with code ${event.code ?? 'none'}${event.signal ? ` (${event.signal})` : ''}`
      } else if (event.type === 'error') {
        status = `error: ${event.message}`
      }
    })
    return unsubscribe
  })

  async function startGame() {
    launchError = ''
    try {
      await launch(selectedIds, options)
    } catch (error) {
      launchError = error instanceof Error ? error.message : String(error)
    }
  }

  async function stopGame() {
    launchError = ''
    try {
      await stop()
    } catch (error) {
      launchError = error instanceof Error ? error.message : String(error)
    }
  }

  const flagLabels: [keyof LaunchOptions, string][] = [
    ['noSplash', '-noSplash'],
    ['skipIntro', '-skipIntro'],
    ['emptyWorld', '-world=empty'],
  ]

  function describe(ids: string[], options: LaunchOptions): string {
    const flags = flagLabels.filter(([key]) => options[key]).map(([, label]) => label)
    return `${ids.length} mod(s)${flags.length > 0 ? `, ${flags.join(' ')}` : ''}`
  }

  async function openProfiles() {
    view = 'profiles'
    profileError = ''
    try {
      store = await listProfiles()
    } catch (error) {
      profileError = error instanceof Error ? error.message : String(error)
    }
  }

  async function saveNamed() {
    profileError = ''
    try {
      store = await saveProfile(profileName)
      profileName = ''
    } catch (error) {
      profileError = error instanceof Error ? error.message : String(error)
    }
  }

  async function removeNamed(name: string) {
    profileError = ''
    try {
      store = await deleteProfile(name)
    } catch (error) {
      profileError = error instanceof Error ? error.message : String(error)
    }
  }

  // A profile can name mods that have since been uninstalled. Select what is
  // still there and say what was skipped, rather than silently loading less
  // than the profile promised. The stored profile is left alone.
  function applyProfile(profile: Profile) {
    const known = new Set(modIds)
    selected = {}
    for (const id of profile.ids) {
      if (known.has(id)) selected[id] = true
    }
    options = { ...profile.options }

    const missing = profile.ids.filter((id) => !known.has(id))
    applyNote =
      missing.length > 0
        ? `Loaded "${profile.name}". Not installed, skipped: ${missing.join(', ')}`
        : `Loaded "${profile.name}".`
    view = 'launch'
  }

  const selectedIds = $derived(modIds.filter((id) => selected[id]))

  // Requirements of the selected mods that will not be loaded as things stand.
  // Only reports what has actually been resolved; an unresolved mod says
  // nothing rather than claiming it has no requirements.
  const unmet = $derived(
    selectedIds.flatMap((id) =>
      (items[id]?.requires ?? [])
        .filter((childId) => !selected[childId])
        .map((childId) => {
          const child = itemFor(childId)
          return {
            key: `${id}:${childId}`,
            mod: items[id]?.name ?? id,
            child: child.name ?? childId,
            reason: child.installed ? 'installed but not selected' : 'not installed',
          }
        }),
    ),
  )

  // The server owns the argument shape, so the preview always reflects what a
  // launch would actually run rather than a second guess at it.
  $effect(() => {
    const ids = selectedIds
    const current = { ...options }
    getPreview(ids, current)
      .then((result) => { plan = result })
      .catch(() => { plan = null })
  })

  function absorb(list: Item[]) {
    for (const item of list) items[item.id] = item
  }

  async function load() {
    try {
      const snapshot = await getSnapshot()
      modIds = snapshot.mods.map((mod) => mod.id)
      apiAvailable = snapshot.apiAvailable
      absorb(snapshot.mods)
      absorb(snapshot.referenced)
    } catch (error) {
      loadError = String(error)
    }
  }

  async function clearCache() {
    if (!confirm('Discard every cached dependency lookup? Installed mod names are not affected.')) {
      return
    }
    rescanning = true
    rescanNote = ''
    try {
      absorbSnapshot(await resetCache())
      rescanNote = 'Dependency cache cleared.'
    } catch (error) {
      rescanNote = String(error)
    } finally {
      rescanning = false
    }
  }

  function absorbSnapshot(snapshot: Awaited<ReturnType<typeof rescan>>) {
    const known = new Set(snapshot.mods.map((mod) => mod.id))
    for (const id of Object.keys(selected)) {
      if (!known.has(id)) delete selected[id]
    }
    items = {}
    modIds = snapshot.mods.map((mod) => mod.id)
    apiAvailable = snapshot.apiAvailable
    absorb(snapshot.mods)
    absorb(snapshot.referenced)
  }

  async function recheck() {
    rescanning = true
    rescanNote = ''
    try {
      const snapshot = await rescan()
      absorbSnapshot(snapshot)
      rescanNote = `${snapshot.mods.length} mod(s) installed.`
    } catch (error) {
      rescanNote = String(error)
    } finally {
      rescanning = false
    }
  }

  async function walk() {
    walking = true
    walkNote = ''
    try {
      const result = await walkAll()
      await load()
      walkNote = `Resolved ${result.resolved} items in ${result.calls} API call(s).`
    } catch (error) {
      walkNote = String(error)
    } finally {
      walking = false
    }
  }

  async function resolve(id: string, refresh = false) {
    busy[id] = true
    delete errors[id]
    try {
      const item = await resolveItem(id, refresh)
      items[id] = item
      // The server may now know names it learned from the same page.
      const snapshot = await getSnapshot()
      absorb(snapshot.referenced)
    } catch (error) {
      errors[id] = error instanceof ResolveError ? error.message : String(error)
    } finally {
      busy[id] = false
    }
  }

  function itemFor(id: string): Item {
    return items[id] ?? { id, name: null, installed: false, requires: null, source: 'unknown', fetchedAt: '' }
  }

  load()
</script>

<main>
  <header>
    <div>
      <h1>brhap</h1>
      <p class="subtitle">Bohemian Rhapsody</p>
      {#if mode === 'desktop'}
        <p class="hint nav">
          <button class:secondary={view !== 'launch'} onclick={() => (view = 'launch')}>home</button>
          <button class:secondary={view !== 'profiles'} onclick={openProfiles}>profiles</button>
        </p>
        <p class="hint loaded">Loaded: {#if applyNote}{applyNote}{:else}<em>none</em>{/if}</p>
        <p class="hint">
          <label><input type="checkbox" bind:checked={options.intelMode} /> Intel Mode</label>
        </p>
        <p class="hint">
          <label><input type="checkbox" bind:checked={options.steamOverlay} /> Steam Overlay</label>
        </p>
      {/if}
    </div>
    <img class="icon" src={icon} alt="" width="64" height="64" />
  </header>

  {#if loadError}
    <p class="error">Could not reach the server: {loadError}</p>
  {/if}

  {#if view === 'launch'}
    <p class="hint">
      {selectedIds.length} of {modIds.length} selected. Dependency lookups happen only when you click.
    </p>

    <p class="hint">
      <button onclick={recheck} disabled={rescanning}>
        {rescanning ? 'rechecking...' : 'recheck installed mods'}
      </button>
      <button class="secondary" onclick={clearCache} disabled={rescanning}>reset dependency cache</button>
      {#if rescanNote}<span class="note">{rescanNote}</span>{/if}
    </p>

    {#if apiAvailable}
      <p class="hint">
        <button onclick={walk} disabled={walking}>
          {walking ? 'resolving...' : 'resolve all via Steam API'}
        </button>
        {#if walkNote}<span class="note">{walkNote}</span>{/if}
      </p>
    {/if}

    <ul class="mods">
      {#each modIds as id (id)}
        {@const mod = itemFor(id)}
        <li>
          <label>
            <input type="checkbox" id={id} bind:checked={selected[id]} />
            <span class="name">{mod.name}</span>
            <span class="id">{id}</span>
          </label>

          <div class="deps">
            {#if mod.requires === null}
              <button onclick={() => resolve(id)} disabled={busy[id]}>
                {busy[id] ? 'checking...' : 'check dependencies'}
              </button>
            {:else if mod.requires.length === 0}
              <span class="none">no dependencies</span>
              <button class="link" onclick={() => resolve(id, true)} disabled={busy[id]}>refresh</button>
            {:else}
              <span class="label">requires:</span>
              {#each mod.requires as childId (childId)}
                {@const child = itemFor(childId)}
                {#if child.name === null}
                  <button onclick={() => resolve(childId)} disabled={busy[childId]}>
                    {busy[childId] ? 'fetching...' : childId}
                  </button>
                {:else if child.installed}
                  <span class="ok">{child.name}</span>
                {:else}
                  <span class="missing">{child.name} (not installed)</span>
                  {#if child.requires === null}
                    <button onclick={() => resolve(childId)} disabled={busy[childId]}>
                      {busy[childId] ? 'fetching...' : 'its dependencies'}
                    </button>
                  {/if}
                {/if}
              {/each}
              <button class="link" onclick={() => resolve(id, true)} disabled={busy[id]}>refresh</button>
            {/if}

            {#if errors[id]}
              <span class="error">{errors[id]}</span>
              <button onclick={() => resolve(id, true)}>retry</button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>

    {#if unmet.length > 0}
      <h2>Unmet requirements</h2>
      <ul class="unmet">
        {#each unmet as entry (entry.key)}
          <li><strong>{entry.mod}</strong> requires {entry.child}, {entry.reason}</li>
        {/each}
      </ul>
    {/if}

    <h2>Startup parameters</h2>
    <p class="hint options">
      <label><input type="checkbox" bind:checked={options.noSplash} /> -noSplash</label>
      <label><input type="checkbox" bind:checked={options.skipIntro} /> -skipIntro</label>
      <label><input type="checkbox" bind:checked={options.emptyWorld} /> -world=empty</label>
    </p>

    {#if plan}
      <h2>Launch</h2>
      <p class="hint">
        {plan.symlinks.length} symlink(s) will be created in the game directory. Status: {status}{pid ? ` (pid ${pid})` : ''}
      </p>
      <p class="hint">
        <button onclick={startGame} disabled={running}>launch</button>
        <button class="destructive" onclick={stopGame} disabled={!running}>stop</button>
        {#if launchError}<span class="error">{launchError}</span>{/if}
      </p>
      <details class="preview">
        <summary>details</summary>
        <pre>{plan.preview}</pre>
      </details>
    {/if}
  {:else}
    <h2>Save the last launch</h2>
    {#if store.lastLaunch}
      <p class="hint">
        Last launched: {describe(store.lastLaunch.ids, store.lastLaunch.options)}
      </p>
      <p class="hint save">
        <input
          type="text"
          placeholder="profile name"
          bind:value={profileName}
          onkeydown={(event) => { if (event.key === 'Enter') saveNamed() }}
        />
        <button onclick={saveNamed} disabled={profileName.trim() === ''}>save</button>
      </p>
    {:else}
      <p class="hint">Launch the game once, and what you selected shows up here to name.</p>
    {/if}
    {#if profileError}<p class="error">{profileError}</p>{/if}

    <h2>Saved profiles</h2>
    {#if store.profiles.length === 0}
      <p class="hint">Nothing saved yet.</p>
    {:else}
      <ul class="profiles">
        {#each store.profiles as profile (profile.name)}
          <li>
            <button class="link" onclick={() => applyProfile(profile)}>{profile.name}</button>
            <span class="note">{describe(profile.ids, profile.options)}</span>
            <button class="destructive" onclick={() => removeNamed(profile.name)}>delete</button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</main>

<style>
  main {
    font-family: system-ui, sans-serif;
    margin: 2rem auto;
    max-width: 46rem;
    padding: 0 1rem;
  }
  header {
    align-items: flex-start;
    display: flex;
    gap: 1rem;
    justify-content: space-between;
    margin-bottom: 1rem;
  }
  h1 { font-size: 1.6rem; letter-spacing: 0.01em; margin: 0; }
  .subtitle { color: #666; font-size: 0.9rem; font-style: italic; margin: 0.1rem 0 0; }
  h2 { font-size: 1rem; margin-bottom: 0.25rem; }
  pre {
    background: #f4f4f4;
    border: 1px solid #ddd;
    font-size: 0.8rem;
    overflow-x: auto;
    padding: 0.6rem;
    white-space: pre-wrap;
    word-break: break-all;
  }
  .hint { color: #666; font-size: 0.85rem; margin-top: 0; }
  ul.mods { list-style: none; padding: 0; }
  /* Pico puts markers on li, so clearing them on ul alone is not enough. */
  ul.mods > li { border-top: 1px solid #ddd; list-style: none; padding: 0.6rem 0; }
  label { display: flex; align-items: baseline; gap: 0.5rem; }
  .name { font-weight: 600; }
  .id { color: #888; font-size: 0.8rem; }
  .deps { align-items: baseline; display: flex; flex-wrap: wrap; gap: 0.4rem; margin: 0.35rem 0 0 1.6rem; }
  .label, .none, .note { color: #666; font-size: 0.85rem; }
  .ok { font-size: 0.85rem; }
  .missing { color: #a33; font-size: 0.85rem; }
  ul.unmet { color: #a33; font-size: 0.85rem; margin: 0; padding-left: 1.2rem; }
  .options { display: flex; flex-wrap: wrap; gap: 1rem; }
  .options label { gap: 0.3rem; }
  .nav { display: flex; gap: 0.4rem; margin: 0.6rem 0 0; }
  /* The 128px source at 64px, so it stays sharp on a retina display. */
  .icon { display: block; flex: none; height: 64px; width: 64px; }
  /* Pico draws a summary as a full width accordion row. The command line is a
     detail, not a section, so keep the toggle down at hint size. */
  .preview { margin: 0; }
  .preview summary { color: #666; font-size: 0.85rem; }
  /* Pico stretches inputs to the full width, which would push the save button
     onto its own line. */
  .save { align-items: center; display: flex; gap: 0.4rem; }
  .save input { font-size: 0.85rem; margin: 0; padding: 0.2rem 0.5rem; width: 16rem; }
  ul.profiles { list-style: none; padding: 0; }
  ul.profiles > li {
    align-items: baseline;
    border-top: 1px solid #ddd;
    display: flex;
    gap: 0.6rem;
    list-style: none;
    padding: 0.5rem 0;
  }
  ul.profiles .link { font-size: 0.95rem; font-weight: 600; }
  ul.profiles .note { margin-right: auto; }
  .error { color: #a33; font-size: 0.85rem; }
  /* Pico sizes every button as a primary call to action. This is a utility
     window, so keep them compact and let the content lead. */
  button {
    font-size: 0.8rem;
    margin: 0;
    padding: 0.2rem 0.6rem;
    width: auto;
  }
  /* Pico ships no danger variant, so stopping a running game gets its own
     rule. The red is Pico's own --pico-del-color, darkened, so it belongs to
     the same palette in both light and dark themes. */
  button.destructive {
    background-color: color-mix(in srgb, var(--pico-del-color) 85%, black);
    border-color: color-mix(in srgb, var(--pico-del-color) 85%, black);
  }
  button.destructive:hover:not(:disabled) {
    background-color: var(--pico-del-color);
    border-color: var(--pico-del-color);
  }
  button.link {
    background: none;
    border: none;
    color: var(--pico-primary);
    cursor: pointer;
    padding: 0;
    text-decoration: underline;
  }
  label { margin: 0; }
  input[type='checkbox'] { margin: 0; }
</style>

export interface Item {
  id: string
  name: string | null
  installed: boolean
  requires: string[] | null
  source: 'disk' | 'cache' | 'unknown'
  fetchedAt: string
}

export interface Snapshot {
  mods: Item[]
  referenced: Item[]
  /** True when a Steam Web API key is available for the batched walk. */
  apiAvailable: boolean
  /** Desktop only: false when a Steam path was guessed rather than confirmed. */
  gameVerified?: boolean
  workshopVerified?: boolean
}

export interface LaunchOptions {
  noSplash?: boolean
  skipIntro?: boolean
  emptyWorld?: boolean
  intelMode?: boolean
  steamOverlay?: boolean
}

export interface LaunchPlan {
  executable: string
  cwd: string
  args: string[]
  env: Record<string, string>
  symlinks: { target: string; link: string }[]
  preview: string
}

export interface Launched {
  pid: number | null
  removed: number
  created: number
}

/** What was selected the last time a launch succeeded. */
export interface LastLaunch {
  ids: string[]
  options: LaunchOptions
  overrides: Record<string, string>
}

export interface Profile {
  name: string
  ids: string[]
  options: LaunchOptions
  overrides: Record<string, string>
}

export interface Profiles {
  lastLaunch: LastLaunch | null
  profiles: Profile[]
}

export type LaunchEvent =
  | { type: 'state'; running: boolean; pid: number | null }
  | { type: 'linked'; removed: number; created: number }
  | { type: 'spawned'; pid: number }
  | { type: 'exited'; code: number | null; signal?: string | null }
  | { type: 'error'; message: string }

export class ResolveError extends Error {
  readonly rateLimited: boolean

  constructor(message: string, rateLimited: boolean) {
    super(message)
    this.name = 'ResolveError'
    this.rateLimited = rateLimited
  }
}

/**
 * The one interface the UI talks to. Two implementations satisfy it: HTTP
 * against the Node server for browser mode, and Tauri commands against the
 * Rust library for the desktop app. Components never learn which is in use.
 */
export interface Adapter {
  getSnapshot(): Promise<Snapshot>
  rescan(): Promise<Snapshot>
  /** Discard every cached dependency lookup. Installed names are unaffected. */
  resetCache(): Promise<Snapshot>
  resolveItem(id: string, refresh: boolean): Promise<Item>
  walkAll(): Promise<{ calls: number; resolved: number }>
  getPreview(ids: string[], options: LaunchOptions, overrides: Record<string, string>): Promise<LaunchPlan>
  launch(ids: string[], options: LaunchOptions, overrides: Record<string, string>): Promise<Launched>
  stop(): Promise<void>
  /** Subscribe to session events. Returns an unsubscribe function. */
  subscribe(handler: (event: LaunchEvent) => void): () => void
  /**
   * Profiles are stored by the Rust backend, so these three reject in browser
   * mode. The UI hides the profiles view there rather than calling them.
   * Each returns the whole store so the caller restates rather than patches.
   */
  listProfiles(): Promise<Profiles>
  saveProfile(name: string): Promise<Profiles>
  deleteProfile(name: string): Promise<Profiles>
  /** Opens a native folder picker. Desktop only; browser mode rejects. */
  pickOverrideFolder(): Promise<string | null>
}

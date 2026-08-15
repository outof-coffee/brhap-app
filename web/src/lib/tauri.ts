import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'

import {
  ResolveError,
  type Adapter,
  type Item,
  type LaunchEvent,
  type LaunchOptions,
  type LaunchPlan,
  type Launched,
  type Profiles,
  type Snapshot,
} from './types'

/** Tauri surfaces command errors as a rejected promise carrying the string. */
function message(error: unknown): string {
  return typeof error === 'string' ? error : error instanceof Error ? error.message : String(error)
}

/**
 * Desktop mode: commands straight into the Rust library, with Tauri events in
 * place of the SSE stream. No HTTP anywhere in this path.
 */
export const tauriAdapter: Adapter = {
  getSnapshot(): Promise<Snapshot> {
    return invoke('list_mods')
  },

  rescan(): Promise<Snapshot> {
    return invoke('rescan')
  },

  resetCache(): Promise<Snapshot> {
    return invoke('reset_cache')
  },

  async resolveItem(id: string, refresh: boolean): Promise<Item> {
    try {
      return await invoke<Item>('resolve_item', { id, refresh })
    } catch (error) {
      const text = message(error)
      throw new ResolveError(
        text.includes('rate limited')
          ? 'Steam is rate limiting us. Wait a moment and try again.'
          : text,
        text.includes('rate limited'),
      )
    }
  },

  walkAll(): Promise<{ calls: number; resolved: number }> {
    return invoke('walk_all')
  },

  getPreview(ids: string[], options: LaunchOptions, overrides: Record<string, string>): Promise<LaunchPlan> {
    return invoke('preview', { ids, options, overrides })
  },

  launch(ids: string[], options: LaunchOptions, overrides: Record<string, string>): Promise<Launched> {
    return invoke('launch', { ids, options, overrides })
  },

  stop(): Promise<void> {
    return invoke('stop')
  },

  subscribe(handler: (event: LaunchEvent) => void): () => void {
    // listen resolves to its unlisten function, so unsubscribing has to wait
    // for that promise even if the caller unsubscribes immediately.
    let stop: (() => void) | null = null
    let cancelled = false

    listen<LaunchEvent>('session-event', (message) => handler(message.payload)).then((unlisten) => {
      if (cancelled) unlisten()
      else stop = unlisten
    })

    return () => {
      cancelled = true
      stop?.()
    }
  },

  listProfiles(): Promise<Profiles> {
    return invoke('list_profiles')
  },

  saveProfile(name: string): Promise<Profiles> {
    return invoke('save_profile', { name })
  },

  deleteProfile(name: string): Promise<Profiles> {
    return invoke('delete_profile', { name })
  },

  async pickOverrideFolder(): Promise<string | null> {
    const picked = await open({ directory: true, multiple: false })
    return picked ?? null
  },
}

import { isTauri } from '@tauri-apps/api/core'

import { httpAdapter } from './http'
import { tauriAdapter } from './tauri'
import type { Adapter, LaunchEvent, LaunchOptions } from './types'

export * from './types'

/**
 * Pick the transport once, at load. Inside the desktop app this is the Tauri
 * adapter talking straight to the Rust library; in a browser it is the HTTP
 * adapter talking to the Node server. Nothing above this line knows which.
 */
const adapter: Adapter = isTauri() ? tauriAdapter : httpAdapter

/** Which transport is live, for display and debugging. */
export const mode = isTauri() ? 'desktop' : 'browser'

export const getSnapshot = () => adapter.getSnapshot()
export const rescan = () => adapter.rescan()
export const resetCache = () => adapter.resetCache()
export const resolveItem = (id: string, refresh = false) => adapter.resolveItem(id, refresh)
export const walkAll = () => adapter.walkAll()
export const getPreview = (ids: string[], options: LaunchOptions = {}) =>
  adapter.getPreview(ids, options)
export const launch = (ids: string[], options: LaunchOptions = {}) => adapter.launch(ids, options)
export const stop = () => adapter.stop()
export const subscribe = (handler: (event: LaunchEvent) => void) => adapter.subscribe(handler)
export const listProfiles = () => adapter.listProfiles()
export const saveProfile = (name: string) => adapter.saveProfile(name)
export const deleteProfile = (name: string) => adapter.deleteProfile(name)

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

const DESKTOP_ONLY = 'profiles are only available in the desktop app'

async function post(path: string, body?: unknown): Promise<Response> {
  return fetch(path, {
    method: 'POST',
    headers: body === undefined ? undefined : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
}

async function json<T>(response: Response, what: string): Promise<T> {
  const body = await response.json().catch(() => null)
  if (!response.ok) throw new Error(body?.error ?? `${what} failed: ${response.status}`)
  return body as T
}

/** Browser mode: the Node server over HTTP, with SSE for session events. */
export const httpAdapter: Adapter = {
  async getSnapshot(): Promise<Snapshot> {
    return json(await fetch('/api/mods'), 'GET /api/mods')
  },

  async rescan(): Promise<Snapshot> {
    return json(await post('/api/rescan'), 'rescan')
  },

  async resetCache(): Promise<Snapshot> {
    return json(await post('/api/cache/reset'), 'cache reset')
  },

  async resolveItem(id: string, refresh: boolean): Promise<Item> {
    const response = await post('/api/resolve', { id, refresh })
    if (response.status === 429) {
      throw new ResolveError('Steam is rate limiting us. Wait a moment and try again.', true)
    }
    const body = await response.json().catch(() => null)
    if (!response.ok) {
      throw new ResolveError(body?.error ?? `resolve failed: ${response.status}`, false)
    }
    return body.item
  },

  async walkAll(): Promise<{ calls: number; resolved: number }> {
    return json(await post('/api/walk'), 'walk')
  },

  async getPreview(ids: string[], options: LaunchOptions): Promise<LaunchPlan> {
    return json(await post('/api/preview', { ids, options }), 'preview')
  },

  async launch(ids: string[], options: LaunchOptions): Promise<Launched> {
    return json(await post('/api/launch', { ids, options }), 'launch')
  },

  async stop(): Promise<void> {
    await json(await post('/api/stop'), 'stop')
  },

  subscribe(handler: (event: LaunchEvent) => void): () => void {
    const stream = new EventSource('/api/events')
    stream.onmessage = (message) => handler(JSON.parse(message.data))
    return () => stream.close()
  },

  // The Node server stores no profiles. These exist to satisfy the interface;
  // the UI hides the profiles view outside the desktop app.
  async listProfiles(): Promise<Profiles> {
    throw new Error(DESKTOP_ONLY)
  },

  async saveProfile(): Promise<Profiles> {
    throw new Error(DESKTOP_ONLY)
  },

  async deleteProfile(): Promise<Profiles> {
    throw new Error(DESKTOP_ONLY)
  },
}

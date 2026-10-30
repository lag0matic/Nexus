import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { OBSERVER_PANELS, observerPanel } from './panels'

describe('browser panel registry', () => {
  it('matches every native pop-out title and the server routes', () => {
    const native = readFileSync(new URL('../DetachedPanel.tsx', import.meta.url), 'utf8')
    const slugs = [...native.matchAll(/panel === '([a-zA-Z]+)'/g)].map(m => m[1] === 'connect' ? 'conditions' : m[1])
    const ids = OBSERVER_PANELS.map(p => p.id)
    expect(slugs.length).toBeGreaterThanOrEqual(18)
    expect(new Set(ids)).toEqual(new Set(slugs))
    const rust = readFileSync(new URL('../../../src-tauri/src/observer_views.rs', import.meta.url), 'utf8')
    const registry = rust.slice(rust.indexOf('const PANELS:'), rust.indexOf('fn text('))
    for (const id of ids) expect(registry).toContain(`"${id}"`)
  })
  it('recognizes deep links and the native conditions alias', () => {
    for (const panel of OBSERVER_PANELS) expect(observerPanel(panel.id)).toBe(panel.id)
    expect(observerPanel('connect')).toBe('conditions')
    expect(observerPanel('set_tx_enabled')).toBe('pota')
  })
})

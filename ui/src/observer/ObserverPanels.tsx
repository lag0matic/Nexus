import { useEffect, useRef, useState } from 'react'
import { Crosshair, Search } from 'lucide-react'
import { ConnectView } from '../components/ConnectView'
import { TV_PRESETS } from '../features/connectPresets'
import { getNeedAlerts, getOtaSpots, getPropagation, getTvStation } from '../api'
import type { NeedTag, PropagationSnapshot } from '../types'
import { useViewport } from '../useViewport'
import { bandFromKhz } from '../otaHunt'
import { listenMode, listenToSpot, observerState, type ObserverRadio } from './client'

type Panel = 'pota' | 'conditions' | 'needed'
type Row = { key: string; call: string; band: string; mhz: number | null; mode: string | null; detail: string }
const PANELS: { id: Panel; label: string }[] = [{ id: 'pota', label: 'POTA' }, { id: 'conditions', label: 'Conditions' }, { id: 'needed', label: 'Needed' }]
function currentPanel(): Panel {
  const slug = location.pathname.split('/').pop()
  return slug === 'conditions' || slug === 'needed' ? slug : 'pota'
}

export function ObserverPanels() {
  useViewport()
  const [panel, setPanel] = useState<Panel>(currentPanel)
  const [radio, setRadio] = useState<ObserverRadio | null>(null)
  const [linked, setLinked] = useState(false)
  const [rows, setRows] = useState<Row[]>([])
  const [prop, setProp] = useState<PropagationSnapshot | null>(null)
  const [grid, setGrid] = useState('')
  const [selectedCall, setSelectedCall] = useState<string | null>(null)
  const [search, setSearch] = useState('')
  const [error, setError] = useState('')
  const [lastRead, setLastRead] = useState(0)
  const [busy, setBusy] = useState(false)
  const click = useRef(false)
  const emptyNeeds = useRef(new Map<string, NeedTag>())

  useEffect(() => {
    const back = () => setPanel(currentPanel())
    window.addEventListener('popstate', back)
    return () => window.removeEventListener('popstate', back)
  }, [])

  useEffect(() => {
    let live = true
    let flight = false
    const load = async () => {
      if (flight) return
      flight = true
      try {
        const value = await observerState()
        if (live) { setRadio(value); setLinked(true) }
      } catch { if (live) setLinked(false) }
      finally { flight = false }
    }
    void load()
    const timer = window.setInterval(() => void load(), 1000)
    return () => { live = false; window.clearInterval(timer) }
  }, [])

  useEffect(() => {
    let live = true
    let flight = false
    setRows([]); setError(''); setLastRead(0)
    const load = async () => {
      if (flight) return
      flight = true
      try {
        let next: Row[] = []
        if (panel === 'pota') {
          const spots = await getOtaSpots('POTA')
          next = spots.map((s) => ({ key: `${s.activator}:${s.reference}:${s.freqKhz}`, call: s.activator,
            band: bandFromKhz(s.freqKhz), mhz: s.freqKhz / 1000, mode: listenMode(s.mode), detail: `${s.reference} ${s.name}` }))
        } else if (panel === 'needed') {
          const needs = await getNeedAlerts()
          next = needs.map((n) => ({ key: `${n.call}:${n.band}:${n.exactMode}`, call: n.call, band: n.band,
            mhz: n.freqMhz, mode: listenMode(n.exactMode, n.mode), detail: `${n.entity} · ${n.headline}` }))
        } else {
          const [conditions, station] = await Promise.all([getPropagation(), getTvStation()])
          if (live) { setProp(conditions); setGrid(station.grid) }
        }
        if (live) { setRows(next); setLastRead(Date.now()); setError('') }
      } catch (e) { if (live) setError(e instanceof Error ? e.message : String(e)) }
      finally { flight = false }
    }
    void load()
    const timer = window.setInterval(() => void load(), 30_000)
    return () => { live = false; window.clearInterval(timer) }
  }, [panel])

  const tune = async (row: Pick<Row, 'mhz' | 'mode' | 'call'>) => {
    if (click.current || !linked || !radio || !row.mhz || !row.mode || Date.now() - lastRead > 90_000) return
    click.current = true; setBusy(true); setError('')
    try {
      await listenToSpot(row.mhz, row.mode, radio.radio)
      setSelectedCall(row.call)
      setRadio(await observerState())
    } catch (e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { click.current = false; setBusy(false) }
  }
  const disabled = !linked || radio?.catOk !== true || radio?.busy || busy || Date.now() - lastRead > 90_000
  const query = search.trim().toUpperCase()
  const filtered = rows.filter((r) => !query || `${r.call} ${r.band} ${r.mode} ${r.detail}`.toUpperCase().includes(query))

  return <div className="app observer-app">
    <header className="observer-header">
      <strong>Nexus</strong>
      <nav aria-label="Panels">{PANELS.map(({ id, label }) => <a key={id}
        href={`/panels/${id}`}
        aria-current={panel === id ? 'page' : undefined}
        onClick={(e) => { if (e.ctrlKey || e.metaKey) return; e.preventDefault(); history.pushState(null, '', `/panels/${id}`); setSearch(''); setPanel(id) }}>{label}</a>)}</nav>
      <span className="observer-radio" role="status">{linked && radio ? `${radio.mhz.toFixed(4)} MHz · ${radio.mode === 'digital' ? radio.tier : radio.mode.toUpperCase()}` : 'Disconnected'}</span>
    </header>
    {error && <div className="observer-error" role="alert">{error}</div>}
    {panel === 'conditions' ? <section className="observer-conditions">
      <ConnectView myGrid={grid} theme="dark" stations={[]} prop={prop} selectedCall={selectedCall}
        onSelectCall={setSelectedCall} needByCall={emptyNeeds.current} presets={TV_PRESETS} hostBar
        onWorkSpot={(spot) => { const mode = listenMode(spot.mode)
          if (!spot.freqMhz || !mode) { setError('This spot has no supported exact frequency or mode'); return }
          void tune({ call: spot.call, mhz: spot.freqMhz, mode }) }} />
    </section> : <main className="observer-list">
      <label className="observer-search"><Search size={16} aria-hidden="true" /><input type="search" aria-label="Filter spots" placeholder="Call, band, mode or reference" value={search} onChange={(e) => setSearch(e.target.value)} /></label>
      <div className="observer-rows" aria-busy={lastRead === 0 && !error}>
        {lastRead === 0 && !error ? <p>Loading…</p> : !filtered.length ? <p>No matching spots.</p> : filtered.map((row) => {
          const available = !disabled && row.mhz != null && row.mode != null && Number.isFinite(row.mhz)
          return <button type="button" key={row.key} className="observer-spot" disabled={!available}
            aria-label={`Listen to ${row.call}`} aria-pressed={selectedCall === row.call}
            title={!row.mode || !row.mhz ? 'No supported exact frequency and mode reported' : `Listen on ${row.mhz.toFixed(4)} MHz ${row.mode}`}
            onClick={() => void tune(row)}>
            <strong>{row.call}</strong><span className="mono">{row.mhz?.toFixed(4) ?? '—'}</span><span>{row.mode ?? 'Unknown'}</span><Crosshair size={16} aria-hidden="true" />
            <small>{row.band} · {row.detail}</small>
          </button>
        })}
      </div>
    </main>}
  </div>
}

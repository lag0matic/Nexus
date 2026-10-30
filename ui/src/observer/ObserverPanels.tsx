import { useEffect, useRef, useState } from 'react'
import { Crosshair, Search, ExternalLink, RefreshCw } from 'lucide-react'
import { ConnectView } from '../components/ConnectView'
import { TV_PRESETS } from '../features/connectPresets'
import { getNeedAlerts, getOtaSpots, getPropagation, getTvStation } from '../api'
import type { NeedTag, PropagationSnapshot } from '../types'
import { useViewport } from '../useViewport'
import { bandFromKhz } from '../otaHunt'
import { listenMode, listenToSpot, observerState, observerPanelData, type MonitorData, type ObserverRadio } from './client'
import { OBSERVER_PANELS, observerPanel, POTA_BANDS, filterMode, type ObserverPanel } from './panels'
import { MonitorPanel } from './MonitorPanel'

type Panel = ObserverPanel
type Row = { key: string; call: string; band: string; mhz: number | null; mode: string | null; detail: string }
const PANELS: { id: Panel; label: string }[] = [{ id: 'pota', label: 'POTA' }, { id: 'conditions', label: 'Conditions' }, { id: 'needed', label: 'Needed' }]
function currentPanel(): Panel { return observerPanel(location.pathname.split('/').pop()) }

export function ObserverPanels() {
  useViewport()
  const [panel, setPanel] = useState<Panel>(currentPanel)
  const [radio, setRadio] = useState<ObserverRadio | null>(null)
  const [linked, setLinked] = useState(false)
  const [rows, setRows] = useState<Row[]>([])
  const [monitor, setMonitor] = useState<MonitorData | null>(null)
  const [band, setBand] = useState('All')
  const [modeFilter, setModeFilter] = useState('All')
  const [prop, setProp] = useState<PropagationSnapshot | null>(null)
  const [grid, setGrid] = useState('')
  const [selectedCall, setSelectedCall] = useState<string | null>(null)
  const [search, setSearch] = useState('')
  const [error, setError] = useState('')
  const [lastRead, setLastRead] = useState(0)
  const [busy, setBusy] = useState(false)
  const click = useRef(false)
  const refresh = useRef<(() => Promise<void>) | null>(null)
  const [reading, setReading] = useState(false)
  const emptyNeeds = useRef(new Map<string, NeedTag>())

  useEffect(() => {
    const back = () => { setSearch(''); setPanel(currentPanel()) }
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
    const timer = window.setInterval(() => { if (!document.hidden) void load() }, 1000)
    const visible = () => { if (!document.hidden) void load() }
    document.addEventListener('visibilitychange', visible)
    return () => { live = false; window.clearInterval(timer); document.removeEventListener('visibilitychange', visible) }
  }, [])

  useEffect(() => {
    let live = true
    let flight = false
    setRows([]); setMonitor(null); setError(''); setLastRead(0)
    const load = async () => {
      if (flight) return
      flight = true; setReading(true)
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
        } else if (panel === 'conditions') {
          const [conditions, station] = await Promise.all([getPropagation(), getTvStation()])
          if (live) { setProp(conditions); setGrid(station.grid) }
        } else {
          const value = await observerPanelData(panel)
          if (live) setMonitor(value)
        }
        if (live) { setRows(next); setLastRead(Date.now()); setError('') }
      } catch (e) { if (live) setError(e instanceof Error ? e.message : String(e)) }
      finally { flight = false; if (live) setReading(false) }
    }
    refresh.current = load
    void load()
    const interval = panel === 'waterfall' ? 500 : ['pota', 'conditions', 'needed', 'dxped', 'sats'].includes(panel) ? 30_000 : 2000
    const timer = window.setInterval(() => { if (!document.hidden) void load() }, interval)
    const visible = () => { if (!document.hidden) void load() }
    document.addEventListener('visibilitychange', visible)
    return () => { live = false; refresh.current = null; window.clearInterval(timer); document.removeEventListener('visibilitychange', visible) }
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
  const filtered = rows.filter((r) => (panel !== 'pota' || (band === 'All' || r.band === band) && (modeFilter === 'All' || filterMode(r.mode) === modeFilter)) &&
    (!query || `${r.call} ${r.band} ${r.mode} ${r.detail}`.toUpperCase().includes(query)))
  const modes = [...new Set(['CW', 'SSB', 'FT8', 'FT4', 'FM', 'AM', ...rows.map(r => filterMode(r.mode)), modeFilter])].filter(m => m !== 'All')
  const navigate = (id: Panel) => { history.pushState(null, '', `/panels/${id}`); setSearch(''); setPanel(id) }

  return <div className="app observer-app">
    <header className="observer-header">
      <strong>Nexus</strong>
      <nav aria-label="Panels">{PANELS.map(({ id, label }) => <a key={id}
        href={`/panels/${id}`}
        aria-current={panel === id ? 'page' : undefined}
        onClick={(e) => { if (e.ctrlKey || e.metaKey) return; e.preventDefault(); navigate(id) }}>{label}</a>)}</nav>
      <select aria-label="Panel" value={panel} onChange={e => navigate(observerPanel(e.target.value))}>
        {OBSERVER_PANELS.map(p => <option key={p.id} value={p.id}>{p.label}</option>)}
      </select>
      <a className="observer-new-tab" href={`/panels/${panel}`} target="_blank" rel="noopener noreferrer"
        aria-label="Open selected panel in another tab" title="Open selected panel in another tab"><ExternalLink size={17} aria-hidden="true" /></a>
      <button type="button" className="observer-refresh" disabled={reading} aria-label="Refresh panel" title="Refresh panel"
        onClick={() => void refresh.current?.()}><RefreshCw size={17} aria-hidden="true" /></button>
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
      {panel !== 'waterfall' && <label className="observer-search"><Search size={16} aria-hidden="true" /><input type="search" aria-label={panel === 'pota' || panel === 'needed' ? 'Filter spots' : 'Filter entries'} placeholder="Call, band, mode or reference" value={search} onChange={(e) => setSearch(e.target.value)} /></label>}
      {panel === 'pota' && <div className="observer-filters">
        <div role="group" aria-label="POTA bands">{['All', ...POTA_BANDS, ...rows.map(r => r.band).filter(b => b && !POTA_BANDS.includes(b))].filter((b, i, all) => all.indexOf(b) === i).map(b =>
          <button type="button" key={b} aria-pressed={band === b} onClick={() => setBand(b)}>{b === 'All' ? 'All bands' : b}</button>)}</div>
        <div role="group" aria-label="POTA modes">{['All', ...modes].map(m =>
          <button type="button" key={m} aria-pressed={modeFilter === m} onClick={() => setModeFilter(m)}>{m === 'All' ? 'All modes' : m}</button>)}</div>
        <output aria-live="polite">{filtered.length} / {rows.length} spots</output>
      </div>}
      <div className="observer-rows" aria-busy={lastRead === 0 && !error}>
        {monitor ? <MonitorPanel key={panel} data={monitor} query={query} disabled={!!disabled} onListen={row => void tune(row)} /> : lastRead === 0 && !error ? <p>Loading…</p> : !filtered.length ? <p>No matching spots.</p> : filtered.map((row) => {
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

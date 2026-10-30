import { useEffect, useRef } from 'react'
import { Crosshair } from 'lucide-react'
import { listenMode, type MonitorData } from './client'
import { observerPanel, OBSERVER_PANELS } from './panels'
import { MapView } from '../components/MapView'

function MonitorMedia({ media }: { media: MonitorData['media'] }) {
  const canvas = useRef<HTMLCanvasElement>(null)
  const previous = useRef('')
  useEffect(() => {
    const c = canvas.current
    const ctx = c?.getContext('2d')
    if (!c || !ctx) return
    if (media.rgb && media.width && media.height) {
      const bytes = atob(media.rgb)
      if (bytes.length !== media.width * media.height * 3) return
      c.width = media.width; c.height = media.height
      const image = ctx.createImageData(c.width, c.height)
      for (let i = 0; i < bytes.length / 3; i++) {
        for (let ch = 0; ch < 3; ch++) image.data[i * 4 + ch] = bytes.charCodeAt(i * 3 + ch)
        image.data[i * 4 + 3] = 255
      }
      ctx.putImageData(image, 0, 0)
    } else if (media.spectrum?.length) {
      const key = JSON.stringify(media.spectrum)
      if (key === previous.current) return
      previous.current = key
      if (c.width !== media.spectrum.length) { c.width = media.spectrum.length; c.height = 300 }
      ctx.drawImage(c, 0, 0, c.width, c.height - 1, 0, 1, c.width, c.height - 1)
      const row = ctx.createImageData(c.width, 1)
      media.spectrum.forEach((power, x) => {
        const p = Math.max(0, Math.min(1, power))
        row.data[x * 4] = Math.max(0, (p - .6) * 638)
        row.data[x * 4 + 1] = Math.max(0, (p - .2) * 319)
        row.data[x * 4 + 2] = Math.min(255, p * 700)
        row.data[x * 4 + 3] = 255
      })
      ctx.putImageData(row, 0, 0)
    }
  }, [media])
  if (media.bmp) return <img className="observer-picture" src={`data:image/bmp;base64,${media.bmp}`} alt="Last received SSTV picture" />
  if (media.spectrum) return <figure className="observer-media">
    <canvas ref={canvas} role="img" aria-label="Receive audio waterfall" />
    <figcaption><span>{media.loHz ?? 0} Hz</span><span>{media.hiHz ?? 4000} Hz</span></figcaption>
    {!media.spectrum.length && <p>No receive audio spectrum available.</p>}
  </figure>
  if (media.rgb) return <figure className="observer-media"><canvas ref={canvas} role="img" aria-label="Incoming SSTV picture" /></figure>
  return null
}

export function MonitorPanel({ data, query, disabled, onListen }: {
  data: MonitorData; query: string; disabled: boolean
  onListen: (row: { call: string; mhz: number; mode: string }) => void
}) {
  return <div className="observer-monitor">
    <h1>{OBSERVER_PANELS.find(p => p.id === observerPanel(data.panel))?.label}</h1>
    {!!data.metrics.length && <dl className="observer-metrics">{data.metrics.map(([label, value]) =>
      <div key={label}><dt>{label}</dt><dd>{value || 'Not active'}</dd></div>)}</dl>}
    <MonitorMedia media={data.media} />
    {data.media.stations && <div className="observer-map"><MapView myGrid={data.media.myGrid ?? ''} theme="dark"
      stations={data.media.stations} prop={null} selectedCall={null} onSelectCall={() => {}}
      needByCall={new Map()} /></div>}
    {data.groups.map(group => {
      const frequency = group.columns.indexOf('MHz')
      const modeIndex = group.columns.indexOf('Mode')
      const classIndex = group.columns.indexOf('Class')
      const rows = group.rows.filter(row => !query || row.join(' ').toUpperCase().includes(query))
      return <section key={group.title} className="observer-table-section">
        <h2>{group.title} <small>{group.rows.length < group.total ? `${group.rows.length} of ${group.total}` : group.total}</small></h2>
        {!rows.length ? <p>{group.total ? 'No matching entries.' : 'No entries available.'}</p> : <div className="observer-table-scroll"><table>
          <thead><tr>{group.columns.map(column => <th key={column}>{column}</th>)}{frequency >= 0 && <th aria-label="Listen" />}</tr></thead>
          <tbody>{rows.map((row, index) => {
            const mhz = frequency >= 0 ? Number(row[frequency]) : NaN
            const mode = listenMode(row[modeIndex], classIndex >= 0 ? row[classIndex] : undefined)
            return <tr key={`${row.join(':')}:${index}`}>{row.map((cell, i) => <td key={group.columns[i]}>{cell || '-'}</td>)}
              {frequency >= 0 && <td><button type="button" disabled={disabled || !mode || !Number.isFinite(mhz) || mhz <= 0}
                title={`Listen to ${row[0]}`} aria-label={`Listen to ${row[0]}`}
                onClick={() => { if (mode && mhz > 0) onListen({ call: row[0], mhz, mode }) }}><Crosshair size={17} aria-hidden="true" /></button></td>}
            </tr>
          })}</tbody>
        </table></div>}
      </section>
    })}
  </div>
}

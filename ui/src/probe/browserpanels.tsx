import '../styles.css'
import { createRoot } from 'react-dom/client'
import { BrowserPanelShare } from '../components/BrowserPanelShare'
import { useEscStop } from '../useEscStop'

const state = { down: 0, up: 0, macro: 0, stop: 0, keyed: false, commands: [] as string[] }
let running = false
;(window as unknown as { __shareProbe: () => typeof state }).__shareProbe = () => ({ ...state, commands: [...state.commands] })
const control = new URLSearchParams(location.search).get('control') === '1'
window.addEventListener('keydown', e => {
  if (e.code === 'Space') { state.down++; state.keyed = true }
  if (e.code === 'F1') state.macro++
}, control)
window.addEventListener('keyup', e => { if (e.code === 'Space') { state.up++; state.keyed = false } })
window.__TAURI_INTERNALS__ = { invoke: async <T,>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
  if (cmd !== 'observer_panel_action') throw new Error(`Unexpected command: ${cmd}`)
  const action = args?.action as { kind: string }
  state.commands.push(action.kind)
  if (action.kind === 'start') running = true
  if (action.kind === 'stop') running = false
  return { running, address: '192.168.1.10', port: 7375, accessCode: running ? '12345678' : null,
    links: running ? { pota: 'http://192.168.1.10:7375' } : {} } as T
} }
document.documentElement.dataset.theme = 'dark'
function Scene() {
  useEscStop(true, () => { state.stop++; state.keyed = false })
  return <div className="app"><header className="topbar"><button id="outside" type="button">Outside control</button>
    <BrowserPanelShare panel="pota" label="Browser panels" /></header></div>
}
createRoot(document.getElementById('root')!).render(<Scene />)

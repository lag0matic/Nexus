import '../styles.css'
import { createRoot } from 'react-dom/client'
import { Logbook } from '../components/Logbook'
import { useViewport } from '../useViewport'
import { answerFrom, type LogQuestion } from '../features/logAnswers'
import type { LoggedQso } from '../types'
import settings from '../components/__fixtures__/defaultSettings.json'

document.documentElement.dataset.theme = 'dark'
localStorage.setItem('nexus.logbook.globe', 'off')
const destination = { longwaveUrl: 'https://log.example.invalid', longwaveLogbookId: 'book' }
const contact = (call: string, id: string): LoggedQso => ({
  id, call, grid: 'FN31', band: '20m', mode: 'CW', freqMhz: 14.0475,
  country: 'United States', whenUnix: 1793016000, timeKnown: true,
  confirmed: false, awardConfirmed: false, rstSent: '599', rstRcvd: '579',
  name: null, qth: null, comment: null, notes: null,
  qslRcvd: { card: false, lotw: false, eqsl: false, qrz: false },
  qslSent: { sent: false, via: null, dateUnix: null },
  ota: { myProgram: null, myRef: null, theirProgram: null, theirRef: null, iota: null }, extra: [], upload: {},
} as LoggedQso)
let rows = [contact('W1AW', 'local-a')]
let revision = 1
window.__TAURI_INTERNALS__ = {
  invoke: async <T,>(command: string, args?: Record<string, unknown>): Promise<T> => {
    if (command === 'ask_log') return answerFrom(rows, args?.q as LogQuestion, revision) as T
    if (command === 'longwave_sync_logbook') {
      rows = [rows[0], { ...contact('W2NEW', 'remote-b'), upload: { longwave: { outcome: 'accepted', whenUnix: 0 } } }]
      revision++
      document.documentElement.dataset.synced = 'yes'
      return { added: 1, skipped: 0, updated: 0, total: 2, run: 'run-a' } as T
    }
    if (command === 'longwave_push_qso') {
      const q = args?.record as LoggedQso
      rows = rows.map(row => row.id === q.id ? { ...row, upload: { longwave: { outcome: 'accepted', whenUnix: 1793016001 } } } : row)
      revision++
      document.documentElement.dataset.pushed = 'yes'
      return { dispatched: 1, outcome: 'accepted', detail: null } as T
    }
    if (command === 'get_settings') return { ...settings, ...destination } as T
    if (['log_operators', 'log_activations', 'lotw_sat_names', 'qso_history', 'get_credentials_status'].includes(command)) return [] as T
    throw new Error(`Unanswered logbook probe command: ${command}`)
  },
}
function Probe() {
  useViewport(1)
  return <div className="app"><Logbook defaultBand="20m" defaultFreqMhz={14.0475} defaultMode="CW"
    longwave={destination} columns={{ shown: ['upLongwave'], hidden: [], widths: {} }} /></div>
}
createRoot(document.getElementById('root')!).render(<Probe />)

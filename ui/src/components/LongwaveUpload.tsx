import { useEffect, useRef, useState } from 'react'
import { Upload } from 'lucide-react'
import { longwavePushQso } from '../api'
import { logSource } from '../features/logSource'
import { uploadMark, uploadWord } from '../features/logColumns'
import { t } from '../i18n'
import { pushToast } from '../toast'
import type { LoggedQso } from '../types'

/** A lost response is not permission to POST again. The backend also checks the live stamp. */
export function LongwaveUpload({ qso, onChanged }: { qso: LoggedQso; onChanged?: () => void }) {
  const [busy, setBusy] = useState(false)
  const [outcome, setOutcome] = useState<string | null>(null)
  const flight = useRef(false)
  const stored = qso.upload?.longwave
  const stamp = stored?.outcome
  useEffect(() => {
    // A fresh row is the station's answer even when IPC lost the command's reply. No stamp
    // is not evidence of a refusal: keep the optimistic pending state in that case.
    if (stored) setOutcome(local => local === null ? null : stored.outcome)
  }, [stored])
  const blocked = [stamp, outcome].some(s => s != null && !['rejected', 'authfail', 'none'].includes(s))
  const push = async () => {
    if (blocked || flight.current) return
    flight.current = true
    setBusy(true)
    // Until the station answers definitively, this request may have reached Longwave.
    setOutcome('pending')
    try {
      const report = await longwavePushQso(qso)
      setOutcome(report.outcome)
      const ok = report.outcome === 'accepted' || report.outcome === 'duplicate'
      pushToast(t('logbook.longwave.push.done', {
        call: qso.call, state: uploadWord(uploadMark({ outcome: report.outcome, whenUnix: 0 }), 'upLongwave'),
      }), ok ? 'success' : report.outcome === 'pending' ? 'info' : 'error')
    } catch (e) {
      pushToast(t('logbook.longwave.push.failed', { detail: e instanceof Error ? e.message : String(e) }), 'error')
    } finally {
      logSource().refresh()
      onChanged?.()
      flight.current = false
      setBusy(false)
    }
  }
  return <button type="button" className="export-btn longwave-action" disabled={busy || blocked}
    aria-busy={busy} title={busy ? t('logbook.longwave.push.busy') : stamp === 'pending' || outcome === 'pending' ? t('logbook.longwave.pending')
      : blocked ? t('logbook.longwave.push.blocked') : t('logbook.longwave.push.title', { call: qso.call })}
    onClick={() => void push()}>
    <Upload size={14} aria-hidden="true" />
    {t('logbook.longwave.push.label')}
  </button>
}

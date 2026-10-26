import { useRef, useState } from 'react'
import { RefreshCw } from 'lucide-react'
import { syncLongwaveLogbook } from '../api'
import { logSource } from '../features/logSource'
import { t } from '../i18n'
import { pushToast } from '../toast'
import type { ImportStats } from '../types'

/** Both entry points use the saved station destination, never an unsaved form. */
export function LongwaveSync({ disabled = false, title, className = 'export-btn', onImported }: {
  disabled?: boolean
  title?: string
  className?: string
  onImported?: (stats: ImportStats) => void
}) {
  const [busy, setBusy] = useState(false)
  const flight = useRef(false)
  const sync = async () => {
    if (disabled || flight.current) return
    flight.current = true
    setBusy(true)
    try {
      const stats = await syncLongwaveLogbook()
      logSource().refresh()
      if (onImported) onImported(stats)
      else pushToast(t('logbook.longwave.sync.done', {
        added: stats.added, skipped: stats.skipped, updated: stats.updated,
      }), 'success')
    } catch (e) {
      pushToast(t('logbook.longwave.sync.failed', { detail: e instanceof Error ? e.message : String(e) }), 'error')
    } finally {
      flight.current = false
      setBusy(false)
    }
  }
  return <button type="button" className={`${className} longwave-action`} disabled={disabled || busy}
    aria-busy={busy} title={busy ? t('logbook.longwave.sync.busy') : title ?? t('logbook.longwave.sync.title')}
    onClick={() => void sync()}>
    <RefreshCw size={14} aria-hidden="true" />
    {t('logbook.longwave.sync.label')}
  </button>
}

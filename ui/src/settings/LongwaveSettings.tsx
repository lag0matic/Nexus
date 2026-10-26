import { useEffect, useRef, useState } from 'react'
import { clearLongwaveToken, getLongwaveLogbooks, setLongwaveToken, type LongwaveLogbook } from '../api'
import { t } from '../i18n'
import { pushToast } from '../toast'
import type { Settings } from '../types'

/** A section of the existing logging form, not another logging workflow. */
export function LongwaveSettings({ form, remote, onChange }: {
  form: Settings
  remote: boolean
  onChange: (patch: Partial<Settings>) => void
}) {
  const [token, setToken] = useState('')
  const [books, setBooks] = useState<LongwaveLogbook[]>([])
  const [busy, setBusy] = useState(false)
  const [credentialBusy, setCredentialBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const request = useRef(0)
  const url = form.longwaveUrl ?? ''
  const currentUrl = useRef(url)
  currentUrl.current = url
  useEffect(() => () => { request.current++ }, [])
  const stationHint = remote
    ? <span className="settings-hint" data-set-at-station>{t('settings.stationOnly.hint')}</span>
    : null

  const credential = async (forget: boolean) => {
    setBusy(true)
    setCredentialBusy(true)
    setError(null)
    const id = ++request.current
    try {
      if (forget) await clearLongwaveToken()
      else await setLongwaveToken(token.trim())
      if (id !== request.current) return
      setToken('')
      setBooks([])
      pushToast(forget ? t('settings.longwave.token.removed') : t('settings.longwave.token.saved'), 'success')
    } catch (e) {
      if (id === request.current) setError(e instanceof Error ? e.message : String(e))
    } finally {
      setCredentialBusy(false)
      if (id === request.current) setBusy(false)
    }
  }

  const test = async () => {
    const id = ++request.current
    const testedUrl = url
    setBusy(true)
    setError(null)
    try {
      const result = await getLongwaveLogbooks(testedUrl)
      if (id !== request.current || testedUrl !== currentUrl.current) return
      setBooks(result)
      if (result.length === 1) onChange({ longwaveLogbookId: result[0].id })
      else if (!result.some(b => b.id === form.longwaveLogbookId)) onChange({ longwaveLogbookId: '' })
      if (!result.length) setError(t('settings.longwave.logbook.empty'))
      else pushToast(t('settings.longwave.connection.ok'), 'success')
    } catch (e) {
      if (id === request.current && testedUrl === currentUrl.current) {
        setBooks([])
        setError(e instanceof Error ? e.message : String(e))
      }
    } finally {
      if (id === request.current) setBusy(false)
    }
  }

  return <div className="settings-featgroup">
    <span className="settings-featgroup-title">{t('settings.longwave.title')}</span>
    <div className="settings-grid longwave-settings-grid">
      <label className="settings-field">
        <span className="settings-label">{t('settings.longwave.url.label')}</span>
        <input className="settings-input" data-setting="longwaveUrl" type="url" disabled={remote || credentialBusy}
          aria-label={t('settings.longwave.url.label')}
          value={url} autoComplete="off" spellCheck={false}
          onChange={e => {
            request.current++
            setBusy(false)
            setBooks([])
            setError(null)
            onChange({ longwaveUrl: e.target.value, longwaveLogbookId: '' })
          }} />
        {stationHint}
      </label>
      <div className="settings-field">
        <label className="settings-label" htmlFor="longwave-client-token">{t('settings.longwave.token.label')}</label>
        <div className="settings-input-row">
          <input id="longwave-client-token" className="settings-input" type="password" disabled={remote || busy}
            value={token} onChange={e => setToken(e.target.value)} autoComplete="off" spellCheck={false} />
          <button className="settings-refresh" type="button" disabled={remote || busy || !token.trim()}
            onClick={() => void credential(false)}>{t('settings.longwave.token.set')}</button>
          <button className="settings-refresh" type="button" disabled={remote || busy}
            onClick={() => void credential(true)}>{t('settings.longwave.token.forget')}</button>
        </div>
      </div>
      <div className="settings-field">
        <label className="settings-label" htmlFor="longwave-logbook">{t('settings.longwave.logbook.label')}</label>
        <div className="settings-input-row">
          <select id="longwave-logbook" className="settings-input" data-setting="longwaveLogbookId"
            disabled={remote || busy} value={form.longwaveLogbookId ?? ''}
            onChange={e => onChange({ longwaveLogbookId: e.target.value })}>
            <option value="">{t('settings.longwave.logbook.choose')}</option>
            {form.longwaveLogbookId && !books.some(b => b.id === form.longwaveLogbookId) &&
              <option value={form.longwaveLogbookId}>{form.longwaveLogbookId}</option>}
            {books.map(b => <option key={b.id} value={b.id}>{b.name} ({b.operator_callsign})</option>)}
          </select>
          <button className="settings-refresh" type="button" disabled={remote || busy || !url.trim()}
            onClick={() => void test()}>{busy ? t('settings.longwave.connection.busy') : t('settings.longwave.connection.test')}</button>
        </div>
        {stationHint}
      </div>
      <div className="settings-field">
        <label className="settings-toggle">
          <span className="settings-label">{t('settings.longwave.upload.label')}</span>
          <button type="button" role="switch" data-setting="longwaveUpload" aria-label={t('settings.longwave.upload.label')}
            aria-checked={form.longwaveUpload ?? false}
            disabled={remote}
            className={`toggle${form.longwaveUpload ? ' on' : ''}`}
            onClick={() => onChange({ longwaveUpload: !form.longwaveUpload })}><span className="toggle-knob" /></button>
        </label>
        {stationHint}
      </div>
    </div>
    {error && <span className="settings-error" role="alert">{error}</span>}
  </div>
}

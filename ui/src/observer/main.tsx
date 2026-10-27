import React from 'react'
import ReactDOM from 'react-dom/client'
import { ObserverPanels } from './ObserverPanels'
import '../styles.css'
import './observer.css'

// Keep the access link out of HTTP requests, referrers, logs and saved browser
// credentials. It lives in this tab's memory and expires when sharing stops.
const fragment = new URLSearchParams(location.hash.slice(1))
const access = fragment.get('access')
if (access && /^[a-f0-9]{64}$/.test(access)) window.__NEXUS_OBSERVER_ACCESS__ = access
history.replaceState(null, '', location.pathname)
window.__NEXUS_TV_RPC__ = '/observer/read'
document.documentElement.setAttribute('data-theme', 'dark')

class ObserverBoundary extends React.Component<{ children: React.ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() { return { failed: true } }
  render() {
    return this.state.failed ? <main className="observer-error" role="alert">Panel unavailable. Reopen its access link from the laptop.</main> : this.props.children
  }
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode><ObserverBoundary><ObserverPanels /></ObserverBoundary></React.StrictMode>,
)

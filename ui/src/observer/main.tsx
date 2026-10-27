import React from 'react'
import ReactDOM from 'react-dom/client'
import { ObserverAccess } from './ObserverAccess'
import '../styles.css'
import './observer.css'

// Credentials stay in this tab's memory, never the address or browser storage.
history.replaceState(null, '', location.pathname)
window.__NEXUS_TV_RPC__ = '/observer/read'
document.documentElement.setAttribute('data-theme', 'dark')

class ObserverBoundary extends React.Component<{ children: React.ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() { return { failed: true } }
  render() {
    return this.state.failed ? <main className="observer-error" role="alert">Panel unavailable. Reload to connect again.</main> : this.props.children
  }
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode><ObserverBoundary><ObserverAccess /></ObserverBoundary></React.StrictMode>,
)

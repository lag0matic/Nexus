import assert from 'node:assert/strict'
import { createServer } from 'vite'
import { mkdir, writeFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { chrome } from '../../remote/test/browser-runtime.mjs'

const ui = join(dirname(fileURLToPath(import.meta.url)), '..')
const server = await createServer({ root: ui, configFile: join(ui, 'vite.config.ts'), logLevel: 'warn', server: { host: '127.0.0.1', port: 0 } })
await server.listen()
let browser
try {
  browser = await chrome()
  const { targetId } = await browser.call('Target.createTarget', { url: 'about:blank' })
  const { sessionId } = await browser.call('Target.attachToTarget', { targetId, flatten: true })
  const call = (method, params = {}) => browser.call(method, params, sessionId)
  const errors = []
  browser.on('Runtime.exceptionThrown', ({ exceptionDetails }) => errors.push(exceptionDetails.exception?.description ?? exceptionDetails.text))
  await call('Runtime.enable')
  const evaluate = async expression => {
    const r = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
    if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails))
    return r.result.value
  }
  const wait = async expression => {
    for (let i = 0; i < 150; i++) {
      if (await evaluate(`Boolean(${expression})`)) return
      await new Promise(r => setTimeout(r, 100))
    }
    throw new Error(`Not ready: ${expression}\n${await evaluate('document.body.innerText')}`)
  }
  const click = async (expression, count = 1) => {
    const r = await evaluate(`(() => { const e=${expression}; if(!e) return null; e.scrollIntoView({block:'center',inline:'center'}); const b=e.getBoundingClientRect(); const x=b.x+b.width/2,y=b.y+b.height/2; return {x,y,visible:b.width>0&&b.height>0&&x>=0&&x<innerWidth&&y>=0&&y<innerHeight,hit:e.contains(document.elementFromPoint(x,y))};})()`)
    assert.ok(r?.visible && r.hit, JSON.stringify(r))
    await call('Input.dispatchMouseEvent', { type: 'mousePressed', x: r.x, y: r.y, button: 'left', clickCount: count })
    await call('Input.dispatchMouseEvent', { type: 'mouseReleased', x: r.x, y: r.y, button: 'left', clickCount: count })
  }
  for (const [width, height] of [[1024, 768], [1920, 1080]]) {
    errors.length = 0
    await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
    await call('Page.navigate', { url: `http://127.0.0.1:${server.httpServer.address().port}/probe/longwave-logbook.html` })
    await wait(`document.querySelector('.logbook-row:not(.head):not(.placeholder)')`)
    await click(`[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Sync Longwave')`)
    await wait(`document.documentElement.dataset.synced==='yes' && document.body.innerText.includes('W2NEW')`)
    await click(`document.querySelector('[title="View everything recorded for W1AW"] > .log-cell:nth-child(3)')`, 2)
    await wait(`document.querySelector('[role="dialog"]')`)
    await click(`[...document.querySelectorAll('[role="dialog"] button')].find(e=>e.textContent.trim()==='Upload to Longwave')`)
    await wait(`document.documentElement.dataset.pushed==='yes'`)
    await wait(`(()=>{const b=[...document.querySelectorAll('[role="dialog"] button')].find(e=>e.textContent.trim()==='Upload to Longwave');return b?.disabled})()`)
    assert.deepEqual(errors, [])
    const screenshot = await call('Page.captureScreenshot', { format: 'png' })
    await mkdir(join(ui, '..', 'target', 'longwave-logbook-probe'), { recursive: true })
    await writeFile(join(ui, '..', 'target', 'longwave-logbook-probe', `${width}x${height}.png`), Buffer.from(screenshot.data, 'base64'))
    console.log(`PASS Longwave Logbook ${width}x${height}: real sync click, history refresh, reachable detail upload, accepted result blocks repeat`)
  }
} finally {
  await browser?.stop()
  await server.close()
}

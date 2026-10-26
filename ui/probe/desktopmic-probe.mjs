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
  const evaluate = async expression => {
    const r = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
    if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails))
    return r.result.value
  }
  for (const [width, height] of [[1024, 768], [1920, 1080]]) {
    await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
    await call('Page.navigate', { url: `http://127.0.0.1:${server.httpServer.address().port}/probe/desktopmic.html` })
    let ready
    for (let i = 0; i < 200; i++) {
      ready = await evaluate('!!document.querySelector("[data-setting=phoneDesktopMic]")')
      if (ready) break
      await new Promise(resolve => setTimeout(resolve, 100))
    }
    assert.ok(ready, 'Phone settings mounted')
    assert.equal(await evaluate('document.querySelector("[data-setting=phoneDesktopMic]").value'), 'rig')
    await evaluate(`(() => { const s=document.querySelector('[data-setting=phoneDesktopMic]'); s.value='desktop'; s.dispatchEvent(new Event('change',{bubbles:true})); })()`)
    await evaluate('new Promise(resolve => requestAnimationFrame(resolve))')
    const boxes = await evaluate(`(() => {
      const group=document.querySelector('[data-setting=phoneDesktopMic]').closest('.settings-featgroup');
      return [...group.querySelectorAll('select')].map(e=>{
        e.scrollIntoView({block:'center'}); const r=e.getBoundingClientRect();
        return {w:r.width,h:r.height,visible:r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight,
          hit:e.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2))};
      });
    })()`)
    assert.equal(boxes.length, 2)
    assert.ok(boxes.every(b=>b.w>=160&&b.h>0&&b.visible&&b.hit), JSON.stringify(boxes))
    await evaluate(`document.querySelector('.settings-save').click()`)
    let saved
    for (let i = 0; i < 50; i++) {
      saved = await evaluate('document.documentElement.dataset.micSaved')
      if (saved) break
      await new Promise(resolve => setTimeout(resolve, 100))
    }
    assert.deepEqual(JSON.parse(saved ?? '{}'), { desktop: true, device: 'Desktop microphone' })
    assert.equal(await evaluate('document.querySelectorAll("[role=dialog]").length'), 0)
    await evaluate(`document.querySelector('[data-setting=phoneDesktopMic]').scrollIntoView({block:'center'})`)
    const shot = await call('Page.captureScreenshot', { format: 'png' })
    const out = join(ui, '..', 'target', 'desktopmic-probe')
    await mkdir(out, { recursive: true })
    await writeFile(join(out, `${width}x${height}.png`), Buffer.from(shot.data, 'base64'))
    console.log(`PASS desktop microphone ${width}x${height}: visible selectors, hit testing, normal Save, no extra dialogs`)
  }
} finally {
  await browser?.stop().catch(e => { if (e.code !== 'ESRCH') throw e })
  await server.close()
}

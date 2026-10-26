// Real browser geometry and interaction check for the existing logging settings.
import assert from 'node:assert/strict'
import { createServer } from 'vite'
import { mkdir, writeFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { chrome } from '../../remote/test/browser-runtime.mjs'

const ui = join(dirname(fileURLToPath(import.meta.url)), '..')
const server = await createServer({ root: ui, configFile: join(ui, 'vite.config.ts'),
  logLevel: 'warn', server: { host: '127.0.0.1', port: 0 } })
await server.listen()
let browser
try {
  browser = await chrome()
  const { targetId } = await browser.call('Target.createTarget', { url: 'about:blank' })
  const { sessionId } = await browser.call('Target.attachToTarget', { targetId, flatten: true })
  const call = (method, params) => browser.call(method, params, sessionId)
  const evaluate = async expression => {
    const result = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.text)
    return result.result.value
  }
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  for (const [width, height] of [[1024, 768], [1920, 1080]]) {
    await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
    await call('Page.navigate', { url: `${origin}/probe/longwave.html` })
    let ready = false
    for (let i = 0; i < 200; i++) {
      ready = await evaluate('!!document.querySelector("[data-setting=longwaveUrl]")')
      if (ready) break
      await new Promise(resolve => setTimeout(resolve, 100))
    }
    assert.ok(ready, 'logging settings must mount')
    await evaluate(`document.querySelector('[data-setting=longwaveUrl]').scrollIntoView({block:'center'})`)
    const boxes = await evaluate(`(() => {
      const field = document.querySelector('[data-setting=longwaveUrl]').closest('.settings-featgroup');
      return [...field.querySelectorAll('input,select,button')].map(e => {
        const r=e.getBoundingClientRect();
        return {name:e.getAttribute('aria-label')||e.textContent||e.type, x:r.x, y:r.y, w:r.width, h:r.height,
          reachable:r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth,
          hit:e.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2))};
      });
    })()`)
    assert.ok(boxes.every(box => box.reachable && box.hit), JSON.stringify(boxes))
    assert.ok(boxes.filter(box => box.name === 'password').every(box => box.w >= 160), 'token input must remain usable')
    await evaluate(`document.querySelector('[data-setting=longwaveLogbookId]').parentElement.querySelector('button').click()`)
    let selected = ''
    for (let i = 0; i < 50; i++) {
      selected = await evaluate(`document.querySelector('[data-setting=longwaveLogbookId]').value`)
      if (selected === 'book-a') break
      await new Promise(resolve => setTimeout(resolve, 100))
    }
    assert.equal(selected, 'book-a', 'connection test populates the existing form')
    await evaluate(`document.querySelector('[data-setting=longwaveUpload]').click()`)
    await evaluate('new Promise(resolve => requestAnimationFrame(resolve))')
    assert.equal(await evaluate(`document.querySelector('[data-setting=longwaveUpload]').getAttribute('aria-checked')`), 'true')
    await evaluate(`[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='Save').click()`)
    let saved
    for (let i = 0; i < 50; i++) {
      saved = await evaluate('document.documentElement.dataset.longwaveSaved')
      if (saved) break
      await new Promise(resolve => setTimeout(resolve, 100))
    }
    assert.deepEqual(JSON.parse(saved ?? '{}'), { book: 'book-a', upload: true }, 'normal Save must persist Longwave settings')
    assert.equal(await evaluate('document.querySelectorAll("[role=dialog]").length'), 0)
    const screenshot = await call('Page.captureScreenshot', { format: 'png' })
    const out = join(ui, '..', 'target', 'longwave-probe')
    await mkdir(out, { recursive: true })
    await writeFile(join(out, `settings-${width}x${height}.png`), Buffer.from(screenshot.data, 'base64'))
    console.log(`PASS Longwave settings ${width}x${height}: usable controls, logbook selected, normal Save persists forwarding, no dialog`)
  }
} catch (error) {
  console.error(error)
  throw error
} finally {
  await browser?.stop().catch(error => {
    // Windows may have already reaped Chrome before the runtime's POSIX cleanup.
    if (error.code !== 'ESRCH') throw error
  })
  await server.close()
}

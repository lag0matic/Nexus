import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { readFile, mkdir, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { chrome } from '../../remote/test/browser-runtime.mjs'

const ui = join(dirname(fileURLToPath(import.meta.url)), '..')
const dist = resolve(ui, 'dist')
const fixture = JSON.parse(await readFile(join(ui, 'src/remote-web/__fixtures__/navigation-connect.json'), 'utf8'))
const access = 'a'.repeat(64)
const posts = []
const monitorPanels = ['spots', 'bandmapPhone', 'bandmapCw', 'memories', 'dxped', 'sats', 'activation', 'operate', 'operatemap', 'waterfall', 'sstvviewer', 'fieldday', 'contestlog', 'contestscreen', 'fdclub']
function monitor(panel) {
  const data = { panel, metrics: [['Status', 'Receiving']], groups: [{ title: 'Recent activity', columns: ['Call', 'MHz', 'Mode'], rows: [['W9XYZ', (14.08 + monitorPanels.indexOf(panel) * .001).toFixed(4), 'FT4']], total: 1 }], media: {} }
  if (panel === 'waterfall') { data.groups = []; data.media = { spectrum: Array.from({ length: 512 }, (_, i) => (Math.sin(i / 7) + 1) / 2), loHz: 0, hiHz: 4000 } }
  if (panel === 'sstvviewer') data.media = { rgb: Buffer.alloc(16 * 16 * 3, 120).toString('base64'), width: 16, height: 16 }
  if (panel === 'operatemap') data.media = { myGrid: 'EN37', stations: [{call: 'W9XYZ', grid: 'EM69', snr: -7, lastHeardSlot: 1, heardCount: 1, presence: 'active', worked: false}] }
  return data
}
let keyed = false
let mhz = 14.074
let tier = 'FT8'
const mime = { '.js': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.geojson': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.woff2': 'font/woff2' }
const server = createServer(async (req, res) => {
  const url = new URL(req.url, 'http://localhost')
  try {
    if (url.pathname.startsWith('/observer/')) {
      if (req.method === 'POST' && url.pathname === '/observer/pair') {
        const chunks = []; for await (const chunk of req) chunks.push(chunk)
        const body = JSON.parse(Buffer.concat(chunks).toString())
        assert.deepEqual(Object.keys(body), ['code'])
        if (body.code !== '12345678') { res.writeHead(403).end('Incorrect access code'); return }
        res.setHeader('Content-Type', 'application/json'); res.end(JSON.stringify({ access })); return
      }
      if (req.headers.authorization !== `Bearer ${access}`) { res.writeHead(403).end('No access'); return }
      const json = body => { res.setHeader('Content-Type', 'application/json'); res.end(JSON.stringify(body)) }
      if (req.method === 'POST') {
        assert.equal(url.pathname, '/observer/listen')
        const chunks = []; for await (const chunk of req) chunks.push(chunk)
        const body = JSON.parse(Buffer.concat(chunks).toString())
        assert.deepEqual(Object.keys(body).sort(), ['mhz', 'mode', 'observedAt', 'radio'])
        posts.push(body); mhz = body.mhz; tier = body.mode
        json({ accepted: true }); return
      }
      const command = url.pathname.split('/').at(-1)
      if (command === 'observer_panel_data') {
        const args = JSON.parse(url.searchParams.get('args'))
        assert.deepEqual(Object.keys(args), ['panel'])
        assert.ok(monitorPanels.includes(args.panel))
        json(monitor(args.panel)); return
      }
      const replies = {
        state: { radio: 2, mhz, band: '20m', mode: 'digital', tier, catOk: true, busy: keyed, observedAt: Date.now() },
        get_ota_spots: [{ activator: 'W9XYZ', reference: 'US-1234', name: 'A long park name for testing wrapping on narrow screens', freqKhz: 14074, mode: 'FT8' }],
        get_need_alerts: [{ call: 'W9XYZ', band: '20m', exactMode: 'FT4', mode: 'Digital', freqMhz: 14.08, entity: 'Test entity', headline: 'New band and mode' }],
        get_propagation: fixture.prop, tv_station: { call: 'W9XYZ', grid: 'EN37' },
        get_kc2g_muf: fixture.muf, get_space_wx_scales: fixture.scales,
        get_xray_now: fixture.xray, get_aurora: fixture.aurora, get_pca: fixture.pca,
        get_band_outlook: fixture.bandOutlook, get_getting_out: fixture.gettingOut,
        get_declination: fixture.declination, get_satellites: [], get_ota_map_spots: [],
      }
      if (command in replies) { json(replies[command]); return }
      res.writeHead(403).end('Not allowed'); return
    }
    const name = url.pathname === '/' || url.pathname.startsWith('/panels/') ? 'observer.html' : url.pathname.replace(/^\//, '')
    const file = resolve(dist, name)
    if (!file.startsWith(`${dist}/`)) { res.writeHead(404).end(); return }
    const extension = file.slice(file.lastIndexOf('.'))
    let body = await readFile(file)
    if (name === 'observer.html') body = Buffer.from(body.toString().replaceAll('./assets/', '/assets/'))
    res.setHeader('Content-Type', mime[extension] ?? 'application/octet-stream'); res.end(body)
  } catch (e) { res.writeHead(500).end(String(e)) }
})
await new Promise(r => server.listen(0, '127.0.0.1', r))
let browser
try {
  browser = await chrome()
  const { targetId } = await browser.call('Target.createTarget', { url: 'about:blank' })
  const { sessionId } = await browser.call('Target.attachToTarget', { targetId, flatten: true })
  const call = (method, params = {}) => browser.call(method, params, sessionId)
  const faults = []
  browser.on('Runtime.exceptionThrown', ({ exceptionDetails }) => faults.push(exceptionDetails.exception?.description ?? exceptionDetails.text))
  await call('Runtime.enable')
  const evaluate = async expression => {
    const value = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
    if (value.exceptionDetails) throw new Error(JSON.stringify(value.exceptionDetails))
    return value.result.value
  }
  const wait = async expression => {
    for (let n = 0; n < 150; n++) { if (await evaluate(expression)) return; await new Promise(r => setTimeout(r, 100)) }
    throw new Error(`Timed out: ${expression}\n${await evaluate('document.body.innerText')}`)
  }
  const click = async selector => {
    const rect = await evaluate(`(() => {const e=document.querySelector(${JSON.stringify(selector)}); e.scrollIntoView({block:'center'}); const r=e.getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2,hit:e.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2))};})()`)
    assert.ok(rect.hit, `real hit test: ${selector}`)
    await call('Input.dispatchMouseEvent', { type:'mousePressed', x:rect.x,y:rect.y,button:'left',clickCount:1 })
    await call('Input.dispatchMouseEvent', { type:'mouseReleased', x:rect.x,y:rect.y,button:'left',clickCount:1 })
  }
  const output = join(ui, '..', 'target', 'observer-probe')
  await mkdir(output, { recursive: true })
  for (const [width,height] of [[375,812], [768,1024], [1024,768], [1920,1080]]) {
    await call('Emulation.setDeviceMetricsOverride', { width,height,deviceScaleFactor:1,mobile:width<600 })
    for (const panel of ['pota', 'needed', 'conditions', ...monitorPanels]) {
      await call('Page.navigate', { url:`http://127.0.0.1:${server.address().port}${panel === 'pota' ? '/' : `/panels/${panel}`}` })
      await wait(`!!document.querySelector('#observer-code')`)
      const entry = await evaluate(`(() => {const e=document.querySelector('#observer-code');const r=e.getBoundingClientRect();return {right:r.right,width:r.width,scroll:document.documentElement.scrollWidth}})()`)
      assert.ok(entry.right <= width + 1 && entry.width > 200 && entry.scroll <= width + 1, JSON.stringify(entry))
      await click('#observer-code')
      await call('Input.insertText', { text: '87654321' })
      await click('button[type=submit]')
      await wait(`document.querySelector('[role=alert]')?.textContent.includes('Incorrect access code')`)
      assert.equal(await evaluate(`!!document.querySelector('.observer-spot')`), false)
      await evaluate(`document.querySelector('#observer-code').select()`)
      await call('Input.insertText', { text: '1234 5678' })
      await click('button[type=submit]')
      if (panel === 'pota' || panel === 'needed') {
        await wait(`!!document.querySelector('.observer-spot:not(:disabled)')`)
        const layout = await evaluate(`(() => {const e=document.querySelector('.observer-spot');const r=e.getBoundingClientRect(); return {right:r.right,width:r.width,height:r.height,scroll:document.documentElement.scrollWidth,viewport:innerWidth};})()`)
        assert.ok(layout.right <= width + 1 && layout.width > 250 && layout.height >= 44 && layout.scroll <= width + 1, JSON.stringify(layout))
        const before = posts.length
        if (panel === 'pota') {
          await click('.observer-filters button[aria-pressed=false]')
          assert.equal(posts.length, before, 'band filters must never tune')
          await click('.observer-filters button:first-child')
          await wait(`!!document.querySelector('.observer-spot:not(:disabled)')`)
        }
        await click('.observer-spot')
        await wait(`document.querySelector('.observer-spot')?.getAttribute('aria-pressed')==='true'`)
        assert.equal(posts.length, before + 1)
        assert.equal(posts.at(-1).mode, panel === 'pota' ? 'FT8' : 'FT4')
        keyed = true
        await wait(`document.querySelector('.observer-spot')?.disabled===true`)
        await click('.observer-spot')
        assert.equal(posts.length, before + 1, 'keyed radio must not send another click')
        keyed = false
      } else if (panel === 'conditions') {
        await wait(`!!document.querySelector('.connect-shell')`)
        await new Promise(r => setTimeout(r, 1000))
        assert.equal(await evaluate(`document.body.innerText.includes('Panel unavailable.')`), false)
      } else {
        await wait(`!!document.querySelector('.observer-monitor h1')`)
        assert.equal(await evaluate(`document.querySelector('select[aria-label=Panel]').value`), panel)
        assert.ok(await evaluate(`document.documentElement.scrollWidth <= innerWidth + 1`), `${panel}: page must fit the viewport`)
        if (['waterfall', 'sstvviewer'].includes(panel)) {
          await wait(`(() => { const c = document.querySelector('canvas'); return c && [...c.getContext('2d').getImageData(0,0,c.width,c.height).data].some((v,i)=>i%4!==3 && v>0) })()`)
        }
        if (panel === 'operatemap') await wait(`!!document.querySelector('.observer-map canvas')`)
        if (['spots', 'memories', 'bandmapPhone', 'bandmapCw'].includes(panel)) {
          const before = posts.length
          await wait(`!!document.querySelector('.observer-monitor td button:not(:disabled)')`)
          await click('.observer-monitor td button')
          const expected = monitor(panel).groups[0].rows[0][1]
          await wait(`document.querySelector('.observer-radio').textContent.includes(${JSON.stringify(expected)})`)
          assert.equal(posts.length, before + 1)
          assert.equal(posts.at(-1).mode, 'FT4', 'monitor clicks retain exact reported modes')
        }
        assert.equal(await evaluate(`!!document.querySelector('[data-action=ptt], [data-action=tx], [data-action=arm]')`), false)
      }
      assert.equal(await evaluate(`location.hash`), '', 'access removed from visible address')
      assert.equal(await evaluate(`Array.from(document.querySelectorAll('.observer-header a')).some(a=>a.href.includes('access='))`), false, 'panel addresses never include credentials')
      assert.equal(await evaluate(`Object.values(localStorage).concat(Object.values(sessionStorage)).some(v=>v.includes(${JSON.stringify(access)}))`), false, 'credentials never saved')
      const shot = await call('Page.captureScreenshot', { format:'png' })
      await writeFile(join(output, `${panel}-${width}x${height}.png`), Buffer.from(shot.data,'base64'))
      console.log(`PASS ${panel} ${width}x${height}: built assets, real layout and browser input`)
    }
  }
  await call('Page.reload')
  await wait(`!!document.querySelector('#observer-code')`)
  console.log('PASS: root address, typed short code, wrong-code refusal, tab-memory access and reload re-entry')
  assert.deepEqual(faults, [], 'no browser runtime exceptions')
  console.log('PASS: only receive-only POST bodies; no transmit or arming commands')
} finally {
  await browser?.stop()
  await new Promise(r => server.close(r))
}

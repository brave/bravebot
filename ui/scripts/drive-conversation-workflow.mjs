// Real Electron renderer acceptance with isolated persistence and deterministic bridge events.
// No provider requests or changes to the user's projects. Screenshots accompany assertions.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
const output = process.env.UX_OUTPUT || join(tmpdir(), 'bravebot-conversation-regression')
mkdirSync(output, { recursive: true })
const profile = mkdtempSync(join(tmpdir(), 'bravebot-ux-profile-'))
const directory = mkdtempSync(join(tmpdir(), 'bravebot-ux-project-'))
mkdirSync(directory+'-copy')
const idA = '11111111-1111-4111-8111-111111111111', idB = '22222222-2222-4222-8222-222222222222'
writeFileSync(join(profile, 'bravebot-ui.json'), JSON.stringify({ bots: [{slug:'review-bot', name:'Review Bot', purpose:'Review a disposable project and remember preferences.', avatar:'review-bot', model:null, directory, session:idA, conversations:[idA,idB], archived:0, remembered:0, quiet:0, retired:0, created:1}], recents:[directory] }))
mkdirSync(join(directory,'.bravebot-ui/bots'), {recursive:true})
writeFileSync(join(directory,'.bravebot-ui/bots/review-bot.md'), '# Preferences\n\n- Keep reviews concise.\n')
const app = await electron.launch({args:['.', '--disable-renderer-backgrounding', '--disable-background-timer-throttling',`--user-data-dir=${profile}`],cwd:process.cwd(),timeout:40000})
let page
const originalClipboard = await app.evaluate(({clipboard})=>clipboard.readText())
try {
  page = await app.firstWindow(); await page.setViewportSize({ width: 1350, height: 900 }); page.setDefaultTimeout(7000)
  const errors=[]; page.on('pageerror',e=>{errors.push(e.message);console.error('RENDERER',e.message)})
  await app.evaluate(({ipcMain,BrowserWindow}, {directory,idA,idB})=>{
    const rows=[{id:idA,title:'Review the sample project'},{id:idB,title:'Plan the next iteration'}].map(r=>({...r,directory,project:'sample-project',branch:'main',updated:Date.now(),bytes:20}))
    const emit=(event,data,session='s-'+idA)=>BrowserWindow.getAllWindows()[0].webContents.send('bravebot:event',{event,data,session})
    globalThis.ux={emit,sent:[],configured:true,choose:directory,grants:{paths:[{path:'',integrity:'trusted'},{path:'private',integrity:'untrusted'}],commands:[{program:'/usr/bin/git',args:['status'],display:'git status'}]}}
    const replace=(name,fn)=>{ipcMain.removeHandler(name);ipcMain.handle(name,fn)}
    replace('bravebot:choose-directory',()=>globalThis.ux.choose)
    replace('bravebot:request',async(_,method,p={})=>{
      const ux=globalThis.ux
      if(method==='agent.info') return {ok:{configured:ux.configured,build:'acceptance fixture',version:'1'}}
      if(method==='doctor') return {ok:{found:true,text:'Backend not configured. Install a configured build or rebuild with the approved environment.'}}
      if(method==='session.list') return {ok:{sessions:rows}}
      if(method==='models.list') return {ok:{defaultModel:'sample/fast',warnings:[],models:[{id:'sample/fast',name:'Fast model',provider:'Sample',premium:false,contextWindow:200000,capabilities:['text','tools','vision']},{id:'sample/deep',name:'Deep model',provider:'Sample',premium:false,contextWindow:100000,capabilities:['text','tools']}]}}
      if(method==='session.new') return {ok:{session:'s-new',directory,branch:'main',model:'sample/fast'}}
      if(method==='session.open') return {ok:{session:'s-'+p.id,model:'sample/fast',record:{...rows.find(r=>r.id===p.id),started:1,turns:1,tokens:100,build:'fixture'},said:p.id===idA?[{kind:'user',text:'Review the project and show a code example.'},{kind:'assistant',text:'The project is ready for review.\n\n```js\nconst message = "A long example line for testing wrapping and copying without losing content.";\nconsole.log(message);\n```\n\nSee [sample.txt](src/nested/sample.txt).'}]:[],todos:{},context:'',trust:{known:true,rules:[]},archived:0,branchNote:null,buildNote:null}}
      if(method==='turn.send'){ux.sent.push(p);emit('turn.started',{turn:ux.sent.length},p.session);return {ok:{turn:ux.sent.length}}}
      if(method==='turn.cancel'){emit('turn.error',{kind:'cancelled',message:'Cancelled by user',turn:1,id:p.session==='s-new'?'33333333-3333-4333-8333-333333333333':p.session.slice(2)},p.session);return {ok:{}}}
      if(method==='permissions.list') return {ok:ux.grants}
      if(method==='permissions.revoke'){if(p.kind==='path')ux.grants.paths=ux.grants.paths.filter(g=>g.path!==p.path);else ux.grants.commands=[];return {ok:ux.grants}}
      return {ok:{}}
    })
    replace('bravebot:bots:send',(_,p)=>{globalThis.ux.sent.push(p);emit('turn.started',{turn:globalThis.ux.sent.length},p.session);return {ok:{turn:globalThis.ux.sent.length}}})
    replace('bravebot:files:list',()=>({path:'',rows:[],truncated:false}))
    replace('bravebot:files:search',()=>({paths:['src/nested/sample.txt'],incomplete:false}))
    replace('bravebot:files:preview',(_,session,path)=>({path,text:'Alpha\nDelta\nGamma\n',truncated:false}))
    replace('bravebot:files:open',()=>{globalThis.ux.opened=true;return {status:'opened'}})
    replace('bravebot:files:choose-attachments',()=>[{id:'fixture-attachment',path:'src/nested/sample.txt',bytes:18}])
  },{directory,idA,idB})
  const snap=async name=>{if(/^2[1245]-memory/.test(name)) await page.locator('.memory-actions').scrollIntoViewIfNeeded();await page.waitForTimeout(400);await page.screenshot({path:join(output,name+'.png'),scale:'css'});console.log('VISUAL',name)}
  const emit=async(event,data,session='s-'+idA)=>app.evaluate((_,v)=>globalThis.ux.emit(v.event,v.data,v.session),{event,data,session})
  const done=async(session='s-'+idA,reply='Review complete.')=>emit('turn.done',{id:session.slice(2),turn:1,reply,model:'sample/fast',steps:1,clean:true,tokens:10,outputTokens:10,notices:[],trust:{rules:[]},archived:0},session)
  await page.reload()
  await page.getByRole('button', {name: 'Open project', exact: true}).waitFor()
  const first = () => page.locator('.session').filter({hasText: 'Review the sample project'})
  const second = () => page.locator('.session').filter({hasText: 'Plan the next iteration'})
  const composer = page.getByRole('textbox', {name: 'Message the agent'})
  await first().click()
  await composer.fill('Draft one')
  await second().click()
  await composer.fill('Draft two')
  await first().click()
  assert.equal(await composer.inputValue(), 'Draft one')
  await page.getByRole('button', {name: 'Send', exact: true}).click()
  await emit('ask.request', {request: 1, prompts: [{header: 'Color', question: 'Which color?', rows: [{index: 0, label: 'Blue', detail: 'Cool'}], multiple: false, key: 'color'}]})
  await composer.fill('Queued follow-up')
  await page.getByRole('button', {name: 'Queue message', exact: true}).click()
  await second().click()
  await page.getByRole('button', {name: /Answer needed · Review/}).waitFor()
  assert.equal(await composer.inputValue(), 'Draft two')
  await first().click()
  await page.getByRole('button', {name: 'Stop', exact: true}).click()
  await page.getByText('Task stopped', {exact: true}).waitFor()
  assert.equal((await app.evaluate(() => globalThis.ux.sent)).length, 1)
  await page.getByRole('button', {name: 'Resume queue', exact: true}).click()
  await page.waitForFunction(() => document.querySelector('.stop'))
  assert.equal((await app.evaluate(() => globalThis.ux.sent)).length, 2)
  await done()
  await composer.fill('Keep my recovery draft')
  await emit('turn.error', {kind: 'chat', message: '429 rate limit exceeded', turn: 2, id: idA})
  await page.getByRole('button', {name: 'Draft continuation'}).last().click()
  assert.match(await composer.inputValue(), /^Keep my recovery draft/)
  await page.getByRole('button', {name: 'Permissions', exact: true}).click()
  await page.getByRole('button', {name: 'Revoke', exact: true}).first().click()
  await page.getByRole('button', {name: 'Done', exact: true}).click()
  assert.equal((await app.evaluate(() => globalThis.ux.grants.paths)).length, 1)
  await page.getByRole('button', {name: 'Actions for Review the sample project'}).click()
  await page.getByRole('menuitem', {name: 'Pin conversation'}).click()
  await page.getByLabel('Pinned', {exact: true}).waitFor()
  for (let n = 0; n < 30; n++) await emit('narration', {text: `Observation ${n}: retain reading position.`})
  await page.locator('.entries').evaluate(e => e.scrollTop = 150)
  await page.waitForTimeout(250)
  const reading = await page.locator('.entries').evaluate(e => e.scrollTop)
  await emit('narration', {text: 'Arrived while reading'})
  await page.getByRole('button', {name: 'New activity ↓', exact: true}).waitFor()
  assert.equal(await page.locator('.entries').evaluate(e => e.scrollTop), reading)
  await second().click()
  await first().click()
  assert.equal(await page.locator('.entries').evaluate(e => e.scrollTop), reading)
  await second().click()
  await page.reload()
  await second().click()
  assert.equal(await composer.inputValue(), 'Draft two')
  await snap('conversation-workflow')
  assert.deepEqual(errors, [])
  console.log('PASS: independent persistent drafts, background approvals, Stop/Resume queue, recovery, permissions, pins and reading position')
} catch(error) {if(page){await page.screenshot({path:join(output,'failure.png')});console.error((await page.locator('body').innerText()).slice(-7000))}throw error}
finally {await app.evaluate(({clipboard},text)=>clipboard.writeText(text),originalClipboard);await app.close()}

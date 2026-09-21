// Real Electron renderer acceptance with isolated persistence and deterministic bridge events.
// No provider requests or changes to the user's projects. Screenshots accompany assertions.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
const output = process.env.UX_OUTPUT || join(tmpdir(), 'bravebot-ux-final')
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
      if(method==='settings.inspect') return {ok:{build:'acceptance fixture',configured:ux.configured,problem:ux.configured?null:'No model service configured.',model:null,brave:false,bedrock:false,providers:[],selected:null,layers:[],overrides:[],managed:{path:null,keys:[]},network:{roots:[],problem:null,trustsNothing:false,proxy:null,authenticated:false,unusableProxy:null,noProxy:null}}}
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
  await page.reload();await page.getByRole('button',{name:'Open project',exact:true}).waitFor();await snap('01-welcome')
  // Playwright clicks bypass native window dragging; assert the native hit-test exclusion too.
  for (const name of ['Conversations', 'Archived']) assert.equal(await page.getByRole('button', {name, exact:true}).evaluate(e => getComputedStyle(e).getPropertyValue('-webkit-app-region')), 'no-drag', `${name} must receive native mouse clicks inside the draggable header`)
  await page.locator('.session').filter({hasText:'Review the sample project'}).click()
  const composer=page.getByRole('textbox',{name:'Message the agent'})
  await composer.fill('Draft retained across conversations.');await snap('02-conversation')
  await page.locator('.session').filter({hasText:'Plan the next iteration'}).click();await composer.fill('Independent draft.')
  await page.locator('.session').filter({hasText:'Review the sample project'}).click();assert.equal(await composer.inputValue(),'Draft retained across conversations.')
  await page.getByRole('button',{name:'Focus',exact:true}).click();await snap('03-focus');await page.getByRole('button',{name:'Exit focus'}).click()
  await page.getByRole('button',{name:'Compact view',exact:true}).click();await snap('04-compact');await page.getByRole('button',{name:'Comfortable view',exact:true}).click()
  await page.getByRole('button',{name:'Permissions',exact:true}).click();await page.getByRole('button',{name:'Revoke',exact:true}).first().waitFor();await snap('05-permissions')
  await page.getByRole('button',{name:'Revoke',exact:true}).first().click();await page.getByText('No trusted path grants.',{exact:true}).waitFor()
  await page.getByRole('button',{name:'Revoke',exact:true}).click();await page.getByText('No remembered command grants.',{exact:true}).waitFor();await snap('06-revoked')
  await page.getByRole('button',{name:'Done',exact:true}).focus();await page.keyboard.press('Tab');assert.equal(await page.getByRole('button',{name:'Refresh',exact:true}).evaluate(e=>e===document.activeElement),true)
  await page.keyboard.press('Escape');assert.equal(await page.getByRole('button',{name:'Permissions',exact:true}).evaluate(e=>e===document.activeElement),true)
  await page.getByRole('button',{name:'Find',exact:true}).click();await page.getByRole('searchbox',{name:'Find in conversation'}).fill('example');await snap('07-search');await page.getByRole('button',{name:'Close search'}).click()
  await page.getByRole('button',{name:'Copy code',exact:true}).click();await page.getByRole('button',{name:'Copied',exact:true}).waitFor();assert.match(await app.evaluate(({clipboard})=>clipboard.readText()),/console.log\(message\)/);await page.getByRole('button',{name:'Wrap',exact:true}).click();await snap('08-code-wrap')
  await page.getByRole('button',{name:'sample.txt',exact:true}).click();await page.getByRole('dialog').waitFor();await snap('09-preview');await page.getByRole('button',{name:'Open in default app'}).click();assert.equal(await app.evaluate(()=>globalThis.ux.opened),true);await page.getByRole('button',{name:'Done',exact:true}).click()
  await page.getByRole('button',{name:'Attach files'}).click();await page.locator('.attachment-chips').waitFor();await snap('10-attachment');await page.getByRole('button',{name:'Remove attachment src/nested/sample.txt'}).click()
  await page.locator('.model-trigger').click();await page.getByRole('option',{name:/Deep model/}).waitFor();await snap('11-models');await page.getByRole('option',{name:/Deep model/}).click();await page.locator('.model-trigger').click();await snap('12-recent-model');await page.keyboard.press('Escape')
  await composer.fill('Start review');await page.getByRole('button',{name:'Send',exact:true}).click()
  await emit('ask.request',{request:1,prompts:[{header:'Color',question:'Which color should the sample use?',rows:[{index:0,label:'Blue',detail:'A cool accent'},{index:1,label:'Green',detail:'A natural accent'}],multiple:false,key:'color'}]})
  await composer.fill('Follow up after this review');await page.getByRole('button',{name:'Queue message',exact:true}).click();await snap('13-question-queue')
  await page.locator('.session').filter({hasText:'Plan the next iteration'}).click();await snap('14-background')
  await page.locator('.session').filter({hasText:'Review the sample project'}).click();await page.getByRole('button',{name:'Stop',exact:true}).click();await page.getByText('Task stopped',{exact:true}).waitFor();await snap('15-stopped-queue')
  assert.equal((await app.evaluate(()=>globalThis.ux.sent)).length,1)
  await page.getByRole('button',{name:'Resume queue',exact:true}).click();await page.waitForFunction(()=>document.querySelector('.stop'))
  assert.equal((await app.evaluate(()=>globalThis.ux.sent)).length,2);await done();await snap('16-resumed')
  await composer.fill('Keep this draft');await emit('turn.error',{kind:'chat',message:'429 rate limit exceeded; retry later',turn:2,id:idA});await page.getByText('The provider is busy',{exact:true}).waitFor();await page.getByRole('button',{name:'Draft continuation'}).last().click();assert.match(await composer.inputValue(),/^Keep this draft/);await snap('17-recovery')
  await page.getByRole('button',{name:'Actions for Review the sample project'}).click();await page.getByRole('menuitem',{name:'Pin conversation'}).click();await page.getByLabel('Pinned',{exact:true}).waitFor()
  await page.getByRole('button',{name:'Actions for Review the sample project'}).click();await page.getByRole('menuitem',{name:'Archive conversation'}).click();await page.getByRole('button',{name:'Archived',exact:true}).click();await snap('18-archived')
  await page.getByRole('button',{name:'Actions for Review the sample project'}).click();await page.getByRole('menuitem',{name:'Restore conversation'}).click();await page.getByText('No archived conversations.',{exact:true}).waitFor();await page.getByRole('button',{name:'Conversations',exact:true}).click()
  await composer.fill('Review one change');await page.getByRole('button',{name:'Send',exact:true}).click()
  const changes=[{kind:'kept',text:'Alpha'},{kind:'removed',text:'Beta'},{kind:'added',text:'Delta'},{kind:'kept',text:'Gamma'}]
  const activity={verb:'Update',target:'ref:1(U,pub):src/nested/sample.txt',note:null,failed:false,untrusted:true,changes:[]}
  await emit('tool.started',activity)
  await emit('confirm.request',{request:2,path:'src/nested/sample.txt',intent:'update',untrusted:true,existing:true,added:1,removed:1,exact:true,changes})
  await page.getByRole('button',{name:'Expand diff'}).click();await snap('28-expanded-diff');await page.getByRole('button',{name:'Done',exact:true}).click()
  await page.getByRole('tab',{name:'Overview',exact:true}).click();await snap('29-waiting-write')
  await page.getByRole('button',{name:'Apply this change',exact:true}).click();await page.locator('.files .applying').waitFor();await snap('30-approved-write')
  await emit('tool.finished',{...activity,note:'Updated sample.txt',changes});await page.locator('.files .applied').waitFor();assert.equal(await page.getByRole('tab',{name:'Overview',exact:true}).count(),1);await snap('31-applied-write');await done()
  const executionPlan='/usr/bin/printf hello > /tmp/result.txt && /usr/bin/true';
  await emit('run.request',{request:3,directory,stages:[{program:'printf',resolved:'/usr/bin/printf',args:['hello'],display:'printf hello > /tmp/result.txt'},{program:'true',resolved:'/usr/bin/true',args:[],display:'true'}],plan:executionPlan,stdin:'ref:3',writes:['/tmp/result.txt'],releasesPrivate:false,vouches:[],summary:'run two steps'});
  await page.getByText(executionPlan,{exact:true}).waitFor();
  await page.getByText('Files created or modified:',{exact:true}).waitFor();
  await page.locator('.confirm.run .permission-scope').filter({hasText:'Standard input:'}).getByText('ref:3',{exact:true}).waitFor();
  await page.locator('.confirm.run').scrollIntoViewIfNeeded();
  await snap('42-command-plan');
  await page.getByRole('button',{name:'Don’t run',exact:true}).click();
  await page.getByText('You refused this command',{exact:true}).waitFor();
  await page.getByRole('tab',{name:'Files',exact:true}).click();await page.getByRole('button',{name:'Search files',exact:true}).click();await page.getByRole('searchbox',{name:/Find files|Search/}).fill('sample');await page.getByRole('button',{name:'src/nested/sample.txt',exact:true}).waitFor();await snap('32-file-search')
  await page.getByRole('tab',{name:'Overview',exact:true}).click()
  for(let n=0;n<30;n++)await emit('narration',{text:`Review observation ${n}: verify layout and retain reading position.`})
  await page.locator('.entries').evaluate(e=>e.scrollTop=150);await page.waitForTimeout(250)
  const reading=await page.locator('.entries').evaluate(e=>e.scrollTop)
  await emit('narration',{text:'New work arrived while reading older content.'});await page.getByRole('button',{name:'New activity ↓',exact:true}).waitFor();assert.equal(await page.locator('.entries').evaluate(e=>e.scrollTop),reading);await snap('33-new-activity')
  await page.locator('.session').filter({hasText:'Plan the next iteration'}).click();await page.locator('.session').filter({hasText:'Review the sample project'}).click();assert.equal(await page.locator('.entries').evaluate(e=>e.scrollTop),reading)
  await emit('narration',{text:'Another observation arrived.'});await page.emulateMedia({reducedMotion:'reduce'});await page.getByRole('button',{name:'New activity ↓',exact:true}).click();await snap('34-follow-latest')
  await composer.fill('Finish a long bot turn');await page.getByRole('button',{name:'Send',exact:true}).click();await composer.fill('Wait for memory maintenance');await page.getByRole('button',{name:'Queue message',exact:true}).click()
  const beforeMaintenance=(await app.evaluate(()=>globalThis.ux.sent)).length
  await emit('turn.done',{id:idA,turn:4,reply:'Conversation compacted; updating memory.',model:'sample/deep',steps:1,clean:true,tokens:10,outputTokens:10,notices:[],trust:{rules:[]},archived:1,consolidating:true})
  await page.waitForTimeout(250);assert.equal((await app.evaluate(()=>globalThis.ux.sent)).length,beforeMaintenance)
  await app.evaluate(({BrowserWindow},session)=>BrowserWindow.getAllWindows()[0].webContents.send('bravebot:bots:consolidating',{session,slug:'review-bot'}),'s-'+idA);await snap('37-memory-maintenance-queue')
  await emit('turn.done',{id:idA,turn:5,reply:'Memory updated.',model:'sample/deep',steps:1,clean:true,tokens:10,outputTokens:10,notices:[],trust:{rules:[]},archived:1,consolidating:true});await page.waitForTimeout(250);assert.equal((await app.evaluate(()=>globalThis.ux.sent)).length,beforeMaintenance)
  await app.evaluate(({BrowserWindow},session)=>BrowserWindow.getAllWindows()[0].webContents.send('bravebot:bots:consolidated',{session,slug:'review-bot',delivered:true}),'s-'+idA)
  await page.waitForFunction(()=>document.querySelector('.queued-messages')===null);assert.equal((await app.evaluate(()=>globalThis.ux.sent)).length,beforeMaintenance+1);await done()
  await page.getByRole('button',{name:'Bots',exact:true}).click();await page.locator('.bot').filter({hasText:'Review Bot'}).click();await snap('19-bot-overview')
  await page.getByRole('button',{name:'Edit bot and memory'}).click();await page.getByRole('button',{name:'Edit memory',exact:true}).waitFor();await snap('20-bot-editor')
  await page.getByRole('button',{name:'Edit memory',exact:true}).click();await page.getByRole('textbox',{name:'Edit persistent memory'}).fill('# Saved preference\n\n- Use a blue accent.');await page.getByRole('button',{name:'Save memory'}).click();await page.locator('.memory-readable').getByText('Saved preference',{exact:true}).waitFor();await snap('21-memory-read')
  await page.getByRole('button',{name:'Raw',exact:true}).click();await snap('22-memory-raw');await page.getByRole('button',{name:'Reset memory…',exact:true}).click();await snap('23-memory-reset');await page.getByRole('button',{name:'Reset saved memory'}).click()
  await page.getByRole('button',{name:'History',exact:true}).click();await page.locator('.memory-history details').filter({hasText:'Saved preference'}).locator('summary').first().click();await snap('24-memory-history');await page.locator('.memory-history details').filter({hasText:'Saved preference'}).getByRole('button',{name:'Review for restore'}).first().click();await page.getByRole('button',{name:'Save memory'}).click();await page.getByRole('button',{name:'Read',exact:true}).click();await page.locator('.memory-readable').getByText('Saved preference',{exact:true}).waitFor();await snap('25-memory-restored');await page.keyboard.press('Escape')
  await app.evaluate(({BrowserWindow})=>BrowserWindow.getAllWindows()[0].setSize(950,780));await page.setViewportSize({width:950,height:780});await page.getByRole('button',{name:'Context panel',exact:true}).click();await snap('26-narrow-drawer');await page.getByRole('button',{name:'Close context panel'}).click()
  await page.emulateMedia({colorScheme:'dark',reducedMotion:'reduce'});await page.waitForFunction(()=>matchMedia('(prefers-color-scheme: dark)').matches);await snap('27-dark');await page.emulateMedia({colorScheme:'light',reducedMotion:'reduce'})
  await app.evaluate(({BrowserWindow,nativeTheme})=>{BrowserWindow.getAllWindows()[0].setSize(1280,820);nativeTheme.themeSource='light';globalThis.ux.configured=false});await page.setViewportSize({width:1280,height:820})
  await page.reload();await page.getByRole('button',{name:'Sessions',exact:true}).click();await page.locator('.session').filter({hasText:'Plan the next iteration'}).click();await page.getByText('Backend setup needed',{exact:true}).waitFor();assert.equal(await composer.inputValue(),'Independent draft.');await snap('35-backend-not-ready');await page.getByRole('button',{name:'Diagnostics',exact:true}).click();await snap('38-diagnostics');await page.keyboard.press('Escape')
  await page.getByRole('button',{name:'Setup help'}).click();await snap('36-setup-help');await page.keyboard.press('Escape')
  await app.evaluate(()=>globalThis.ux.configured=true);await page.getByRole('button',{name:'Check again'}).click();await page.getByText('Backend setup needed',{exact:true}).waitFor({state:'hidden'})
  await page.getByRole('button',{name:'Bots',exact:true}).click();await page.locator('.bot').filter({hasText:'Review Bot'}).click();await page.locator('.bot-conversations button').filter({hasText:'Review the sample project'}).click();await page.locator('.transcript-head').count()
  await page.locator('.bot').filter({hasText:'Review Bot'}).click();await page.getByRole('button',{name:'New conversation',exact:true}).click();await page.getByRole('button',{name:"Don't trust",exact:true}).click();await snap('39-new-bot-conversation')
  await composer.fill('First turn failure fixture');await page.getByRole('button',{name:'Send',exact:true}).click();await emit('turn.error',{kind:'chat',message:'401 credentials rejected',turn:1,id:'33333333-3333-4333-8333-333333333333'},'s-new');await page.getByText('The provider could not authenticate this request',{exact:true}).waitFor()
  await page.getByRole('button',{name:'Sessions',exact:true}).click();assert.equal(await page.locator('.session').filter({hasText:'First turn failure fixture'}).count(),1);await snap('40-first-turn-failure')
  await page.getByRole('button',{name:'Bots',exact:true}).click();await page.locator('.bot').filter({hasText:'Review Bot'}).click();await page.getByRole('button',{name:'Edit bot and memory'}).click();await app.evaluate(()=>globalThis.ux.choose+='-copy');await page.getByRole('button',{name:'Duplicate into another project'}).click();await page.getByRole('dialog').waitFor({state:'hidden'});await page.locator('.bot').filter({hasText:'Review Bot copy'}).click();await snap('41-duplicated-bot');assert.match(await page.getByRole('dialog').innerText(),/-copy/);await page.keyboard.press('Escape')
  const localHistory = await page.evaluate(() => window.bravebot.readMemoryHistory('review-bot'));
  assert.ok(localHistory.length > 0, 'fixture has local memory revisions before removal');
  await page.evaluate(() => window.bravebot.removeBot('review-bot'));
  const memoryFiles = ['memory-history.json', 'ground.md'].map(name => existsSync(join(profile, 'bots', 'review-bot', name)));
  assert.deepEqual(memoryFiles, [false, false]);
  const replacementBot = await page.evaluate(async directory => window.bravebot.writeBot({name:'Review Bot',purpose:'New identity',directory}), directory);
  assert.equal(replacementBot.slug, 'review-bot');
  assert.deepEqual(await page.evaluate(() => window.bravebot.readMemoryHistory('review-bot')), []);
  assert.deepEqual(errors,[])
  console.log('PASS UX acceptance', {output,profile,directory})
} catch(error) {if(page){await page.screenshot({path:join(output,'failure.png')});console.error((await page.locator('body').innerText()).slice(-7000))}throw error}
finally {await app.evaluate(({clipboard},text)=>clipboard.writeText(text),originalClipboard);await app.close()}

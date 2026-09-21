// Bot history acceptance: saved, archived, associated, draft and unavailable records.
// No provider requests or changes to the user's projects. Screenshots accompany assertions.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
const output = process.env.UX_OUTPUT || join(tmpdir(), 'bravebot-bot-history')
mkdirSync(output, { recursive: true })
const profile = mkdtempSync(join(tmpdir(), 'bravebot-ux-profile-'))
const directory = mkdtempSync(join(tmpdir(), 'bravebot-ux-project-'))
const idA = '11111111-1111-4111-8111-111111111111', idB = '22222222-2222-4222-8222-222222222222'
writeFileSync(join(profile, 'bravebot-ui.json'), JSON.stringify({ bots: [{slug:'review-bot', name:'Review Bot', purpose:'Review a disposable project and remember preferences.', avatar:'review-bot', model:null, directory, session:idA, conversations:[idA,'44444444-4444-4444-8444-444444444444'], archived:0, remembered:0, quiet:0, retired:0, created:1}], recents:[directory] }))
mkdirSync(join(directory,'.bravebot-ui/bots'), {recursive:true})
writeFileSync(join(directory,'.bravebot-ui/bots/review-bot.md'), '# Preferences\n\n- Keep reviews concise.\n')
writeFileSync(join(profile, 'experience.json'), JSON.stringify({conversations:{[JSON.stringify([directory,idA])]:{archived:true},[JSON.stringify([directory,idB])]:{botSlug:'review-bot'},[JSON.stringify([directory,'draft:older'])]:{botSlug:'review-bot',draft:'Earlier unsent review'}}}))
const app = await electron.launch({args:['.',`--user-data-dir=${profile}`],cwd:process.cwd(),timeout:40000})
let page
try {
  page = await app.firstWindow(); page.setDefaultTimeout(7000)
  const errors=[]; page.on('pageerror',e=>{errors.push(e.message);console.error('RENDERER',e.message)})
  await app.evaluate(({ipcMain,BrowserWindow}, {directory,idA,idB})=>{
    const rows=[{id:idA,title:'Review the sample project'},{id:idB,title:'Plan the next iteration'}].map(r=>({...r,directory,project:'sample-project',branch:'main',updated:Math.floor(Date.now()/1000),bytes:20}))
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
  const snap=async name=>{await page.waitForTimeout(400);await page.screenshot({path:join(output,name+'.png'),scale:'css'});console.log('VISUAL',name)}
  await page.reload();
  await page.getByRole('button',{name:'Sessions',exact:true}).click();
  assert.equal(await page.getByRole('searchbox',{name:'Filter sessions',exact:true}).count(),0);
  await snap('00-sidebar-compact');
  await page.getByRole('button',{name:'Filter sessions',exact:true}).click();
  const filter=page.getByRole('searchbox',{name:'Filter sessions',exact:true});
  await filter.fill('next iteration');
  assert.equal(await page.locator('.session-row').count(),1);
  await snap('00-sidebar-search');
  await filter.press('Escape');
  assert.equal(await filter.count(),0);
  assert.equal(await page.getByRole('button',{name:'Filter sessions',exact:true}).evaluate(el=>el===document.activeElement),true);
  await page.getByRole('button',{name:'Bots',exact:true}).click();
  await page.getByRole('button',{name:'Search bots',exact:true}).click();
  await page.getByRole('searchbox',{name:'Search bots',exact:true}).fill('no-such-bot');
  await page.getByText('No bots match this search.',{exact:true}).waitFor();
  await page.getByRole('button',{name:'Close search',exact:true}).click();
  const overview=async()=>{await page.locator('.bot').filter({hasText:'Review Bot'}).click();await page.getByRole('heading',{name:/Conversation history/}).waitFor()}
  await overview();
  assert.equal(await page.locator('.bot-conversations button').count(),3);
  assert.equal(await page.locator('.bot-history-unavailable').count(),1);
  await page.getByRole('button',{name:/Review the sample project/}).getByText(/Archived/).waitFor();
  await snap('01-all-history');
  await page.getByRole('searchbox',{name:'Search bot conversations'}).fill('next iteration');
  assert.equal(await page.locator('.bot-conversations button').count(),1);
  await snap('02-filtered-history');
  await page.locator('.bot-conversations button').click();
  await page.getByRole('textbox',{name:'Message the agent'}).waitFor();
  await overview();await page.locator('.bot-conversations button').filter({hasText:'Review the sample project'}).click();
  await page.getByText('Review the project and show a code example.',{exact:true}).waitFor();
  await snap('03-open-archived-conversation');
  if (!(await page.getByRole('tab',{name:'Files',exact:true}).isVisible())) {
    await page.getByRole('button',{name:'Context panel',exact:true}).click();
  }
  await page.getByRole('tab',{name:'Files',exact:true}).click();
  assert.equal(await page.locator('#panel-files .panel-head').count(),0);
  assert.equal(await page.getByRole('searchbox',{name:'Search project files by name'}).count(),0);
  const panel=await page.locator('.context').boundingBox();
  const tree=await page.locator('.tree-body').boundingBox();
  assert.ok(Math.abs(panel.y+panel.height-tree.y-tree.height-14)<2,'File list fills panel height');
  await snap('03-files-full-height');
  await page.getByRole('button',{name:'Search files',exact:true}).click();
  const fileSearch=page.getByRole('searchbox',{name:'Search project files by name'});
  await fileSearch.fill('sample');
  await page.locator('.file-search-results button').filter({hasText:'src/nested/sample.txt'}).waitFor();
  await snap('03-files-search');
  await fileSearch.press('Escape');
  assert.equal(await fileSearch.count(),0);
  assert.equal(await page.getByRole('button',{name:'Search files',exact:true}).evaluate(el=>el===document.activeElement),true);

  await overview();await page.getByRole('button',{name:'New conversation',exact:true}).click();
  await page.getByRole('button',{name:"Don't trust",exact:true}).click();await page.getByRole('dialog').waitFor({state:'hidden'});
  await page.getByRole('textbox',{name:'Message the agent'}).fill('Keep this newer conversation draft');
  await overview();assert.equal(await page.locator('.bot-conversations button').count(),4);await snap('04-history-after-new');
  await page.keyboard.press('Escape');await page.reload();await overview();
  await page.getByRole('searchbox',{name:'Search bot conversations'}).fill('Keep this newer');
  await page.locator('.bot-conversations button').click();
  await page.getByRole('button',{name:"Don't trust",exact:true}).click();await page.getByRole('dialog').waitFor({state:'hidden'});
  assert.equal(await page.getByRole('textbox',{name:'Message the agent'}).inputValue(),'Keep this newer conversation draft');
  await snap('05-draft-restored-after-reload');
  await overview();await page.getByRole('searchbox',{name:'Search bot conversations'}).fill('no-such-conversation');
  await page.getByText(/No conversations match/).waitFor();await snap('06-no-results');
  assert.deepEqual(errors,[]);console.log('PASS: full bot history, archived navigation, search, new conversation and restart.');
} catch(error) {if(page) await page.screenshot({path:join(output,'failure.png'),scale:'css'});throw error}
finally {await app.close()}

// Bot history acceptance: saved, archived, associated, draft and unavailable records.
// No provider requests or changes to the user's projects. Screenshots accompany assertions.
import assert from 'node:assert/strict'
import { mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
import { idA, idB, installBridge, makeProfile } from './acceptance-fixture.mjs'
const output = process.env.UX_OUTPUT || join(tmpdir(), 'bravebot-bot-history')
mkdirSync(output, { recursive: true })
const { profile, directory } = makeProfile({ conversations: [idA, '44444444-4444-4444-8444-444444444444'] })
writeFileSync(join(profile, 'experience.json'), JSON.stringify({conversations:{[JSON.stringify([directory,idA])]:{archived:true},[JSON.stringify([directory,idB])]:{botSlug:'review-bot'},[JSON.stringify([directory,'draft:older'])]:{botSlug:'review-bot',draft:'Earlier unsent review'}}}))
const app = await electron.launch({args:['.',`--user-data-dir=${profile}`],cwd:process.cwd(),timeout:40000})
let page
try {
  page = await app.firstWindow(); page.setDefaultTimeout(7000)
  const errors=[]; page.on('pageerror',e=>{errors.push(e.message);console.error('RENDERER',e.message)})
  await installBridge(app, { directory })
  const snap=async name=>{await page.waitForTimeout(400);await page.screenshot({path:join(output,name+'.png'),scale:'css'});console.log('VISUAL',name)}
  await page.reload();
  await page.getByRole('button',{name:'Chats',exact:true}).click();
  // The filter is always there: no toggle to open it, and Escape clears it without taking it away.
  const filter=page.getByRole('searchbox',{name:'Search chats',exact:true});
  assert.equal(await filter.count(),1);
  await snap('00-sidebar-compact');
  await filter.fill('next iteration');
  assert.equal(await page.locator('.session-row').count(),1);
  await snap('00-sidebar-search');
  await filter.press('Escape');
  assert.equal(await filter.inputValue(),'');
  await page.waitForFunction(()=>document.querySelectorAll('.session-row').length>1);
  // Switching to Bots opens the first bot's own page.
  await page.getByRole('button',{name:'Bots',exact:true}).click();
  await page.locator('[data-test="bot-conversations"]').waitFor();
  const bots=page.getByRole('searchbox',{name:'Search bots',exact:true});
  await bots.fill('no-such-bot');
  await page.getByText('No bots match this search.',{exact:true}).waitFor();
  await bots.press('Escape');
  // A bot's page: its recent conversations, a composer that starts another, and its details.
  const conversations=page.locator('[data-test="bot-conversations"] button.bot-history-row');
  const overview=async()=>{
    if(!(await page.locator('.bot-open').count())) await page.getByRole('button',{name:'Bots',exact:true}).click();
    await page.locator('.bot').filter({hasText:'Review Bot'}).locator('.bot-open-button').click();
    await page.getByRole('heading',{name:'Recent conversations'}).waitFor()
  }
  const searchHistory=async(text)=>{
    const field=page.getByRole('searchbox',{name:'Search conversations'});
    if(!(await field.count())) await page.getByRole('button',{name:'Search conversations',exact:true}).click();
    await field.fill(text)
  }
  await overview();
  // Saved, archived and associated conversations, the unsent draft, and a record the list no longer
  // has, which is said rather than dropped.
  assert.equal(await conversations.count(),3);
  assert.equal(await page.locator('[data-test="bot-conversations"] .bot-history-row.unavailable').count(),1);
  await conversations.filter({hasText:'Review the sample project'}).getByText(/Archived/).waitFor();
  assert.equal(await page.locator('[data-test="bot-details"]').count(),1,'the bot page shows its details beside the list');
  await snap('01-all-history');
  await searchHistory('next iteration');
  assert.equal(await conversations.count(),1);
  await snap('02-filtered-history');
  await conversations.click();
  await page.getByRole('textbox',{name:'Message the agent'}).waitFor();
  await overview();await conversations.filter({hasText:'Review the sample project'}).click();
  await page.getByText('Review the project and show a code example.',{exact:true}).waitFor();
  await snap('03-open-archived-conversation');
  if (!(await page.getByRole('tab',{name:'Files',exact:true}).isVisible())) {
    await page.getByRole('button',{name:'Context panel',exact:true}).click();
  }
  await page.getByRole('tab',{name:'Files',exact:true}).click();
  assert.equal(await page.locator('#panel-files summary').count(),0);
  assert.equal(await page.getByRole('searchbox',{name:'Search project files by name'}).count(),0);
  await page.locator('.tree-body').waitFor();
  const panel=await page.locator('.context').boundingBox();
  const tree=await page.locator('.tree-body').boundingBox();
  assert.ok(Math.abs(panel.y+panel.height-tree.y-tree.height-12)<2,'File list fills panel height');
  await snap('03-files-full-height');
  await page.getByRole('button',{name:'Search files',exact:true}).click();
  const fileSearch=page.getByRole('searchbox',{name:'Search project files by name'});
  await fileSearch.fill('sample');
  await page.locator('.file-search-results .search-result').filter({hasText:'src/nested/sample.txt'}).waitFor();
  await snap('03-files-search');
  await fileSearch.press('Escape');
  await fileSearch.waitFor({state:'detached'});
  assert.equal(await fileSearch.count(),0);
  assert.equal(await page.getByRole('button',{name:'Search files',exact:true}).evaluate(el => el === document.activeElement || el.getRootNode().host === document.activeElement),true);

  // A new conversation is started from the bot's page by sending to it. With no project picked it
  // runs in the bot's home folder, so there is no project chip and no context column.
  await overview();
  assert.equal((await page.locator('[data-test="project-trigger"]').textContent())?.trim(),'No project');
  const sent=(await app.evaluate(()=>globalThis.ux.sent.length));
  await page.getByRole('textbox',{name:'Message the agent'}).fill('Start a newer conversation');
  await page.getByRole('button',{name:'Send',exact:true}).click();
  await page.getByRole('button',{name:"Don't trust",exact:true}).click();await page.getByRole('dialog').waitFor({state:'hidden'});
  // The message waits in the queue for the trust question, then goes; polled, since it is sent on
  // the main process's side.
  let delivered=[];
  for(let tries=0;tries<50&&!delivered.length;tries++){delivered=await app.evaluate((_,n)=>globalThis.ux.sent.slice(n),sent);if(!delivered.length)await page.waitForTimeout(100)}
  assert.equal(delivered.at(-1)?.slug,'review-bot','the first message goes to the bot');
  assert.equal(delivered.at(-1)?.prompt,'Start a newer conversation');
  assert.equal(await page.locator('.app.no-context').count(),1,'a conversation with no project has no context column');
  assert.equal(await page.locator('.transcript-head .where').count(),0,'and no project chip');
  await page.getByRole('textbox',{name:'Message the agent'}).fill('Keep this newer conversation draft');
  await snap('04-history-after-new');
  await page.reload();
  await page.getByRole('button',{name:'Chats',exact:true}).click();
  await page.locator('.session').filter({hasText:'Keep this newer'}).click();
  await page.getByRole('button',{name:"Don't trust",exact:true}).click();await page.getByRole('dialog').waitFor({state:'hidden'});
  assert.equal(await page.getByRole('textbox',{name:'Message the agent'}).inputValue(),'Keep this newer conversation draft');
  await snap('05-draft-restored-after-reload');
  await overview();await searchHistory('no-such-conversation');
  await page.getByText(/No conversation matches/).waitFor();await snap('06-no-results');
  assert.deepEqual(errors,[]);console.log('PASS: full bot history on the bot page, archived navigation, search, a new no-project conversation and a draft kept across restart.');
} catch(error) {if(page) await page.screenshot({path:join(output,'failure.png'),scale:'css'});throw error}
finally {await app.close()}

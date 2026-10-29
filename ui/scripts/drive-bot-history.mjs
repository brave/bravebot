// Bot history acceptance: saved, archived, associated, draft and unavailable records.
// No provider requests or changes to the user's projects. Screenshots accompany assertions.
import assert from 'node:assert/strict'
import { mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
import { idA, idB, installBridge, makeProfile } from './acceptance-fixture.mjs'
import { BOT_OPEN_BUTTON, COMPOSER_NAME } from './drive-support.mjs'
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
  // The chat open is one of Review Bot's, so switching to Bots keeps it. Opening the bot opens its
  // conversation in its home folder.
  await page.getByRole('button',{name:'Bots',exact:true}).click();
  await page.locator('.bot').filter({hasText:'Review Bot'}).locator(BOT_OPEN_BUTTON).click();
  await page.locator('[data-test="bot-greeting"]').waitFor();
  const bots=page.getByRole('searchbox',{name:'Search bots',exact:true});
  await bots.fill('no-such-bot');
  await page.getByText('No bots match this search.',{exact:true}).waitFor();
  await bots.press('Escape');
  // A bot's earlier conversations, reached from its conversation's header, and its details.
  const conversations=page.locator('[data-test="bot-conversations"] button.bot-history-row');
  const openBot=async()=>{
    if(!(await page.locator('.bot-open').count())) await page.getByRole('button',{name:'Bots',exact:true}).click();
    await page.locator('.bot').filter({hasText:'Review Bot'}).locator(BOT_OPEN_BUTTON).click();
    await page.locator('[data-test="earlier-conversations"]').waitFor()
  }
  const overview=async()=>{
    await openBot();
    await page.locator('[data-test="earlier-conversations"]').click();
    await page.getByRole('heading',{name:'Earlier conversations'}).waitFor()
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
  assert.equal(await page.locator('[data-test="bot-details"]').count(),1,'the page shows the bot’s details beside the list');
  assert.equal(await page.locator('[data-test="bot-page"] .composer').count(),0,'the page starts no conversation of its own');
  await snap('01-all-history');
  await searchHistory('next iteration');
  assert.equal(await conversations.count(),1);
  await snap('02-filtered-history');
  await conversations.click();
  await page.getByRole('textbox',{name:COMPOSER_NAME}).waitFor();
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

  // Opening the bot again goes back to the conversation in its home folder, which has no project
  // chip and no context column, and the back button on its earlier conversations does the same.
  await overview();
  await page.locator('[data-test="earlier-back"]').click();
  await page.locator('[data-test="bot-greeting"]').waitFor();
  await openBot();
  await page.locator('[data-test="bot-greeting"]').waitFor();
  const sent=(await app.evaluate(()=>globalThis.ux.sent.length));
  await page.getByRole('textbox',{name:COMPOSER_NAME}).fill('Start a newer conversation');
  await page.getByRole('button',{name:'Send',exact:true}).click();
  // Polled, since it is sent on the main process's side.
  let delivered=[];
  for(let tries=0;tries<50&&!delivered.length;tries++){delivered=await app.evaluate((_,n)=>globalThis.ux.sent.slice(n),sent);if(!delivered.length)await page.waitForTimeout(100)}
  assert.equal(delivered.at(-1)?.slug,'review-bot','the first message goes to the bot');
  assert.equal(delivered.at(-1)?.prompt,'Start a newer conversation');
  assert.equal(await page.locator('.app.no-context').count(),0,'a bot’s conversation keeps the context column');await page.getByRole('button',{name:/context panel/i}).first().waitFor();await page.locator('#context-column [data-test="bot-details"]').waitFor();assert.equal(await page.locator('[data-test="inspector-tabs"]').count(),0,'the column holds the bot, not the files and plan');
  assert.equal(await page.locator('.transcript-head .where').count(),0,'and no project chip');
  await page.getByRole('textbox',{name:COMPOSER_NAME}).fill('Keep this newer conversation draft');
  await snap('04-history-after-new');
  // Opening the bot after a restart goes back to the same conversation, with the draft kept.
  await page.reload();
  await openBot();
  assert.equal(await page.getByRole('textbox',{name:COMPOSER_NAME}).inputValue(),'Keep this newer conversation draft');
  await snap('05-draft-restored-after-reload');
  // With the home conversation archived, opening the bot makes a new one. The archived one stays
  // open in the window, and reopening it would leave the bot with no conversation that is not archived.
  const made=()=>app.evaluate(()=>globalThis.ux.created.length);
  const before=await made();
  await page.getByRole('button',{name:'Chats',exact:true}).click();
  await page.getByRole('button',{name:'Actions for New Chat'}).click();
  await page.getByRole('menuitem',{name:'Archive conversation'}).click();
  const confirm=page.getByRole('button',{name:/^Archive$/});
  if(await confirm.count()) await confirm.click();
  await page.getByRole('button',{name:'Bots',exact:true}).click();
  await openBot();
  await page.locator('[data-test="bot-greeting"]').waitFor();
  assert.equal(await made(),before+1,'opening the bot asked for a new conversation instead of reopening the archived one');
  await snap('05-new-home-after-archive');
  await overview();await searchHistory('no-such-conversation');
  await page.getByText(/No conversation matches/).waitFor();await snap('06-no-results');
  assert.deepEqual(errors,[]);console.log('PASS: opening a bot opens its home conversation; its earlier conversations, archived navigation and search are a page reached from the header; a message in the home conversation goes to the bot, and a draft is kept across restart.');
} catch(error) {if(page) await page.screenshot({path:join(output,'failure.png'),scale:'css'});throw error}
finally {await app.close()}

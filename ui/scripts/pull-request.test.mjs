// What a sidebar row shows of a chat's pull request: `PR #<number>`, a link to the recorded
// address, and nothing for an address that is not a pull request.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

function load(path) {
  const source = buildSync({
    entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs', jsx: 'automatic',
    external: ['react', 'react-dom', 'react/jsx-runtime'],
  }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { pullRequestLink } = load('src/shared/pull-request.ts')
const { PullRequestMark } = load('src/renderer/components/PullRequestMark.tsx')
const { matching } = load('src/renderer/components/Sessions.tsx')

test('a pull request address reads as its number and links to the address recorded', () => {
  const url = 'https://github.com/brave/bravebot/pull/4309'
  assert.deepEqual(pullRequestLink(url), { text: 'PR #4309', href: url })
})

test('the text is the number alone, whatever else the address carries', () => {
  const url = 'https://github.com/brave/bravebot/pull/4309/files?diff=split#top'
  assert.equal(pullRequestLink(url)?.text, 'PR #4309')
  assert.equal(pullRequestLink(url)?.href, url)
})

test('an address that is not an http or https pull request draws no link', () => {
  for (const url of [
    null, undefined, '', 'not a url',
    'https://github.com/brave/bravebot/issues/4309',
    'https://github.com/brave/bravebot/pull/abc',
    'https://github.com/brave/bravebot/pulls',
    'javascript:alert(1)//pull/1',
    'file:///tmp/pull/12',
  ]) assert.equal(pullRequestLink(url), null, String(url))
})

test('the link reads PR #<number>, opens through the window-open road, and shows no URL as text', () => {
  const url = 'https://github.com/brave/bravebot/pull/4309'
  const html = renderToStaticMarkup(React.createElement(PullRequestMark, { url }))
  assert.match(html, />PR #4309<\/a>$/)
  assert.match(html, /<a [^>]*href="https:\/\/github.com\/brave\/bravebot\/pull\/4309"/)
  assert.match(html, /target="_blank"/)
  assert.match(html, /rel="noopener noreferrer nofollow"/)
  assert.equal(renderToStaticMarkup(React.createElement(PullRequestMark, { url: 'https://example.com/x' })), '')
})

test('the search filter matches the pull request text a row shows and not the URL behind it', () => {
  const row = (extra) => ({ id: 'a', directory: '/w/app', project: 'app', branch: 'main', title: 'Fix', updated: 1, bytes: 1, ...extra })
  const withPr = row({ pullRequest: 'https://github.com/brave/bravebot/pull/4309' })
  const without = row({})
  assert.deepEqual(matching([withPr, without], 'PR #4309'), [withPr])
  assert.deepEqual(matching([withPr, without], '4309'), [withPr])
  assert.deepEqual(matching([withPr, without], 'github'), [])
})

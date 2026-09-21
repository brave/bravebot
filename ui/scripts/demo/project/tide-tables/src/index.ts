#!/usr/bin/env node
import { readFileSync } from 'node:fs'
import { parseTable } from './parse.ts'
import { asStations } from './format.ts'

const [, , file] = process.argv
if (!file) {
  console.error('usage: tide-tables <export.csv>')
  process.exit(2)
}

const rows = parseTable(readFileSync(file, 'utf8'))
process.stdout.write(JSON.stringify(asStations(rows), null, 2) + '\n')

import { test } from 'node:test'
import { CHUNKINGS, loadScenarios, runScenario } from './support/scenario.js'

// Each scenario is played under every chunking, because a stream gives no promise about where
// one read ends. The scenarios are language-independent data under test-fixtures/scenarios.
for (const scenario of loadScenarios()) {
  for (const chunking of CHUNKINGS) {
    test(`${scenario.name} (${chunking} chunks): ${scenario.description}`, async () => {
      await runScenario(scenario, chunking)
    })
  }
}

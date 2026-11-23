# token-bench

Compares the tokens a coding agent spends on the same tasks, with the same model and cache
settings. It runs bravebot, Claude Code and Codex headless, plus a minimal `reference` loop that
calls an OpenAI-compatible endpoint (OpenRouter by default) directly. The reference loop shows what
a plain agent costs on a route, so the difference between it and a tool is the tool's own overhead.

Standard library Python. Plots need matplotlib.

```sh
contrib/token-bench/bench.py --selftest
contrib/token-bench/bench.py list --config contrib/token-bench/contestants.example.json
contrib/token-bench/bench.py run --config my.json --repeats 3
contrib/token-bench/bench.py report contrib/token-bench/results/<file>.jsonl --plot plots/
```

`run` writes one JSON line per turn to `results/` (ignored by git) and prints the ranking when it
ends. `report` reads that file again, so a ranking and the plots can be remade without running
anything. `--only` and `--scenarios` narrow a run, and `--no-warmup` skips the unscored first call.

## What is measured

Each tool reports usage differently, so `bench.py` reduces every turn to five numbers:

| Number | Meaning |
|---|---|
| `fresh` | prompt tokens neither read from nor written to a cache |
| `cache_read` | prompt tokens served from the cache |
| `cache_write` | prompt tokens written to the cache |
| `output` | tokens the model wrote |
| `requests` | model calls the turn made |

The tools differ in ways that would skew the numbers if read raw. `bench.py` handles each one:

- **bravebot** `--json`: `total` is prompt plus output, so `fresh` is what is left after output
  and both cache figures.
- **Claude Code** `--output-format json`: `input_tokens` is net of the cache. The usage is already
  summed over every request in the invocation.
- **Codex** `exec --json`: `input_tokens` includes the cached part, and a resumed turn reports the
  whole thread's running total. The turn's cost is the difference from the turn before.
- **reference**: reads `prompt_tokens_details.cached_tokens` and `cache_write_tokens`, and puts a
  cache mark on the system prompt and the last message.

The selftest runs the parsers on output captured from the real tools, with totals checked by hand.

## Score

A turn's cost is one number in units of an uncached input token: fresh tokens count 1, cache
reads 0.10, cache writes 1.25 (Anthropic) or 1.0 (OpenAI), and output 5 or 8. A contestant can
choose a profile or state its own numbers with `"weights": {"write": 1.25, "read": 0.1,
"output": 5}`. These are list-price ratios, so change them if the prices you pay differ.

For each scenario, the cheapest contestant that solved it in every repeat scores 100, and the others
score `100 * best / cost`. A contestant that failed any repeat is scaled down by its success rate,
so a cheap wrong answer cannot win. A contestant's score is the mean over scenarios, and each
category (overhead, tools, cache, system_prompt) has its own mean. Costs are medians over repeats.

## Scenarios

| Scenario | Category | Measures |
|---|---|---|
| `hello` | overhead | the fixed cost of the system prompt and tool schemas |
| `read_count` | tools | a few file reads and a sum |
| `fix_bug` | tools | reading a failing script, an edit and a re-run, checked by running the test |
| `find_symbol` | tools | searching forty files for one definition |
| `multi_turn` | cache | five turns over one conversation, and how much of each is read from the cache |
| `sysprompt_short`, `sysprompt_long` | system_prompt | the effect of 10 and 150 added rules |

Every scenario has a check on the result, so a tool that spends nothing by not doing the work
fails. Add a scenario by adding a `Scenario` to `scenarios()`.

## Comparing models and routes

A contestant is a tool and a model, so the same tool can appear several times. Bedrock against
Brave's endpoint, or against OpenRouter, is two or three entries:

```json
{"contestants": [
  {"name": "bravebot-bedrock",    "tool": "bravebot", "model": "sonnet", "args": ["--safe"]},
  {"name": "bravebot-openrouter", "tool": "bravebot", "model": "openrouter/anthropic/claude-sonnet-4.5", "args": ["--safe"]},
  {"name": "claude-bedrock",      "tool": "claude",   "args": ["--bare"]},
  {"name": "ref-openrouter",      "tool": "reference", "model": "anthropic/claude-sonnet-4.5"}
]}
```

Fields: `name`, `tool` (`bravebot`, `claude`, `codex`, `reference`), `model` (passed as `--model`),
`args` (extra flags), `env` (extra environment, `${VAR}` is expanded), `bin`, `timeout`, `weights`.
The `reference` tool also takes `base_url` and `key_env`.

For a fair comparison on a model, give every contestant the same underlying model. `model` for
bravebot is a tier word from `ANTHROPIC_DEFAULT_*_MODEL` for Bedrock, `openrouter/<model>` for
OpenRouter, or a Brave roster name. Claude Code takes its route from its own settings, and
`bench.py` does not change it. Check each result's reply model (bravebot `--json` has a `model`
field) before comparing the numbers.

## Fairness

The benchmark controls these, and each can change a result:

- **Working directory.** The same path is used on every run. Tools put the path in the system
  prompt, and a path that changed would change the cached prefix.
- **Configuration loaded.** `--safe` (bravebot) and `--bare` (Claude Code) skip user instructions,
  skills, hooks and MCP servers, so a result reflects the tool and not one machine's setup. Run
  without them to measure what a person would see, as a separate contestant.
- **Warm-up.** Each contestant makes one unscored call first so the static prefix is already
  cached for the others to read.
- **Permissions.** Tools run with permission checks off, in a temporary directory.

## Known limits

- Two identical `bravebot -p` calls in a row, with and without `--incognito` and `--safe`, each
  wrote the whole prefix (about 29,000 tokens with `--safe`) and read nothing from the cache,
  while rounds inside one invocation did read it. Claude Code and Codex read about 90% of their
  prompt in the same conditions. The cause is not established. The environment block of the system
  prompt (`crates/agent/src/preamble.rs`) holds a scratch directory that differs for each session,
  which would change the prefix. This is the largest gap in the first results.
- Cache lifetime is about five minutes on Anthropic models, so a run that spends longer between
  turns than that measures a cold cache.
- Codex does not report cache writes for these models, so its write cost is zero.
- Three repeats is the default. Cache behaviour on a shared endpoint is noisy, so raise `--repeats`
  before drawing a conclusion from a small gap.
- Costs are list-price ratios, not invoices. Claude Code also reports `total_cost_usd`, which is
  recorded as `cost_usd` for comparison.

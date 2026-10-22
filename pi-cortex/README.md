# pi-cortex

[Cortex](https://github.com/thrashr888/cortex) memory for [pi](https://github.com/earendil-works/pi).
The agent saves what it learns, finds it again later, and Cortex consolidates it in the
background, so a long-lived session (pi-durable, Tavern's host) keeps what compaction summaries
drop.

## What it does

- **Tools:** `cortex_save` (one fact, decision, preference, fix or pattern) and `cortex_recall`
  (search this agent's memory and the global `~/.cortex` store).
- **Every prompt** gets a `<memory>` block in the system prompt: how to use memory, the
  consolidated context (entities, learned patterns, global knowledge) and the memories that
  match the message.
- **Session start:** creates `.cortex/` in the session's folder if needed, then `cortex wake`
  catches up anything an earlier run saved but never consolidated.
- **Compaction:** `cortex sleep`. What the summary leaves out is still in Cortex.
- **Turn end:** `cortex sleep` once 15 new memories have piled up.
- **Nightly:** `cortex dream` once a day after 3:00 local time.
- **Shutdown:** `cortex sleep --micro` (no LLM).

Writes to the store run one at a time, so a save never races a consolidation.

## Model access comes from pi

Cortex's sleep and dream call a model. pi-cortex hands Cortex the agent's own pi login on each
run: it picks a Haiku on the session model's provider (else the session model, else any Haiku
pi can reach; `CORTEX_PI_MODEL=provider/id` to choose), resolves its key, base URL and headers
through pi (so a gateway's workspace header comes along), and passes them as `ANTHROPIC_API_KEY`,
`ANTHROPIC_BASE_URL`, `CORTEX_HEADERS` and `CORTEX_MODEL`. Nothing to configure in Cortex.

Only Anthropic Messages models qualify. Subscription (OAuth) logins aren't passed on; without
another key Cortex falls back to LLM-free micro sleeps, and saving and recall still work.

## Install

pi-cortex runs the `cortex` binary: `../target/release/cortex` when it sits in a cortex checkout,
else `cortex` on `PATH` (`brew install thrashr888/tap/cortex`), or `CORTEX_BIN`.

```bash
pi install /path/to/cortex/pi-cortex      # from a checkout
```

## Settings

| Variable | Default | |
|---|---|---|
| `CORTEX_BIN` | see above | cortex binary |
| `CORTEX_PI_DIR` | session cwd | folder holding the agent's `.cortex/` |
| `CORTEX_PI_MODEL` | Haiku on the session's provider | `provider/id` of the pi model Cortex uses |
| `CORTEX_PI_INHERIT` | `1` | `0`: don't pass pi's model access; Cortex uses its own env |
| `CORTEX_PI_SLEEP_AFTER` | `15` | new memories that trigger a sleep at turn end (`0` never) |
| `CORTEX_PI_DREAM_HOUR` | `3` | local hour after which the nightly dream runs (`-1` never) |
| `CORTEX_MODEL` | | overrides the model pi-cortex picks |
| `CORTEX_EXTRACT_ON_SAVE` | `0` here | `1` extracts entities on every save instead of in sleep |

## Tests

```bash
cargo build && cd pi-cortex && npm test     # Node 22.18+; uses ../target/debug/cortex
```

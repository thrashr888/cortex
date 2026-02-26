
Lightweight agent memory tool for repo-based self-learning
Shared by Paul
This is a copy of a chat between Claude and Paul. Content may include unverified or unsafe content that do not represent the views of Anthropic. Shared snapshot may contain attachments and data not displayed here.

What is a tiny tool that can be used to enable self-learning with a single repo or project, to be used via CLI or MCP within an AI agent. Maybe it’s agent memory but I want it to have a small footprint and potentially cohost the db with the repo or have db files per repo. Like SQLite or Turso or similar. Auto updating skill files might be a way. Think creatively and research our options.

Feb 24

Searched the web
Searched the web




Here's the landscape, ranked by fit for your "tiny, repo-local, self-learning" requirements:

Winner: Engram ⭐
Single Go binary, SQLite + FTS5, zero dependencies. This is exactly your spec.

DB location: ~/.engram/engram.db (global) — but supports engram sync to export compressed chunks to .engram/ in the repo for git-committing learnings
Interfaces: CLI, MCP (stdio), HTTP API, TUI (Bubbletea)
Philosophy: Agent decides what's worth remembering via mem_save, not firehose capture
Key insight: FTS5 covers 95% of use cases without vector DB complexity
Agent support: Claude Code plugin, OpenCode, Cursor, Windsurf, Gemini CLI, Codex
Install: brew install gentleman-programming/tap/engram
The engram sync feature is the magic — it exports memories as compressed JSON chunks to .engram/ in your project dir, so you can commit learned knowledge alongside the code.

Runner-up: mcp-memory-libsql-go
Same idea but uses libSQL (Turso's SQLite fork) with optional vector search. Go binary. Key differentiator: you can use file:./my-memory.db for per-repo local DB, or point to Turso for cloud sync. Has knowledge graph support built in.

The Markdown-Native Approach: project-memory skill
Zero-DB option. Just .claude/skills/project-memory/ with markdown files: bugs.md, decisions.md, key_facts.md. Agent reads/writes them directly. Git-native by design. The simplest possible approach, but no search beyond what the LLM does natively over the files.

Creative ideas you didn't ask about:
Hybrid: Engram + auto-updating skill files — Use an engram hook or Claude Code SubagentStop hook to periodically distill engram memories into a .claude/skills/learned/SKILL.md file. This gives you both structured DB search AND skill-file format that any agent loads automatically. The distillation step is key: run a summarization pass that turns raw memories into actionable patterns.
Build your own ~200 LOC tool — A Python/Go CLI that wraps a single SQLite file at .ai-memory/memory.db in the repo root. Three commands: learn, recall, distill. distill generates a LEARNED.md skill file from the DB. Expose via MCP with 3 tools. This is genuinely buildable in an afternoon and could be an interesting open-source project — there's a gap for something even simpler than Engram.
libSQL embedded in your agent harness — Since you're evaluating PydanticAI/LiteLLM, you could embed libsql-client directly in your Python agent with a memory.db per repo. Skip the MCP layer entirely. ~50 lines of code for a memory module that your agent imports.
(Speculative) SQLite + FTS5 + sqlite-vec in a single file — Use sqlite-vec for embeddings stored inline. One .db file gets you full-text AND semantic search. No external embedding service needed if you use a tiny local model like all-MiniLM-L6-v2 (22MB). This is what mcp-memory-service does but it's bloated — you could strip it to essentials.
My recommendation
Start with Engram for immediate productivity. If it feels too heavy or you want tighter repo integration, build option #2 as a tiny open-source tool — "repo-brain" or similar. The market gap is a tool that's:

Single file in the repo (.ai-memory/brain.db)
Auto-generates .claude/skills/learned/SKILL.md on each session close
Works as both CLI and MCP server
<500 LOC
That doesn't exist yet and would be a great fit for your GitHub portfolio + Terraform ecosystem audience.

URLs:

https://github.com/Gentleman-Programming/engram
https://github.com/ZanzyTHEbar/mcp-memory-libsql-go
https://github.com/SpillwaveSolutions/project-memory
https://github.com/joleyline/mcp-memory-libsql
https://github.com/CaviraOSS/OpenMemory
https://github.com/doobidoo/mcp-memory-service
https://github.com/anthropics/skills
Turn on web search in Search and tools menu. Otherwise, links provided may not be accurate or up to date.

Memory alone isn’t good enough. It should have compaction or reasoning over the memories or some kind of recursive improvement. Some ideas include a sleep state where memories move to long term memory.

Feb 24

Searched the web
Searched the web



OK, now we're talking about something much more interesting than CRUD memory. Here's the landscape of cognitive memory — systems that actually think about what they know:

Tier 1: Existing Tools with Reasoning/Consolidation
Letta (formerly MemGPT) — The gold standard, but NOT tiny
Sleep-time agents: A separate agent runs async between sessions to consolidate, defrag, and reorganize memory blocks. Uses a cheaper/slower model for reflection while the primary agent uses a fast model for conversation.
Recursive summarization: Evicted messages are summarized with existing summaries, creating progressive abstraction layers.
Memory defrag: You can trigger defrag subagents that refactor redundant memory blocks.
Downside: It's a full platform (Docker, Postgres, API server). Opposite of tiny. But the architecture is what you should study.
SimpleMem (paper: Jan 2026) — Research-grade, Python
Three-stage pipeline inspired by CLS (Complementary Learning Systems) theory:

Semantic Structured Compression: Entropy-aware filtering discards noise, distills to atomic facts
Recursive Memory Consolidation: Clusters memories by affinity (semantic similarity + temporal proximity), merges clusters above 0.85 threshold into abstract representations. "User ordered latte × 47 times" → "User prefers hot coffee with oat milk"
Adaptive Query-Aware Retrieval: Estimates query complexity, adjusts retrieval depth dynamically
Uses LanceDB for vectors. 26% better F1 than Mem0, 30x fewer tokens. Has MCP support. But it's Python, uses OpenAI API calls for consolidation, and is research-oriented — not exactly "tiny".

Engram — Closest to tiny, has some consolidation
mem_session_summary forces structured summaries at session end (Goal/Discoveries/Accomplished/Files)
Passive capture extracts ## Key Learnings: sections automatically
Missing: No background consolidation, no sleep-time reasoning, no recursive improvement
Tier 2: The Architecture You Actually Want to Build
None of the existing tiny tools do what you're describing. Here's a design for a ~500 LOC tool that does:

"cortex" — repo-local cognitive memory
.cortex/
├── raw.db          # SQLite + FTS5 — episodic memory (all observations)
├── consolidated.db # SQLite — long-term memory (merged/abstracted)
├── skills/         # Auto-generated SKILL.md files from learned patterns
└── config.toml     # consolidation settings
Three modes of operation:

1. Wake (during agent sessions)

cortex save / MCP mem_save — store observations with type tags (bugfix, decision, pattern, preference)
cortex recall — FTS5 search + recency-weighted scoring
Lightweight, fast, no LLM calls
2. Sleep (cron job or session-end hook) This is the key innovation. Runs an LLM pass over recent raw memories to:

Consolidate: Cluster related observations, merge into abstract representations
Contradict: Detect conflicting memories, keep the most recent, archive old ones
Promote: Move high-frequency patterns from raw.db → consolidated.db
Decay: Score memories by recency × access_count × importance. Below threshold → archive
Distill: Generate/update .cortex/skills/SKILL.md files from consolidated patterns
The sleep pass uses a single structured LLM call:

Given these N recent memories and the existing consolidated memories,
output JSON with: {consolidations, contradictions, promotions, decayed, skill_updates}
3. Dream (periodic deep reflection — weekly/manual)

Cross-session pattern mining: "You've hit this class of bug 5 times across 3 projects"
Meta-learning: Updates the consolidation prompts themselves based on what's been useful
Generates "insight" entries that are high-priority in recall
Key design decisions:

Two-DB architecture mirrors hippocampus (fast episodic) / neocortex (slow consolidated) from CLS theory
Sleep runs locally with a cheap model (Haiku, gemma-3 via Ollama, or even claude-sonnet-4-5 via API) — you only need reasoning, not generation
Skill file output means any MCP-compatible agent automatically benefits from learned patterns without needing the cortex tool itself
Per-repo DB means knowledge is contextual: your Terraform project learns different things than your Python side project
Git-committable: .cortex/consolidated.db and .cortex/skills/ can be committed. Raw can be .gitignored
Interface:

bash
cortex save "Fixed N+1 query — needed eager loading on UserList" --type bugfix
cortex recall "performance issues"
cortex sleep                    # run consolidation pass
cortex dream                    # deep reflection
cortex stats                    # memory health: count, staleness, consolidation ratio
cortex mcp                      # MCP stdio server
What makes this different from everything else
Feature	Engram	Letta	SimpleMem	cortex (proposed)
Tiny footprint	✅	❌	❌	✅
Per-repo DB	❌ (global)	❌	❌	✅
Sleep consolidation	❌	✅	✅ (async)	✅
Recursive abstraction	❌	✅	✅	✅
Auto skill files	❌	❌	❌	✅
Memory decay	❌	❌	❌	✅
Contradiction detection	❌	partial	❌	✅
Dream/meta-learning	❌	❌	❌	✅
No vector DB needed	✅	❌	❌	✅
MCP + CLI	✅	✅	✅	✅
The auto-generating skill files is the killer feature nobody else does. It closes the loop: learn → consolidate → distill → inject back into agent context automatically.

Implementation recommendation
Go for the binary (like Engram), single LLM call for sleep (not a complex pipeline), SQLite + FTS5 for storage (no vector DB — FTS5 + LLM reranking beats vector search for repo-scoped knowledge). The sleep/dream commands can shell out to claude CLI or hit the Anthropic API directly.

(Speculative): This could be a really compelling open-source project for the Terraform ecosystem audience — "give your IaC agent a brain that learns your patterns." The skill-file output means it works with Claude Code, Codex, Cursor, Gemini CLI without any of them needing to know about cortex.

URLs:

https://www.letta.com/blog/sleep-time-compute
https://www.letta.com/blog/agent-memory
https://github.com/aiming-lab/SimpleMem
https://arxiv.org/abs/2601.02553
https://github.com/Gentleman-Programming/engram
https://mem0.ai/research
https://arxiv.org/abs/2504.19413
https://aws.amazon.com/blogs/machine-learning/building-smarter-ai-agents-agentcore-long-term-memory-deep-dive/
Turn on web search in Search and tools menu. Otherwise, links provided may not be accurate or up to date.

Ok this is better. does it require a daemon service? How does it work with my laptop closed? Maybe use session close hooks or like every 10th message hook or something?

Feb 24
No daemon needed. Here's how it works purely with hooks and triggers:

Trigger Architecture
Claude Code hooks are the perfect mechanism. They fire on specific lifecycle events — no background process, no daemon, no cron.

jsonc
// .claude/hooks/hooks.json
{
  "hooks": {
    "Stop": [{
      "command": "cortex sleep --quick",     // light consolidation on session end
      "timeout": 15000
    }],
    "SubagentStop": [{
      "command": "cortex sleep --micro",     // even lighter pass after subagent work
      "timeout": 5000
    }],
    "Notification": [{
      "command": "cortex maybe-sleep",       // checks message count, runs if threshold hit
      "timeout": 10000
    }]
  }
}
```

## Three consolidation tiers, no daemon

**Micro sleep** (~2 seconds, no LLM call)
- Trigger: Every `SubagentStop`, or `cortex save` can internally check counter
- What it does: Pure SQLite operations — dedup exact matches, update access counts, compute decay scores, delete anything below threshold
- Cost: Zero API calls. Just SQL.

**Quick sleep** (~5-10 seconds, 1 LLM call)
- Trigger: `Stop` hook (session end), or manually via `cortex sleep`
- What it does: Gathers unprocessed memories since last sleep, sends one structured prompt to consolidate. Marks processed memories. Updates skill files if patterns changed.
- Cost: 1 API call, ~2K tokens typically

**Deep dream** (~30-60 seconds, 2-3 LLM calls)  
- Trigger: **Manual only** — `cortex dream` or `/dream` slash command
- What it does: Cross-session pattern mining, meta-learning, full skill file regeneration, contradiction resolution across all consolidated memories
- Cost: 2-3 API calls. You run this when you want to, not automatically.

## The message-counter approach

Instead of a hook on every Nth message (which Claude Code doesn't directly support), track it internally:
```
cortex save "some observation"  ← increments counter in raw.db
cortex save "another thing"     ← counter = 2
...
cortex save "tenth thing"       ← counter hits threshold → auto-triggers micro sleep
The save command itself checks SELECT COUNT(*) FROM raw WHERE consolidated = 0. If it exceeds a configurable threshold (default 10), it runs micro sleep inline before returning. Adds maybe 200ms. The agent doesn't even notice.

What happens when your laptop closes
Nothing — and that's fine. Here's why:

Stop hook fires before Claude Code exits — this is the normal path. Quick sleep runs, memories get consolidated.
Hard close (lid slam, crash) — raw memories are already in SQLite (written on each save). They're just not consolidated yet. Next session start, cortex detects unconsolidated memories and runs quick sleep as the first action.
Session start hook can handle this:
jsonc
"SessionStart": [{
  "command": "cortex wake",   // checks for unconsolidated memories, runs sleep if needed
  "timeout": 15000
}]
```

So the lifecycle is:
```
Session start → cortex wake (catch up if needed)
  ↓
Working... cortex save × N (micro sleep triggers at threshold)
  ↓
Session end → Stop hook → cortex sleep --quick
  ↓
Laptop closes. Nothing runs. Nothing needs to.
  ↓
Next session start → cortex wake (verify, inject context)
```

## For non-Claude-Code agents

Same pattern, different hook mechanisms:

- **Cursor**: `.cursor/rules` can reference a pre-session script
- **OpenCode**: Plugin lifecycle hooks (`onSessionStart`, `onSessionEnd`)
- **Codex**: `.codex/setup.sh` runs on session init
- **Git hook fallback**: `post-commit` hook runs `cortex maybe-sleep` — you're committing regularly anyway, so this catches consolidation even without agent hooks
- **Manual**: Just `cortex sleep` in your terminal whenever. It's idempotent.

## Simplified state machine
```
         ┌─────────────────────────────┐
         │  raw.db                      │
         │  unconsolidated memories     │
         │  counter: N                  │
         └──────────┬──────────────────┘
                    │
        ┌───────────┼───────────────┐
        │           │               │
   N >= 10      Stop hook      wake (startup)
   (inline)     (session end)  (catch-up)
        │           │               │
        ▼           ▼               ▼
   micro sleep  quick sleep    quick sleep
   (SQL only)   (1 LLM call)  (1 LLM call)
        │           │               │
        └───────────┼───────────────┘
                    │
                    ▼
         ┌─────────────────────────────┐
         │  consolidated.db             │
         │  .cortex/skills/SKILL.md     │
         └─────────────────────────────┘
                    │
              manual trigger
                    │
                    ▼
              deep dream
              (2-3 LLM calls)
The key insight: consolidation doesn't need to be real-time. Human brains consolidate during actual sleep. Your agent can consolidate at session boundaries. The raw memories are always safe in SQLite — consolidation is an optimization pass, not a requirement for recall to work.

What about vidmem? What about an opencode or pi agent extension?

Feb 24

Searched the web
Searched the web




Good catches. Here's where each fits:

Memvid
You were close — it's Memvid (not vidmem). Interesting but probably wrong fit for this use case.

What it is: Encodes memories into a single .mv2 file using a video-codec-inspired append-only frame structure. Claims +35% over SOTA on LoCoMo, 0.025ms P50 retrieval, 1,372× higher throughput than vector DBs. Single file, no server, offline-capable.

Why it's tempting: The "single portable file per repo" concept is exactly your spec. .cortex/memory.mv2 committed to the repo.

Why it's probably wrong: It's a retrieval/storage layer, not a cognitive layer. No consolidation, no sleep, no reasoning over memories. It would replace SQLite as the storage backend but doesn't solve the self-improvement problem. Also it's a commercial product now (free tier at 50MB) — the open source version is the older v1. For a tiny self-learning tool, SQLite + FTS5 is simpler and you control it entirely.

Where it could fit: If you wanted semantic search without running embeddings locally, Memvid's built-in embedding + retrieval is lighter than sqlite-vec + a local model. But FTS5 is good enough for repo-scoped knowledge.

OpenCode Plugin — This is the best integration target
OpenCode's plugin system is more capable than Claude Code hooks for this use case. Here's why:

typescript
// .opencode/plugins/cortex.ts
import type { Plugin } from "@opencode-ai/plugin"
import { tool } from "@opencode-ai/plugin"

let saveCount = 0

export const CortexPlugin: Plugin = async ({ client, $, directory }) => {
  return {
    // Custom tools the LLM can call directly
    tool: {
      cortex_save: tool({
        description: "Save a learning, decision, or pattern to project memory",
        args: {
          content: tool.schema.string().describe("What was learned"),
          type: tool.schema.string().optional().describe("bugfix|decision|pattern|preference"),
        },
        async execute(args, ctx) {
          await $`cortex save "${args.content}" --type ${args.type || "observation"}`
          saveCount++
          if (saveCount >= 10) {
            await $`cortex sleep --micro`  // inline consolidation
            saveCount = 0
          }
          return "Saved to project memory"
        },
      }),

      cortex_recall: tool({
        description: "Search project memory for relevant learnings",
        args: {
          query: tool.schema.string(),
        },
        async execute(args) {
          const result = await $`cortex recall "${args.query}" --json`
          return result.stdout
        },
      }),
    },

    // Hook: inject context at session start
    event: async ({ event }) => {
      if (event.type === "session.created") {
        // Run catch-up consolidation, inject recent context
        await $`cortex wake --json`
        const context = await $`cortex context --json`
        // Inject learned context without triggering a response
        await client.session.prompt({
          path: { id: (event as any).session_id },
          body: {
            noReply: true,  // <-- OpenCode-specific: injects context silently
            parts: [{ type: "text", text: `Project memory context:\\n${context.stdout}` }],
          },
        })
      }
    },

    // Hook: consolidate on session idle
    "session.idle": async (input, output) => {
      await $`cortex sleep --quick`
    },

    // Hook: intercept compaction to preserve memory
    "experimental.session.compacting": async (input, output) => {
      // Before compaction, save current session learnings
      await $`cortex sleep --quick`
      // Append memory context to the compaction prompt
      const context = await $`cortex context --compact`
      output.prompt = input.prompt + `\\n\\nProject memory (preserved across compaction):\\n${context.stdout}`
    },

    // Hook: after tool execution, auto-detect learnings
    "tool.execute.after": async (input) => {
      // If agent just fixed a bug via edit, prompt it to save the learning
      if (input.tool === "edit" && saveCount === 0) {
        // The agent will naturally call cortex_save if instructed in AGENTS.md
      }
    },
  }
}
Key OpenCode advantages over Claude Code hooks:

noReply prompt injection — silently inject memory context without burning a turn
session.idle event — perfect sleep trigger, fires when agent finishes responding
experimental.session.compacting — intercept compaction to preserve memory across context resets
Custom tools registered inline — no MCP server needed, the tools are native
$ shell helper — run cortex CLI directly
tool.execute.after — react to specific tool usage patterns
Pi Agent Extension — Also excellent, different philosophy
Pi's extension system is the most powerful of the three. Key differences:

typescript
// ~/.pi/agent/extensions/cortex/index.ts
import type { ExtensionAPI } from "@mariozechner/pi-coding-agent"
import { Type } from "@sinclair/typebox"

export default function (pi: ExtensionAPI) {

  // Inject memory context before every turn
  pi.on("before_turn", async (event, ctx) => {
    const context = await execCortex("context --compact")
    // Modify messages before they reach the LLM
    return { 
      prependSystemMessage: `Project memory:\\n${context}` 
    }
  })

  // Auto-save after tool results
  pi.on("tool_result", async (event, ctx) => {
    if (event.toolName === "bash" && event.output?.includes("error")) {
      // Widget in the TUI footer showing memory status
      ctx.ui.setStatus("cortex", "🧠 Error detected — will prompt to save learning")
    }
  })

  // Session end → sleep
  pi.on("session_end", async (event, ctx) => {
    ctx.ui.setStatus("cortex", "💤 Consolidating memories...")
    await execCortex("sleep --quick")
    ctx.ui.notify("Memory consolidated", "success")
  })

  // Register cortex tools natively (no MCP!)
  pi.registerTool({
    name: "cortex_save",
    label: "Save Learning",
    description: "Save a learning to project memory",
    parameters: Type.Object({
      content: Type.String({ description: "What was learned" }),
      type: Type.Optional(Type.StringEnum(["bugfix", "decision", "pattern", "preference"])),
    }),
    async execute(toolCallId, params, signal, onUpdate, ctx) {
      const result = await execCortex(`save "${params.content}" --type ${params.type || "observation"}`)
      return { content: [{ type: "text", text: result }], details: {} }
    },
  })

  // Slash command for manual dream
  pi.registerCommand("dream", {
    description: "Deep reflection on project memories",
    handler: async (args, ctx) => {
      ctx.ui.setStatus("cortex", "🌙 Deep dreaming...")
      await execCortex("dream")
      ctx.ui.notify("Dream complete — skills updated", "success")
    },
  })

  // TUI widget showing memory stats
  pi.on("session_start", async (event, ctx) => {
    const stats = JSON.parse(await execCortex("stats --json"))
    ctx.ui.setWidget("cortex", [
      `🧠 ${stats.total} memories | ${stats.consolidated} consolidated`,
      `📚 ${stats.skills} skill files | Last sleep: ${stats.lastSleep}`,
    ])
  })
}
Pi-specific advantages:

before_turn — inject context before every LLM call, not just session start. This is the RAG-like behavior that makes recall actually work.
ctx.ui.setWidget / ctx.ui.setStatus — live TUI widgets showing memory health in the footer. Very cool for observability.
tool_result event — react to what the agent did, not just what it was asked. Can auto-detect error patterns.
No MCP needed — Pi's philosophy is "CLI tools with READMEs" which aligns perfectly with cortex being a simple binary.
StringEnum from typebox — proper typed tool params.
RPC mode — could embed cortex directly via Pi's SDK if you wanted to skip the CLI layer.
Comparison: Where to build first
Claude Code	OpenCode	Pi
Hook granularity	Session lifecycle only	Session + tool + compaction	Turn-level + tool + UI
Silent context injection	❌ (must use SystemStart)	✅ noReply	✅ before_turn
Custom tools without MCP	❌	✅	✅
TUI observability	❌	❌	✅ widgets/status
Compaction interception	❌	✅	✅ (extensions)
Distribution	hooks.json + shell scripts	npm package	pi package (npm/git)
Language	Shell scripts	TypeScript/JS	TypeScript/JS
My recommendation: Build cortex as a standalone Go binary (the brain), then write thin adapter plugins for all three. The OpenCode plugin is your best first target — richest hook system, growing community, and the noReply + compaction hooks are exactly what sleep/wake needs. Pi extension second (best DX, TUI widgets are a nice demo). Claude Code hooks last (most limited, but biggest install base).

The cortex binary itself stays agent-agnostic. The plugins are ~50 lines each.

URLs:

https://github.com/memvid/memvid
https://memvid.com
https://opencode.ai/docs/plugins/
https://opencode.ai/docs/custom-tools/
https://opencode.ai/docs/sdk/
https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/extensions.md
https://shittycodingagent.ai
https://github.com/badlogic/pi-skills
https://github.com/Dicklesworthstone/pi_agent_rust
Turn on web search in Search and tools menu. Otherwise, links provided may not be accurate or up to date.


CopyCopy
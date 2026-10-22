/**
 * pi-cortex: Cortex memory for pi.
 *
 * The agent saves what it learns with `cortex_save` and searches it with `cortex_recall`. Each
 * prompt gets the consolidated memory plus the memories that match the message. Cortex
 * consolidates (sleeps) when pi compacts, when enough new memories pile up, and dreams once a
 * night, so a long-lived session such as pi-durable keeps what compaction summaries drop.
 *
 * Settings (environment):
 *   CORTEX_BIN             cortex binary (default: ../target/release/cortex in a cortex checkout,
 *                          else `cortex` on PATH)
 *   CORTEX_PI_DIR          folder holding the agent's `.cortex/` (default: the session's cwd)
 *   CORTEX_PI_SLEEP_AFTER  new memories that trigger a sleep when a turn ends (default 15, 0 = never)
 *   CORTEX_PI_DREAM_HOUR   local hour after which the nightly dream runs (default 3, -1 = never)
 *   CORTEX_PI_MODEL        "provider/id" of the pi model cortex uses (default: a Haiku on the
 *                          session's provider, else the session model)
 *
 * Model access comes from pi: the chosen model's key, base URL and headers (a gateway's
 * workspace header, say) go to cortex for each run, so cortex needs no setup of its own.
 * CORTEX_PI_INHERIT=0 stops that; cortex then uses ANTHROPIC_API_KEY or AWS credentials from
 * the environment. Subscription (OAuth) logins aren't passed on; without other credentials
 * cortex falls back to micro sleeps.
 */

import fs from "node:fs";
import path from "node:path";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import { Cortex, dreamDue, inferenceEnv, localDate, MEMORY_TYPES, memoryBlock, pickInferenceModel, recallQuery } from "./cortex.ts";

const SLEEP_AFTER = Number(process.env.CORTEX_PI_SLEEP_AFTER ?? 15);
const DREAM_HOUR = Number(process.env.CORTEX_PI_DREAM_HOUR ?? 3);

export default function piCortex(pi: ExtensionAPI) {
	let cortex: Cortex | undefined;
	let context = "";
	let sleeping: Promise<unknown> | undefined;
	let dreamTimer: ReturnType<typeof setInterval> | undefined;

	const open = (ctx: ExtensionContext): Cortex => {
		cortex ??= new Cortex({ bin: process.env.CORTEX_BIN, dir: process.env.CORTEX_PI_DIR || ctx.cwd });
		return cortex;
	};
	/** Hands cortex the session's model access, fresh each time (tokens rotate). */
	const inherit = async (ctx: ExtensionContext) => {
		const store = open(ctx);
		if (process.env.CORTEX_PI_INHERIT === "0" || !ctx.modelRegistry) return;
		try {
			const model = pickInferenceModel(ctx.modelRegistry.getAvailable() as never, ctx.model as never, process.env.CORTEX_PI_MODEL);
			if (!model || ctx.modelRegistry.isUsingOAuth(model as never)) return store.setInference({});
			store.setInference(inferenceEnv(model, (await ctx.modelRegistry.getApiKeyAndHeaders(model as never)) as never));
		} catch (error) {
			ctx.ui.notify(`cortex couldn't use pi's model access: ${error instanceof Error ? error.message : error}`, "warning");
		}
	};
	const refresh = async () => {
		if (cortex) context = await cortex.context();
	};
	const warn = (ctx: ExtensionContext, what: string, stderr: string) => {
		const line = stderr.trim().split("\n").at(-1) ?? "";
		ctx.ui.notify(`cortex ${what} failed${line ? `: ${line}` : ""}`, "warning");
	};

	/** One consolidation at a time; LLM-free when `micro`. */
	const sleep = (ctx: ExtensionContext, micro = false) => {
		const store = open(ctx);
		sleeping ??= (micro ? Promise.resolve() : inherit(ctx))
			.then(() => store.sleep(micro))
			.then((result) => (result.ok ? refresh() : warn(ctx, "sleep", result.stderr)))
			.finally(() => (sleeping = undefined));
		return sleeping;
	};

	const dreamStateFile = () => path.join(cortex!.dir, ".cortex", "pi-cortex.json");
	const maybeDream = async (ctx: ExtensionContext) => {
		if (DREAM_HOUR < 0 || !cortex) return;
		let state: { lastDream?: string } = {};
		try {
			state = JSON.parse(fs.readFileSync(dreamStateFile(), "utf8"));
		} catch {}
		const now = new Date();
		if (!dreamDue(now, state.lastDream, DREAM_HOUR)) return;
		// Recorded first, so a failing dream isn't retried every hour.
		fs.writeFileSync(dreamStateFile(), JSON.stringify({ ...state, lastDream: localDate(now) }));
		await sleep(ctx);
		await inherit(ctx);
		const result = await cortex.dream();
		if (result.ok) await refresh();
		else warn(ctx, "dream", result.stderr);
	};

	pi.on("session_start", async (_event, ctx) => {
		const store = open(ctx);
		const init = await store.init();
		if (init && !init.ok) return warn(ctx, "init", init.stderr);
		await refresh();
		// Catch up on memories an earlier run saved but never consolidated; don't hold up the start.
		void inherit(ctx).then(() => store.wake()).then(refresh);
		dreamTimer ??= setInterval(() => void maybeDream(ctx), 60 * 60_000);
		dreamTimer.unref?.();
	});

	pi.on("before_agent_start", async (event, ctx) => {
		const store = open(ctx);
		if (!store.initialized) return;
		const recalled = await store.recall(recallQuery(event.prompt ?? ""), 5);
		return { systemPrompt: `${event.systemPrompt}\n\n${memoryBlock(context, recalled)}` };
	});

	pi.on("agent_end", async (_event, ctx) => {
		if (!cortex?.initialized || SLEEP_AFTER <= 0) return;
		const stats = await cortex.stats();
		if (Number(stats?.unconsolidated_count ?? 0) >= SLEEP_AFTER) void sleep(ctx);
	});

	// What compaction summarizes away is still in Cortex; consolidating now keeps it findable.
	pi.on("session_compact", async (_event, ctx) => {
		if (cortex?.initialized) void sleep(ctx);
	});

	pi.on("session_shutdown", async (_event, ctx) => {
		if (dreamTimer) clearInterval(dreamTimer);
		dreamTimer = undefined;
		if (cortex?.initialized) await (sleeping ?? sleep(ctx, true));
	});

	pi.registerTool({
		name: "cortex_save",
		label: "Remember",
		description:
			"Save something worth remembering to long-term memory (Cortex): a decision and why, a preference, a bug fix and its cause, a pattern, or a durable fact. One fact per call.",
		promptSnippet: "Save a fact, decision, preference or fix to long-term memory",
		parameters: Type.Object({
			content: Type.String({ description: "The memory, written to make sense on its own later." }),
			// A plain string enum (what pi-ai's StringEnum makes), which every provider accepts.
			type: Type.Unsafe<(typeof MEMORY_TYPES)[number]>({ type: "string", enum: [...MEMORY_TYPES], description: "bugfix, decision, pattern, preference or observation" }),
		}),
		async execute(_id, params, _signal, _onUpdate, ctx) {
			const store = open(ctx);
			await store.init();
			const result = await store.save(params.content, params.type);
			const message = (result.stderr.match(/Saved memory #\d+[^\n]*/) ?? [result.ok ? "Saved." : ""])[0];
			if (!result.ok) return { content: [{ type: "text", text: `Couldn't save: ${result.stderr.trim()}` }], details: {}, isError: true };
			void refresh();
			return { content: [{ type: "text", text: message }], details: { type: params.type } };
		},
	});

	pi.registerTool({
		name: "cortex_recall",
		label: "Recall",
		description: "Search long-term memory (Cortex), this agent's and the global store, for what you learned before.",
		promptSnippet: "Search long-term memory",
		parameters: Type.Object({
			query: Type.String({ description: "Words to look for: names, tools, topics." }),
			limit: Type.Optional(Type.Number({ description: "Most results (default 10)." })),
		}),
		async execute(_id, params, _signal, _onUpdate, ctx) {
			const memories = await open(ctx).recall(params.query, Math.min(Math.max(params.limit ?? 10, 1), 50));
			const text = memories.length ? memories.map((m) => `[${m.type}] #${m.id}: ${m.content}`).join("\n") : "No memories found.";
			return { content: [{ type: "text", text }], details: { count: memories.length } };
		},
	});
}

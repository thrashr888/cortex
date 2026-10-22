// The cortex CLI, as pi-cortex uses it. No pi imports here, so it runs under plain node for tests.

import { execFile } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const MEMORY_TYPES = ["bugfix", "decision", "pattern", "preference", "observation"] as const;
export type MemoryType = (typeof MEMORY_TYPES)[number];

export interface Memory {
	id: number;
	content: string;
	type: string;
}

export interface RunResult {
	ok: boolean;
	stdout: string;
	stderr: string;
}

export interface CortexOptions {
	/** The cortex binary (CORTEX_BIN), default `cortex` on PATH. */
	bin?: string;
	/** The folder whose `.cortex/` holds this agent's memory. */
	dir: string;
	env?: NodeJS.ProcessEnv;
	timeoutMs?: number;
}

/**
 * One agent's Cortex store. Commands that write (init, save, wake, sleep, dream) run one at a
 * time, so a save never races a consolidation over SQLite; reads run straight away.
 */
export class Cortex {
	readonly bin: string;
	readonly dir: string;
	#base: NodeJS.ProcessEnv;
	#env: NodeJS.ProcessEnv;
	#timeoutMs: number;
	#writes: Promise<unknown> = Promise.resolve();

	constructor(options: CortexOptions) {
		this.bin = options.bin || findBinary();
		this.dir = options.dir;
		// Long-lived agents save often: leave entity extraction to sleep's one batched call,
		// unless the user chose otherwise.
		this.#base = { CORTEX_EXTRACT_ON_SAVE: "0", ...(options.env ?? process.env) };
		this.#env = this.#base;
		this.#timeoutMs = options.timeoutMs ?? 180_000;
	}

	/** Model access for cortex's LLM calls (sleep, dream), replacing what was set before. */
	setInference(env: Record<string, string>): void {
		// pi's access replaces any ANTHROPIC_* the host was started with (that key may need pi's
		// base URL or headers); a CORTEX_MODEL the user set still picks the model.
		const own = this.#base.CORTEX_MODEL ? { CORTEX_MODEL: this.#base.CORTEX_MODEL } : {};
		this.#env = { ...this.#base, ...env, ...own };
	}

	get initialized(): boolean {
		return fs.existsSync(path.join(this.dir, ".cortex"));
	}

	/** Runs `cortex --dir <dir> ...args`; never throws. */
	run(args: string[]): Promise<RunResult> {
		return new Promise((resolve) => {
			execFile(
				this.bin,
				["--dir", this.dir, ...args],
				{ env: this.#env, timeout: this.#timeoutMs, maxBuffer: 8 * 1024 * 1024 },
				(error, stdout, stderr) => resolve({ ok: !error, stdout: String(stdout ?? ""), stderr: String(stderr ?? error?.message ?? "") }),
			);
		});
	}

	/** Queues a writing command behind the others. */
	write(args: string[]): Promise<RunResult> {
		const next = this.#writes.then(() => this.run(args));
		this.#writes = next.catch(() => undefined);
		return next;
	}

	async init(): Promise<RunResult | undefined> {
		if (this.initialized) return undefined;
		fs.mkdirSync(this.dir, { recursive: true });
		return this.write(["init"]);
	}

	wake(): Promise<RunResult> {
		return this.write(["wake"]);
	}

	/** The memory context for the system prompt: entities, learned patterns, global knowledge. */
	async context(limit = 15): Promise<string> {
		const result = await this.run(["context", "--limit", String(limit)]);
		return result.ok ? result.stdout.trim() : "";
	}

	async recall(query: string, limit = 5): Promise<Memory[]> {
		if (!query.trim()) return [];
		const result = await this.run(["recall", query, "--limit", String(limit), "--json"]);
		if (!result.ok || !result.stdout.trim()) return [];
		try {
			const parsed = JSON.parse(result.stdout);
			return Array.isArray(parsed) ? parsed.map((m) => ({ id: Number(m.id), content: String(m.content ?? ""), type: String(m.type ?? "observation") })) : [];
		} catch {
			return [];
		}
	}

	save(content: string, type: MemoryType): Promise<RunResult> {
		return this.write(["save", content, "--type", type]);
	}

	async stats(): Promise<Record<string, unknown> | null> {
		const result = await this.run(["stats", "--json"]);
		try {
			return result.ok ? JSON.parse(result.stdout) : null;
		} catch {
			return null;
		}
	}

	sleep(micro = false): Promise<RunResult> {
		return this.write(micro ? ["sleep", "--micro"] : ["sleep"]);
	}

	dream(): Promise<RunResult> {
		return this.write(["dream"]);
	}
}

/**
 * cortex next to this package when it runs from a cortex checkout (pi-bundle builds it there),
 * else `cortex` on PATH.
 */
export function findBinary(): string {
	const built = fileURLToPath(new URL("../target/release/cortex", import.meta.url));
	return fs.existsSync(built) ? built : "cortex";
}

/** The variables cortex reads for model access; pi-cortex sets them from pi's own login. */
export const INFERENCE_KEYS = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_BASE_URL", "CORTEX_HEADERS", "CORTEX_MODEL"];

export interface InferenceModel {
	provider: string;
	id: string;
	api: string;
	baseUrl?: string;
	headers?: Record<string, string>;
}

export interface InferenceAuth {
	ok: boolean;
	apiKey?: string;
	headers?: Record<string, string>;
	baseUrl?: string;
}

/**
 * Picks the model cortex should use from pi's available models: `CORTEX_PI_MODEL`
 * ("provider/id") when set, else a Haiku on the session model's provider, else the session
 * model, else any Haiku. Cortex speaks the Anthropic Messages API, so only those models count.
 */
export function pickInferenceModel(available: InferenceModel[], current: InferenceModel | undefined, wanted?: string): InferenceModel | undefined {
	const usable = available.filter((m) => m.api === "anthropic-messages");
	if (wanted) {
		const [provider, ...rest] = wanted.split("/");
		return usable.find((m) => m.provider === provider && m.id === rest.join("/"));
	}
	const haiku = (m: InferenceModel) => /haiku/i.test(m.id);
	const newest = (models: InferenceModel[]) => [...models].sort((a, b) => b.id.localeCompare(a.id, undefined, { numeric: true }))[0];
	const onProvider = current ? usable.filter((m) => m.provider === current.provider) : [];
	return newest(onProvider.filter(haiku)) ?? (current?.api === "anthropic-messages" ? current : undefined) ?? newest(usable.filter(haiku)) ?? usable[0];
}

/** Environment for cortex from a pi model and its resolved auth. */
export function inferenceEnv(model: InferenceModel, auth: InferenceAuth): Record<string, string> {
	if (!auth.ok) return {};
	const env: Record<string, string> = { CORTEX_MODEL: model.id };
	const baseUrl = auth.baseUrl ?? model.baseUrl;
	if (baseUrl) env.ANTHROPIC_BASE_URL = baseUrl;
	if (auth.apiKey) env.ANTHROPIC_API_KEY = auth.apiKey;
	const headers = { ...(model.headers ?? {}), ...(auth.headers ?? {}) };
	if (Object.keys(headers).length) env.CORTEX_HEADERS = JSON.stringify(headers);
	return env.ANTHROPIC_API_KEY || env.CORTEX_HEADERS ? env : {};
}

const STOP_WORDS = new Set(
	"a an and are as at be but by can could do does for from had has have how i if in into is it its me my of on or our please should so that the their them then there these this to us was we were what when where which who why will with would you your".split(
		" ",
	),
);

/** Search words from a message: the distinctive ones, at most `max`, from its start. */
export function recallQuery(text: string, max = 12): string {
	const words: string[] = [];
	for (const word of text.slice(0, 2000).toLowerCase().match(/[\p{L}\p{N}][\p{L}\p{N}_.-]*/gu) ?? []) {
		const clean = word.replace(/[.-]+$/, "");
		if (clean.length < 3 || STOP_WORDS.has(clean) || words.includes(clean)) continue;
		words.push(clean);
		if (words.length >= max) break;
	}
	return words.join(" ");
}

const GUIDE = `You have long-term memory (Cortex) that outlives this conversation and its compactions.
- Save with cortex_save when you learn something worth keeping: a decision and its reason, a preference of the people you work with, a fix and its cause, a recurring pattern, a fact about a system you'll need again. One fact per save, written to make sense on its own later.
- Don't save secrets, keys or passwords, or things only this turn needs.
- Search with cortex_recall before re-asking something you might already know.
- Memories consolidate on their own when the conversation compacts and overnight.`;

/** The block pi-cortex adds to the system prompt. */
export function memoryBlock(context: string, recalled: Memory[]): string {
	const parts = [GUIDE];
	if (context.trim()) parts.push(context.trim());
	if (recalled.length) {
		parts.push(`### Possibly relevant to this message\n${recalled.map((m) => `- [${m.type}] ${m.content}`).join("\n")}`);
	}
	return `<memory>\n${parts.join("\n\n")}\n</memory>`;
}

/** Whether tonight's dream is due: past `hour` local time and none yet today. */
export function dreamDue(now: Date, lastDream: string | undefined, hour: number): boolean {
	if (now.getHours() < hour) return false;
	return lastDream !== localDate(now);
}

export function localDate(date: Date): string {
	const pad = (n: number) => String(n).padStart(2, "0");
	return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

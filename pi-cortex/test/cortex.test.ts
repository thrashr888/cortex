// Runs against a real cortex binary: CORTEX_BIN, or the repo's target/debug build.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Cortex, dreamDue, memoryBlock, recallQuery } from "../cortex.ts";

const bin = process.env.CORTEX_BIN ?? fileURLToPath(new URL("../../target/debug/cortex", import.meta.url));
const haveBin = fs.existsSync(bin);

function store() {
	const home = fs.mkdtempSync(path.join(os.tmpdir(), "pi-cortex-"));
	const dir = path.join(home, "agents", "flint");
	// No LLM credentials: sleeps fall back to micro, saves never call out.
	const env = { PATH: process.env.PATH, HOME: home, AWS_SHARED_CREDENTIALS_FILE: path.join(home, "none") };
	return new Cortex({ bin, dir, env });
}

test("recall queries keep the distinctive words", () => {
	assert.equal(recallQuery("What does Paul prefer for npm in the tavern repo?"), "paul prefer npm tavern repo");
	assert.equal(recallQuery("deploy deploy DEPLOY"), "deploy");
	assert.equal(recallQuery("a b c"), "");
	assert.equal(recallQuery("one two three four", 2), "one two");
});

test("the memory block has the guide, the context and what matched", () => {
	const block = memoryBlock("## Project Memory Context\n- [preference] pnpm", [{ id: 1, content: "Tavern uses port 19011", type: "observation" }]);
	assert.match(block, /^<memory>\nYou have long-term memory/);
	assert.match(block, /## Project Memory Context/);
	assert.match(block, /### Possibly relevant to this message\n- \[observation\] Tavern uses port 19011\n<\/memory>$/);
	assert.doesNotMatch(memoryBlock("", []), /Possibly relevant/);
});

test("the dream is due once a day, after the hour", () => {
	const at = (h: number) => new Date(2026, 9, 22, h, 30);
	assert.equal(dreamDue(at(2), undefined, 3), false);
	assert.equal(dreamDue(at(4), undefined, 3), true);
	assert.equal(dreamDue(at(4), "2026-10-22", 3), false);
	assert.equal(dreamDue(at(4), "2026-10-21", 3), true);
});

test("init, save, recall, context, stats and sleep against cortex", { skip: !haveBin && "no cortex binary" }, async () => {
	const cortex = store();
	assert.equal(cortex.initialized, false);
	assert.equal((await cortex.init())?.ok, true);
	assert.equal(cortex.initialized, true);
	assert.equal(await cortex.init(), undefined);

	// Saves queue behind each other.
	const saves = await Promise.all([
		cortex.save("Paul prefers pnpm over npm in JavaScript repos", "preference"),
		cortex.save("The Tavern durable host listens on port 19011", "observation"),
	]);
	assert.ok(saves.every((r) => r.ok));
	assert.match(saves[0].stderr, /Saved memory #1/);

	const found = await cortex.recall(recallQuery("which package manager does Paul prefer, pnpm?"));
	assert.deepEqual(found.map((m) => [m.id, m.type]), [[1, "preference"]]);
	assert.deepEqual(await cortex.recall("zebra"), []);
	assert.deepEqual(await cortex.recall(""), []);

	assert.match(await cortex.context(), /2 total memories/);
	assert.equal((await cortex.stats())?.unconsolidated_count, 2);
	assert.equal((await cortex.sleep(true)).ok, true);
});

test("a missing binary fails softly", async () => {
	const cortex = new Cortex({ bin: "/nonexistent/cortex", dir: os.tmpdir() });
	const result = await cortex.run(["stats"]);
	assert.equal(result.ok, false);
	assert.deepEqual(await cortex.recall("x"), []);
	assert.equal(await cortex.context(), "");
	assert.equal(await cortex.stats(), null);
});

test("cortex uses a Haiku on the session's provider, else the session model", async () => {
	const { pickInferenceModel, inferenceEnv } = await import("../cortex.ts");
	const m = (provider: string, id: string, api = "anthropic-messages") => ({ provider, id, api });
	const available = [m("anthropic", "claude-opus-5-5"), m("anthropic", "claude-haiku-4-5"), m("anthropic", "claude-haiku-5-5"), m("gateway", "claude-sonnet-5-5"), m("openai", "gpt-5", "openai-responses")];
	assert.equal(pickInferenceModel(available, m("anthropic", "claude-opus-5-5"))?.id, "claude-haiku-5-5");
	assert.deepEqual(pickInferenceModel(available, m("gateway", "claude-sonnet-5-5")), m("gateway", "claude-sonnet-5-5"));
	assert.equal(pickInferenceModel(available, m("openai", "gpt-5", "openai-responses"))?.id, "claude-haiku-5-5");
	assert.equal(pickInferenceModel(available, undefined, "anthropic/claude-haiku-4-5")?.id, "claude-haiku-4-5");
	assert.equal(pickInferenceModel(available, undefined, "openai/gpt-5"), undefined);

	const gateway = { provider: "gateway", id: "claude-sonnet-5-5", api: "anthropic-messages", baseUrl: "https://gw.example/anthropic", headers: { "x-workspace": "ember" } };
	assert.deepEqual(inferenceEnv(gateway, { ok: true, apiKey: "sk-1", headers: { "x-trace": "1" } }), {
		CORTEX_MODEL: "claude-sonnet-5-5",
		ANTHROPIC_BASE_URL: "https://gw.example/anthropic",
		ANTHROPIC_API_KEY: "sk-1",
		CORTEX_HEADERS: JSON.stringify({ "x-workspace": "ember", "x-trace": "1" }),
	});
	assert.deepEqual(inferenceEnv(gateway, { ok: false }), {});
	assert.deepEqual(inferenceEnv(m("anthropic", "claude-haiku-5-5"), { ok: true }), {});
});

test("pi's model access reaches cortex's API calls, and the user's CORTEX_MODEL wins", { skip: !haveBin && "no cortex binary" }, async () => {
	const http = await import("node:http");
	const seen: { url?: string; headers: Record<string, unknown>; model?: string }[] = [];
	const server = http.createServer((req, res) => {
		let body = "";
		req.on("data", (c) => (body += c));
		req.on("end", () => {
			seen.push({ url: req.url, headers: req.headers, model: JSON.parse(body).model });
			res.setHeader("content-type", "application/json");
			res.end(JSON.stringify({ content: [{ type: "text", text: '{"memories": []}' }] }));
		});
	});
	await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
	const port = (server.address() as { port: number }).port;
	try {
		const cortex = store();
		await cortex.init();
		await cortex.save("Ember reaches models through the gateway", "observation");
		cortex.setInference({ CORTEX_MODEL: "claude-haiku-5-5", ANTHROPIC_BASE_URL: `http://127.0.0.1:${port}/v1`, ANTHROPIC_API_KEY: "sk-test", CORTEX_HEADERS: JSON.stringify({ "x-workspace": "ember" }) });
		await cortex.sleep();
		assert.ok(seen.length >= 1);
		assert.equal(seen[0].url, "/v1/messages");
		assert.equal(seen[0].headers["x-api-key"], "sk-test");
		assert.equal(seen[0].headers["x-workspace"], "ember");
		assert.equal(seen[0].model, "claude-haiku-5-5");

		// A model the user chose stays, whatever pi offers.
		const home = fs.mkdtempSync(path.join(os.tmpdir(), "pi-cortex-"));
		// pi's key replaces one the host was started with.
		const own = new Cortex({ bin, dir: path.join(home, "agent"), env: { PATH: process.env.PATH, HOME: home, CORTEX_MODEL: "claude-sonnet-5-5", ANTHROPIC_API_KEY: "sk-host" } });
		await own.init();
		await own.save("Tally runs on the SER8", "observation");
		own.setInference({ CORTEX_MODEL: "claude-haiku-5-5", ANTHROPIC_BASE_URL: `http://127.0.0.1:${port}`, ANTHROPIC_API_KEY: "sk-test" });
		await own.sleep();
		assert.equal(seen.at(-1)?.model, "claude-sonnet-5-5");
		assert.equal(seen.at(-1)?.headers["x-api-key"], "sk-test");
	} finally {
		server.close();
	}
});

import { readdir, readFile } from "node:fs/promises";
import path, { resolve } from "node:path";
import { pathToFileURL } from "node:url";

export function verifyCoverage(summaries) {
  const globals = summaries.filter((summary) => summary.mode === "global");
  const shards = summaries.filter((summary) => summary.mode === "shard");
  if (globals.length !== 1 || !globals[0].global_checks || !globals[0].rollback_race) {
    throw new Error("expected exactly one global summary with interactive and rollback checks");
  }
  const shardTotals = new Set(shards.map((shard) => shard.shard?.split("/")[1]));
  if (shardTotals.size !== 1 || !/^\d+$/.test([...shardTotals][0] ?? "")) {
    throw new Error("Storybook shard summaries must declare one common total");
  }
  const total = Number([...shardTotals][0]);
  if (shards.length !== total || total < 2 || total > 3) {
    throw new Error(`expected ${total} shard summaries, got ${shards.length}`);
  }
  const indexes = new Set(shards.map((shard) => shard.shard?.split("/")[0]));
  for (let index = 1; index <= total; index += 1) {
    if (!indexes.has(String(index))) throw new Error(`missing Storybook shard ${index}/${total}`);
  }

  const baseline = JSON.stringify([...globals[0].baseline_story_ids].sort());
  const selected = new Map();
  for (const shard of shards) {
    if (JSON.stringify([...shard.baseline_story_ids].sort()) !== baseline) {
      throw new Error("Storybook shard baseline lists differ");
    }
    for (const storyId of shard.selected_story_ids) {
      if (selected.has(storyId)) throw new Error(`Storybook story assigned more than once: ${storyId}`);
      selected.set(storyId, true);
    }
  }
  const expected = JSON.parse(baseline);
  if (JSON.stringify([...selected.keys()].sort()) !== JSON.stringify(expected)) {
    throw new Error(`Storybook shard union differs from baseline (${selected.size}/${expected.length})`);
  }
  return { storyCount: expected.length, total };
}

async function main() {
  const directory = process.argv[2];
  if (!directory) throw new Error("usage: verify-storybook-coverage.mjs <directory>");
  const files = (await readdir(directory)).filter((name) => name.endsWith(".json")).sort();
  if (files.length < 3 || files.length > 4) throw new Error(`expected one global plus two or three shard summaries, got ${files.length}`);
  const summaries = await Promise.all(
    files.map(async (name) => JSON.parse(await readFile(path.join(directory, name), "utf8"))),
  );
  const result = verifyCoverage(summaries);
  console.log(`PASS: Storybook coverage partition (${result.storyCount} stories, ${result.total} shards, 1 global)`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(error);
    process.exit(1);
  });
}

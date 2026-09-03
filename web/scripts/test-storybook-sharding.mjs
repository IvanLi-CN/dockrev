import assert from "node:assert/strict";
import { shardCoverage, selectSmokeShard } from "./storybook-sharding.mjs";
import { verifyCoverage } from "./verify-storybook-coverage.mjs";

const storyIds = Array.from({ length: 381 }, (_, index) => `story-${index}`);

for (const total of [2, 3]) {
  const shards = shardCoverage(storyIds, total);
  const counts = new Map();
  for (const shard of shards) {
    for (const storyId of shard) counts.set(storyId, (counts.get(storyId) ?? 0) + 1);
  }
  assert.equal(shards.length, total);
  assert.equal(counts.size, storyIds.length);
  assert.deepEqual([...counts.values()], Array(storyIds.length).fill(1));
  assert.deepEqual(shards.flat().sort(), [...storyIds].sort());
}

assert.throws(() => selectSmokeShard(storyIds, "0/3"), /one-based index/);
assert.throws(() => selectSmokeShard(storyIds, "4/3"), /one-based index/);
assert.equal(selectSmokeShard(storyIds, "1/1").length, storyIds.length);

for (const total of [2, 3]) {
  const shards = shardCoverage(storyIds, total);
  const summaries = [
    {
      mode: "global",
      baseline_story_ids: storyIds,
      selected_story_ids: [],
      global_checks: true,
      rollback_race: true,
    },
    ...shards.map((selectedStoryIds, index) => ({
      mode: "shard",
      shard: `${index + 1}/${total}`,
      baseline_story_ids: storyIds,
      selected_story_ids: selectedStoryIds,
      global_checks: false,
      rollback_race: false,
    })),
  ];
  assert.deepEqual(verifyCoverage(summaries), { storyCount: storyIds.length, total });
}

console.log("PASS: Storybook sharding coverage");

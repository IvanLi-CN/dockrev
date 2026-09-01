export function parseStorybookShard(raw) {
  if (!raw) return null;
  const match = /^(\d+)\/(\d+)$/.exec(raw);
  if (!match) {
    throw new Error(
      "DOCKREV_TEST_STORYBOOK_SHARD must use the one-based index/total form, for example 1/4.",
    );
  }
  const index = Number(match[1]);
  const total = Number(match[2]);
  if (!Number.isInteger(index) || !Number.isInteger(total) || index < 1 || index > total) {
    throw new Error(
      "DOCKREV_TEST_STORYBOOK_SHARD must use a one-based index within its total, for example 1/4.",
    );
  }
  return { index, total };
}

export function selectSmokeShard(storyIds, rawShard = process.env.DOCKREV_TEST_STORYBOOK_SHARD) {
  const shard = parseStorybookShard(rawShard);
  if (!shard) return storyIds;
  return storyIds.filter((_, storyIndex) => storyIndex % shard.total === shard.index - 1);
}

export function shardCoverage(storyIds, total) {
  return Array.from({ length: total }, (_, index) =>
    selectSmokeShard(storyIds, `${index + 1}/${total}`),
  );
}

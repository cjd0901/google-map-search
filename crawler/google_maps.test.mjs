import assert from "node:assert/strict";
import test from "node:test";

import { parseRating, parseReviewCount } from "./google_maps.mjs";

test("parseRating reads Chinese and English labels", () => {
  assert.equal(parseRating("4.7 星，共 123 条评价"), 4.7);
  assert.equal(parseRating("4,6 stars 89 reviews"), 4.6);
  assert.equal(parseRating("暂无评分"), null);
});

test("parseReviewCount reads Chinese and English labels", () => {
  assert.equal(parseReviewCount("4.7 星，共 1,234 条评价"), 1234);
  assert.equal(parseReviewCount("4.6 stars 89 reviews"), 89);
  assert.equal(parseReviewCount("4.8 (2,345) 咖啡店"), 2345);
  assert.equal(parseReviewCount("没有评价"), null);
});

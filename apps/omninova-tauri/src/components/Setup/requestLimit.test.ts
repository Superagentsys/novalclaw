import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  applyRequestLimitInput,
  formatRequestLimitInput,
  parseRequestLimitInput,
  requestLimitError,
} from "./requestLimit.ts";

describe("R2.3.1 request output limit validation", () => {
  it("A. existing 32000 loads as 32000", () => {
    assert.equal(formatRequestLimitInput(32000), "32000");
  });

  it("B. valid 64000 parses as 64000", () => {
    const result = parseRequestLimitInput("64000", 384000);
    assert.equal(result.status, "valid");
    assert.equal(result.value, 64000);
  });

  it("C. blank input is valid unset", () => {
    const result = parseRequestLimitInput("", 384000);
    assert.equal(result.status, "blank");
    assert.equal(result.value, undefined);
  });

  it("D. clearing 32000 removes request_max_output_tokens", () => {
    const next = applyRequestLimitInput(32000, "", 384000);
    assert.equal(next, undefined);
  });

  it("E. abc is invalid", () => {
    const result = parseRequestLimitInput("abc", 384000);
    assert.equal(result.status, "invalid");
    assert.match(result.error ?? "", /正整数/);
  });

  it("F. -1 is invalid", () => {
    const result = parseRequestLimitInput("-1", 384000);
    assert.equal(result.status, "invalid");
  });

  it("G. 0 is invalid and does not silently become unset", () => {
    const result = parseRequestLimitInput("0", 384000);
    assert.equal(result.status, "invalid");
    const next = applyRequestLimitInput(32000, "0", 384000);
    assert.equal(next, 32000);
  });

  it("H. 1.5 is invalid", () => {
    const result = parseRequestLimitInput("1.5", 384000);
    assert.equal(result.status, "invalid");
  });

  it("I. above model max is invalid", () => {
    const result = parseRequestLimitInput("384001", 384000);
    assert.equal(result.status, "invalid");
    assert.match(result.error ?? "", /384K/);
  });

  it("J. exactly model max is valid", () => {
    const result = parseRequestLimitInput("384000", 384000);
    assert.equal(result.status, "valid");
    assert.equal(result.value, 384000);
  });

  it("K. unknown model max allows positive integer", () => {
    const result = parseRequestLimitInput("64000", null);
    assert.equal(result.status, "valid");
    assert.equal(result.value, 64000);
  });

  it("L. failed validation does not mutate existing saved config", () => {
    assert.equal(applyRequestLimitInput(32000, "abc", 384000), 32000);
    assert.equal(applyRequestLimitInput(32000, "1.5", 384000), 32000);
    assert.equal(applyRequestLimitInput(32000, "-1", 384000), 32000);
  });

  it("N. model max capability is not changed by request limit edit", () => {
    // This module never touches max_output_tokens; only the request value is returned.
    const next = applyRequestLimitInput(32000, "64000", 384000);
    assert.equal(next, 64000);
  });

  it("requestLimitError returns concise messages", () => {
    assert.equal(requestLimitError("abc", 384000), "请输入正整数 Token 数。");
    assert.equal(requestLimitError("0", 384000), "请输入大于 0 的 Token 数；留空表示使用 OmniNova 默认策略。");
    assert.equal(requestLimitError("384001", 384000), "不能超过模型最大输出上限 384K。");
    assert.equal(requestLimitError("", 384000), null);
  });
});

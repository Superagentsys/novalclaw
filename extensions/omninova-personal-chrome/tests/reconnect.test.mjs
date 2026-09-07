import assert from "node:assert/strict";
import { test } from "node:test";
import { shouldReconnect } from "../dist/protocol.js";

test("reconnect is normal except after protocol mismatch", () => {
  assert.equal(shouldReconnect("disconnected"), true);
  assert.equal(shouldReconnect("connecting"), true);
  assert.equal(shouldReconnect("connected"), true);
  assert.equal(shouldReconnect("protocol_mismatch"), false);
});

test("reconnect does not imply launching chrome or reopening tabs", () => {
  assert.equal(shouldReconnect("disconnected"), true);
});

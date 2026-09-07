import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  APPLICATION_MAX_MESSAGE_BYTES,
  PROTOCOL_VERSION,
  buildHello,
  buildPing,
  isProtocolMismatch,
  nativeHostName,
} from "../dist/protocol.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

test("handshake helpers round-trip request ids", () => {
  const hello = buildHello("req-hello", "0.1.0");
  assert.equal(hello.protocol_version, PROTOCOL_VERSION);
  assert.equal(hello.request_id, "req-hello");
  assert.equal(hello.operation, "hello");
  assert.equal(hello.payload.protocol_version, 1);
  const ping = buildPing("req-ping", "echo");
  assert.equal(ping.request_id, "req-ping");
  assert.equal(ping.payload.echo, "echo");
});

test("shared constants match the native host crate", () => {
  const extension = JSON.parse(readFileSync(join(root, "src/constants.json"), "utf8"));
  const rust = JSON.parse(
    readFileSync(
      join(root, "../../crates/omninova-browser-host/shared/constants.json"),
      "utf8"
    )
  );
  assert.deepEqual(extension, rust);
  assert.equal(nativeHostName(), rust.native_host_name);
  assert.equal(PROTOCOL_VERSION, rust.protocol_version);
  assert.equal(APPLICATION_MAX_MESSAGE_BYTES, rust.application_max_message_bytes);
});

test("protocol mismatch is detected without treating other errors as mismatch", () => {
  assert.equal(isProtocolMismatch({ ok: false, error: { code: "ProtocolMismatch" } }), true);
  assert.equal(isProtocolMismatch({ ok: false, error: { code: "UnknownOperation" } }), false);
});

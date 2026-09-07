import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";

const pickerSource = readFileSync(
  fileURLToPath(new URL("./ModelPicker.tsx", import.meta.url)),
  "utf8"
);
const pickerCss = readFileSync(
  fileURLToPath(new URL("./ModelPicker.css", import.meta.url)),
  "utf8"
);
const chatSource = readFileSync(
  fileURLToPath(new URL("./Chat.tsx", import.meta.url)),
  "utf8"
);

describe("composer dropdown popovers", () => {
  it("renders the model list through a document.body portal", () => {
    assert.match(pickerSource, /createPortal/);
    assert.match(pickerSource, /document\.body/);
    assert.match(pickerCss, /position:\s*fixed/);
  });

  it("renders approval and workspace menus through document.body portals", () => {
    assert.match(chatSource, /createPortal\(/);
    assert.match(chatSource, /className="chat-permission-menu"/);
    assert.match(chatSource, /className="chat-workspace-menu"/);
    assert.match(chatSource, /document\.body/);
  });
});

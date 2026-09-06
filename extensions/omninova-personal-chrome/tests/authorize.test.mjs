import assert from "node:assert/strict";
import { test } from "node:test";
import {
  attachSession,
  detachSession,
  getAuthorized,
  grantTestAuthorization,
  invalidateOnGenerationChange,
  listAuthorized,
  revokeAll,
} from "../dist/authorize.js";

test("authorized tab registry denies unauthorized ids", () => {
  revokeAll();
  const grant = grantTestAuthorization(1, 11);
  assert.equal(grant.tabId, 11);
  assert.ok(getAuthorized(11, grant.authorizationGeneration));
  assert.equal(getAuthorized(99), undefined);
  assert.equal(getAuthorized(11, grant.authorizationGeneration + 1), undefined);
  assert.equal(listAuthorized().length, 1);
  attachSession("pcs:test", 11);
  detachSession("pcs:test");
  revokeAll();
  assert.equal(listAuthorized().length, 0);
});

test("authorization generation invalidation clears grants", () => {
  revokeAll();
  grantTestAuthorization(1, 11);
  invalidateOnGenerationChange();
  assert.equal(getAuthorized(11), undefined);
});

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  attachSession,
  detachSession,
  getAuthorized,
  grantAuthorization,
  grantTestAuthorization,
  invalidateOnGenerationChange,
  listAuthorized,
  revokeAll,
  restoreAuthorizationGeneration,
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

test("a new explicit grant replaces the previous tab and bumps generation", () => {
  revokeAll();
  const first = grantAuthorization(1, 11);
  const second = grantAuthorization(2, 22);
  assert.ok(second.authorizationGeneration > first.authorizationGeneration);
  assert.equal(getAuthorized(11), undefined);
  assert.ok(getAuthorized(22, second.authorizationGeneration));
});

test("revoke increments generation so stale requests cannot continue", () => {
  revokeAll();
  const grant = grantAuthorization(1, 11);
  const generation = revokeAll();
  assert.ok(generation > grant.authorizationGeneration);
  assert.equal(getAuthorized(11, grant.authorizationGeneration), undefined);
});

test("service worker generation restore prevents stale grant reuse", () => {
  revokeAll();
  const previous = grantAuthorization(1, 11);
  restoreAuthorizationGeneration(previous.authorizationGeneration + 100);
  const next = grantAuthorization(1, 11);
  assert.ok(next.authorizationGeneration > previous.authorizationGeneration + 100);
  assert.equal(getAuthorized(11, previous.authorizationGeneration), undefined);
});

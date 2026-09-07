export interface AuthorizedTab {
  windowId: number;
  tabId: number;
  authorizationGeneration: number;
  originPermission?: string;
}

export interface AuthorizationSnapshot {
  generation: number;
  authorized: AuthorizedTab[];
}

const authorized = new Map<number, AuthorizedTab>();
let authGeneration = 0;
const attachedSessions = new Map<string, number>();

export function grantAuthorization(
  windowId: number,
  tabId: number,
  originPermission?: string
): AuthorizedTab {
  authGeneration += 1;
  authorized.clear();
  attachedSessions.clear();
  const grant: AuthorizedTab = {
    windowId,
    tabId,
    authorizationGeneration: authGeneration,
    originPermission,
  };
  authorized.set(tabId, grant);
  return grant;
}

/** Retained only for isolated protocol fixtures; production uses popup grants. */
export const grantTestAuthorization = grantAuthorization;

export function revokeAll(): number {
  authGeneration += 1;
  authorized.clear();
  attachedSessions.clear();
  return authGeneration;
}

export function revokeTab(tabId: number): number {
  if (authorized.has(tabId)) {
    authGeneration += 1;
  }
  authorized.delete(tabId);
  for (const [token, attachedTabId] of attachedSessions) {
    if (attachedTabId === tabId) attachedSessions.delete(token);
  }
  return authGeneration;
}

export function listAuthorized(): AuthorizedTab[] {
  return [...authorized.values()];
}

export function getAuthorized(
  tabId: number,
  generation?: number
): AuthorizedTab | undefined {
  const grant = authorized.get(tabId);
  if (!grant) {
    return undefined;
  }
  if (generation !== undefined && grant.authorizationGeneration !== generation) {
    return undefined;
  }
  return grant;
}

export function attachSession(token: string, tabId: number): void {
  attachedSessions.set(token, tabId);
}

export function detachSession(token?: string): void {
  if (token) {
    attachedSessions.delete(token);
    return;
  }
  attachedSessions.clear();
}

export function invalidateOnGenerationChange(): void {
  revokeAll();
}

export function authorizationSnapshot(): AuthorizationSnapshot {
  return { generation: authGeneration, authorized: listAuthorized() };
}

export function restoreAuthorizationGeneration(value: number): number {
  if (Number.isSafeInteger(value) && value >= 0) {
    authGeneration = Math.max(authGeneration, value);
  }
  return authGeneration;
}

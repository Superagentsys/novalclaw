export interface AuthorizedTab {
  windowId: number;
  tabId: number;
  authorizationGeneration: number;
}

const authorized = new Map<number, AuthorizedTab>();
let authGeneration = 0;
const attachedSessions = new Map<string, number>();

export function grantTestAuthorization(windowId: number, tabId: number): AuthorizedTab {
  authGeneration += 1;
  const grant: AuthorizedTab = {
    windowId,
    tabId,
    authorizationGeneration: authGeneration,
  };
  authorized.set(tabId, grant);
  return grant;
}

export function revokeAll(): void {
  authorized.clear();
  attachedSessions.clear();
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
  authorized.clear();
  attachedSessions.clear();
}

export const SETUP_CONFIG_UPDATED_EVENT = "omninova:setup-config-updated";

/**
 * Lets permanently mounted screens refresh configuration after Setup saves.
 * A browser event keeps this bridge lightweight and avoids introducing a
 * global state dependency just for cross-page cache invalidation.
 */
export function notifySetupConfigUpdated() {
  window.dispatchEvent(new Event(SETUP_CONFIG_UPDATED_EVENT));
}

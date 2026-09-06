# OmniNova Personal Chrome (B3.5-C)

Chrome cannot silently install unpacked extensions. Development load is one-time:

1. Build this extension: `npm install` then `npm run build`
2. Open `chrome://extensions`
3. Enable Developer mode
4. Load unpacked → this folder (`extensions/omninova-personal-chrome`)
5. Confirm the extension ID is `caooogobppgihkdpcjibhoinkfobenhe`
6. In OmniNova Desktop, install the Native Messaging host (`install_personal_chrome_bridge`)

Production Chrome Web Store packaging is deferred.

## Manifest permissions (B3.5-C v1)

| Permission | Why |
|---|---|
| `nativeMessaging` | B3.5-B transport to the Native Host |
| `storage` | Transport status for the popup |
| `alarms` | Reconnect + keepalive ping |
| `tabs` | Read/update **only** an explicitly authorized tab (`tabs.get` / `tabs.update`) |

Content scripts match `http://*/*` and `https://*/*` so the isolated-world DOM engine can run on normal web pages. This is **not** silent all-tab control: the service worker refuses every observe/act/navigate unless that exact tab is in the authorized-tab registry.

Not requested: `debugger`, `cookies`, `history`, `downloads`, `webNavigation`, `<all_urls>`, `scripting`, `activeTab`.

Screenshot (`captureVisibleTab`) is typed `OperationUnsupported` in v1 because it needs broader host access than this model allows.

Eval / `page_eval` / CDP / `chrome.debugger` are typed unsupported. Production Agent activation remains fail-closed until B3.5-D authorization UX.

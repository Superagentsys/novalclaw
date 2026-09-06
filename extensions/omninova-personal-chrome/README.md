# OmniNova Personal Chrome (B3.5-B transport)

Chrome cannot silently install unpacked extensions. Development load is one-time:

1. Build this extension: `npm install` then `npm run build`
2. Open `chrome://extensions`
3. Enable Developer mode
4. Load unpacked → this folder (`extensions/omninova-personal-chrome`)
5. Confirm the extension ID is `caooogobppgihkdpcjibhoinkfobenhe`
6. In OmniNova Desktop, install the Native Messaging host (`install_personal_chrome_bridge`)

Production Chrome Web Store packaging is deferred.

This skeleton only establishes Native Messaging transport. It does not automate pages.

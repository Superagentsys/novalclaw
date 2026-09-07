# Third-party notices

## AnyDoc 0.2.4

Local document extraction is powered by AnyDoc (https://github.com/firecrawl/anydoc).
No hosted document conversion is enabled by this integration.

MIT License

Copyright (c) 2026 Sideguide Technologies Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Ant Design Icons

This application uses `@ant-design/icons` 6.3.2 for interface icons.

Source: <https://github.com/ant-design/ant-design-icons>

MIT License

Copyright (c) 2018-present Ant UED, <https://xtech.antfin.com/>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## agent-browser

This application may bundle the `agent-browser` native CLI (pinned 0.36.0) as
an optional Browser Runtime. The CLI is extracted at build time from the npm
package `agent-browser` and is **not** copied from a developer machine path.

Source: <https://github.com/vercel-labs/agent-browser>

Copyright 2025 Vercel Inc.

Licensed under the Apache License, Version 2.0.
A copy of the Apache-2.0 license is staged next to the binary as
`resources/agent-browser/LICENSE` when `npm run prepare:browser-runtime` runs.

The bundled CLI does not include Chromium. Chrome for Testing / system Chrome
is obtained at runtime by `agent-browser install` or by detecting an existing
Chrome/Brave/Playwright/Puppeteer install. OmniNova does not redistribute
Google Chrome.

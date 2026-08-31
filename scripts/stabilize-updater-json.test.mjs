import assert from 'node:assert/strict'
import test from 'node:test'

import { stabilizeUpdaterJson } from './stabilize-updater-json.mjs'

test('replaces GitHub asset API URLs with stable download URLs', () => {
  const apiUrl = 'https://api.github.com/repos/owner/repo/releases/assets/123'
  const stableUrl = 'https://github.com/owner/repo/releases/download/v1.2.3/app.tar.gz'
  const latest = {
    version: '1.2.3',
    platforms: {
      'darwin-aarch64': { url: apiUrl, signature: 'sig' },
      'darwin-aarch64-app': { url: apiUrl, signature: 'sig' },
    },
  }

  const result = stabilizeUpdaterJson(latest, {
    assets: [{ apiUrl, url: stableUrl }],
  })

  assert.equal(result.platforms['darwin-aarch64'].url, stableUrl)
  assert.equal(result.platforms['darwin-aarch64-app'].url, stableUrl)
})

import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { resolve } from 'node:path'

export function stabilizeUpdaterJson(latest, release) {
  if (!latest?.platforms || !Array.isArray(release?.assets)) {
    throw new Error('latest.json 或 Release assets 格式无效')
  }

  const stableUrlByApiUrl = new Map(
    release.assets.map((asset) => [asset.apiUrl, asset.url]),
  )
  const result = structuredClone(latest)

  for (const [platform, config] of Object.entries(result.platforms)) {
    if (typeof config?.url !== 'string') {
      throw new Error(`${platform} 缺少 updater URL`)
    }
    if (!config.url.startsWith('https://api.github.com/')) continue

    const stableUrl = stableUrlByApiUrl.get(config.url)
    if (!stableUrl) {
      throw new Error(`${platform} 的 updater 资产不在当前 Release 中`)
    }
    config.url = stableUrl
  }

  return result
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [latestPath, assetsPath] = process.argv.slice(2)
  if (!latestPath || !assetsPath) {
    throw new Error('用法: node scripts/stabilize-updater-json.mjs <latest.json> <assets.json>')
  }
  const latest = JSON.parse(readFileSync(latestPath, 'utf8'))
  const release = JSON.parse(readFileSync(assetsPath, 'utf8'))
  const stable = stabilizeUpdaterJson(latest, release)
  writeFileSync(latestPath, `${JSON.stringify(stable, null, 2)}\n`)
}

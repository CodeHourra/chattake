import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const tag = process.argv[2] ?? process.env.GITHUB_REF_NAME
if (!tag || !/^v\d+\.\d+\.\d+$/.test(tag)) {
  console.error(`无效发布标签：${tag ?? '未提供'}，必须为 vX.Y.Z`)
  process.exit(1)
}

const root = resolve(fileURLToPath(new URL('..', import.meta.url)))
const expected = tag.slice(1)
const errors = []
const jsonFiles = [
  'apps/desktop/package.json',
  'apps/desktop/src-tauri/tauri.conf.json',
  'packages/shared/package.json',
  'packages/sidecar/package.json',
  'packages/mcp-server/package.json',
]

for (const file of jsonFiles) {
  const actual = JSON.parse(readFileSync(resolve(root, file), 'utf8')).version
  if (actual !== expected) errors.push(`${file}: ${actual ?? '缺少 version'}`)
}

const cargo = readFileSync(resolve(root, 'apps/desktop/src-tauri/Cargo.toml'), 'utf8')
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
if (cargoVersion !== expected) errors.push(`apps/desktop/src-tauri/Cargo.toml: ${cargoVersion ?? '缺少 version'}`)

const changelog = readFileSync(resolve(root, 'CHANGELOG.md'), 'utf8')
if (!changelog.includes(`\n## ${expected}（`) && !changelog.includes(`\n## ${expected} (`)) {
  errors.push(`CHANGELOG.md: 缺少 ${expected} 版本标题`)
}

if (errors.length) {
  console.error(`标签 ${tag} 与发布元数据不一致：\n- ${errors.join('\n- ')}`)
  process.exit(1)
}

console.log(`发布版本一致：${expected}`)

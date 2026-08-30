const target = process.env.CHATTAKE_BUN_TARGET?.trim()
const builds = [
  ['packages/sidecar/src/index.ts', 'packages/sidecar/dist/chattake-sidecar'],
  ['packages/mcp-server/src/index.ts', 'packages/mcp-server/dist/chattake-mcp'],
]

for (const [entry, output] of builds) {
  const result = Bun.spawnSync([
    process.execPath,
    'build',
    entry,
    '--compile',
    ...(target ? [`--target=${target}`] : []),
    '--outfile',
    output,
  ], { cwd: import.meta.dir + '/..', stdout: 'inherit', stderr: 'inherit' })
  if (result.exitCode !== 0) process.exit(result.exitCode)
}

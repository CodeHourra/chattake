import { join } from 'path'
import { homedir } from 'os'

export const XUNJI_HOME = join(homedir(), '.xunji')
export const XUNJI_DB_PATH = join(XUNJI_HOME, 'db', 'xunji.db')
export const XUNJI_CONFIG_PATH = join(XUNJI_HOME, 'config.toml')
export const XUNJI_LOG_DIR = join(XUNJI_HOME, 'logs')

export const DEFAULT_CLAUDE_CODE_DIRS = [join(homedir(), '.claude')]

export const DEFAULT_CURSOR_DIRS = [
  join(homedir(), 'Library', 'Application Support', 'Cursor'),
]

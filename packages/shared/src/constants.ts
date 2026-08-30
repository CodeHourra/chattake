import { join } from 'path'
import { homedir } from 'os'

export const CHATTAKE_HOME = join(homedir(), '.chattake')
export const CHATTAKE_DB_PATH = join(CHATTAKE_HOME, 'db', 'chattake.db')
export const CHATTAKE_CONFIG_PATH = join(CHATTAKE_HOME, 'config.toml')
export const CHATTAKE_LOG_DIR = join(CHATTAKE_HOME, 'logs')

export const DEFAULT_CLAUDE_CODE_DIRS = [join(homedir(), '.claude')]

export const DEFAULT_CURSOR_DIRS = [
  join(homedir(), 'Library', 'Application Support', 'Cursor'),
]

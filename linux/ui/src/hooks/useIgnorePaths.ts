import { useCallback } from 'react';
import { useStoredState } from './useStoredState';

const STORAGE_KEY = 'cardinal.ignorePaths';
// 信创（Linux）默认忽略表：聚焦「系统伪文件系统 + 高频缓存/日志 + 不可变系统目录」。
// 与 Rust 侧 `linux/src-tauri/src/index.rs::default_ignore_paths()` 保持一一对应。
const DEFAULT_IGNORE_PATHS = [
  '/proc',
  '/sys',
  '/dev',
  '/run',
  '/boot',
  '/lost+found',
  '/var/cache',
  '/var/lib/docker',
  '/var/lib/containers',
  '/var/log',
  '/var/tmp',
  '/snap',
  '/usr',
  '/lib',
  '/lib64',
  '/tmp',
  '~/.cache',
  '~/.npm',
  '~/.pnpm-store',
  '~/.cargo',
  '~/.m2',
  '~/.gradle',
  '~/.nuget',
  '~/.local/share/Trash',
  '~/.Trash',
  '~/.dsh',
];

const cleanPaths = (next: string[]): string[] =>
  next.map((item) => item.trim()).filter((item) => item.length > 0);

export function useIgnorePaths() {
  const [ignorePaths, setIgnorePathsState] = useStoredState<string[]>({
    key: STORAGE_KEY,
    defaultValue: DEFAULT_IGNORE_PATHS,
    read: (raw) => {
      const parsed = JSON.parse(raw);
      if (!Array.isArray(parsed)) return null;
      return cleanPaths(parsed.filter((item): item is string => typeof item === 'string'));
    },
    write: (value) => JSON.stringify(value),
    readErrorMessage: 'Unable to read saved ignore paths',
    writeErrorMessage: 'Unable to persist default ignore paths',
  });

  const setIgnorePaths = useCallback(
    (next: string[]) => {
      const cleaned = cleanPaths(next);
      setIgnorePathsState(cleaned);
    },
    [setIgnorePathsState],
  );

  return { ignorePaths, setIgnorePaths, defaultIgnorePaths: DEFAULT_IGNORE_PATHS };
}

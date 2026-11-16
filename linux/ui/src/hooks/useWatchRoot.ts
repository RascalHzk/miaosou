import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { useStoredState } from './useStoredState';

const STORAGE_KEY = 'cardinal.watchRoot';
// 信创（Linux/UOS）环境默认只索引用户主目录，而不是整盘 /。
// 这里保留 `~` 作为首屏兜底，随后用后端命令解析成 Debian/UOS 的真实路径（如 /home/uos）。
const DEFAULT_WATCH_ROOT = '~';

export function useWatchRoot() {
  const [watchRoot, setStoredState] = useStoredState<string>({
    key: STORAGE_KEY,
    defaultValue: DEFAULT_WATCH_ROOT,
    read: (raw) => {
      const trimmed = raw.trim();
      return trimmed.length > 0 ? trimmed : null;
    },
    write: (value) => value,
    readErrorMessage: 'Unable to read saved watch root',
    writeErrorMessage: 'Unable to persist default watch root',
  });

  const [defaultWatchRoot, setDefaultWatchRoot] = useState(DEFAULT_WATCH_ROOT);
  const watchRootRef = useRef(watchRoot);
  watchRootRef.current = watchRoot;

  // 启动时向 Rust 侧要一次平台默认值：UOS 上会得到 /home/<user>，设置页不再显示字面量 `~`。
  // 如果用户没有自定义过（仍等于兜底值），同步采纳为当前值并持久化。
  useEffect(() => {
    let cancelled = false;

    const resolveDefaultWatchRoot = async (): Promise<void> => {
      try {
        const resolved = await invoke<string>('get_default_watch_root');
        if (cancelled || !resolved || resolved === DEFAULT_WATCH_ROOT) {
          return;
        }
        setDefaultWatchRoot(resolved);
        if (watchRootRef.current === DEFAULT_WATCH_ROOT) {
          setStoredState(resolved);
        }
      } catch (error) {
        console.warn('Unable to resolve the platform default watch root', error);
      }
    };

    void resolveDefaultWatchRoot();
    return () => {
      cancelled = true;
    };
  }, [setStoredState]);

  const setWatchRoot = useCallback(
    (next: string) => {
      const trimmed = next.trim();
      const normalized = trimmed.length > 0 ? trimmed : defaultWatchRoot;
      setStoredState(normalized);
    },
    [defaultWatchRoot, setStoredState],
  );

  return { watchRoot, setWatchRoot, defaultWatchRoot };
}

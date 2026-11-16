import { useCallback } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { currentMonitor, getCurrent, primaryMonitor } from '@tauri-apps/api/window';

type QuickLookRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

type QuickLookItemPayload = {
  path: string;
  rect?: QuickLookRect;
  transitionImage?: string;
};

const escapePathForSelector = (value: string): string => {
  return window.CSS.escape(value);
};

/**
 * Quick Look 面板是 macOS 专属能力（上游用 objc2 + QLPreviewPanel 实现）。
 * Linux 上暂时禁用：后续 P3 用内置预览组件替代。
 */
export const QUICKLOOK_AVAILABLE =
  typeof navigator !== 'undefined' && /Macintosh|Mac OS X/.test(navigator.userAgent);

// Linux（信创目标）没有 QLPreviewPanel：P3 决策是**不做内置预览**，统一用系统默认程序打开
//（见 `openResultPath`）。因此非 macOS 平台不暴露「快速查看」入口、空格键也不触发。

type UseQuickLookConfig = {
  getPaths: () => string[];
};

/**
 * Provides Quick Look helpers for the file list. Given a function that returns the currently
 * selected paths, the hook exposes memoized callbacks to toggle/update/close the Quick Look panel.
 * It resolves window geometry when needed and translates DOM rects
 * into screen coordinates suitable for macOS' Quick Look APIs.
 */
export const useQuickLook = ({ getPaths }: UseQuickLookConfig) => {
  const resolveWindowGeometry = useCallback(async () => {
    try {
      const currentWindow = getCurrent();
      const [position, scaleFactor, monitor, mainMonitor] = await Promise.all([
        currentWindow.innerPosition(),
        currentWindow.scaleFactor(),
        currentMonitor(),
        primaryMonitor(),
      ]);

      if (!monitor || !mainMonitor) {
        return null;
      }

      // Bug in tauri-apps/tao: https://github.com/tauri-apps/tao/pull/1159
      position.y = position.y - mainMonitor.size.height + monitor.size.height;

      const scale = scaleFactor || monitor.scaleFactor || window.devicePixelRatio || 1;
      return {
        windowOrigin: {
          x: position.x / scale,
          y: position.y / scale,
        },
        screenHeight: monitor.size.height / scale,
      };
    } catch (error) {
      console.warn('Failed to resolve window metrics for Quick Look', error);
      return null;
    }
  }, []);

  const getQuickLookItems = useCallback(async (): Promise<QuickLookItemPayload[]> => {
    const paths = getPaths();
    if (!paths.length) {
      return [];
    }

    const geometry = await resolveWindowGeometry();
    if (!geometry) {
      return paths.map((path) => ({ path }));
    }

    // This compensates for a coordinate system mismatch on macOS:
    // - `geometry.windowOrigin.y` (from Tauri's `innerPosition`) is relative to the *visible* screen area (below the menu bar).
    // - `geometry.screenHeight` is the *full* height of the monitor hosting the window.
    // - `window.screen.availTop` provides the height of the menu bar, allowing us to correctly adjust `logicalYTop`
    //   to be relative to the absolute top of the screen for `QLPreviewPanel`'s `sourceFrameOnScreenForPreviewItem`.
    const screenTopOffset = (window.screen as { availTop?: number }).availTop ?? 0;

    const buildItem = (path: string): QuickLookItemPayload => {
      const selector = `[data-row-path="${escapePathForSelector(path)}"]`;
      const row = document.querySelector<HTMLElement>(selector);
      const anchor = row?.querySelector<HTMLElement>('.file-icon, .file-icon-placeholder');
      const iconImage = row?.querySelector<HTMLImageElement>('img.file-icon');
      if (!row || !anchor || !iconImage) {
        return { path };
      }

      const rect = anchor.getBoundingClientRect();
      const logicalX = geometry.windowOrigin.x + rect.left;
      const logicalYTop = geometry.windowOrigin.y + screenTopOffset + rect.top;
      const logicalWidth = rect.width;
      const logicalHeight = rect.height;

      const x = logicalX;
      const y = geometry.screenHeight - (logicalYTop + logicalHeight);

      return {
        path,
        rect: {
          x,
          y,
          width: logicalWidth,
          height: logicalHeight,
        },
        transitionImage: iconImage.src,
      };
    };

    return paths.map((path) => buildItem(path));
  }, [getPaths, resolveWindowGeometry]);

  const toggleQuickLook = useCallback(async () => {
    if (!QUICKLOOK_AVAILABLE) {
      return;
    }
    const items = await getQuickLookItems();
    if (!items.length) {
      return;
    }
    await invoke('toggle_quicklook', { items });
  }, [getQuickLookItems]);

  const updateQuickLook = useCallback(async () => {
    if (!QUICKLOOK_AVAILABLE) {
      return;
    }
    const items = await getQuickLookItems();
    if (!items.length) {
      return;
    }
    await invoke('update_quicklook', { items });
  }, [getQuickLookItems]);

  const closeQuickLook = useCallback(() => {
    if (!QUICKLOOK_AVAILABLE) {
      return;
    }
    void invoke('close_quicklook');
  }, []);

  return {
    toggleQuickLook,
    updateQuickLook,
    closeQuickLook,
  };
};

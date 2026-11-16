import { useCallback } from 'react';
import type { MouseEvent as ReactMouseEvent } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { useTranslation } from 'react-i18next';
import { openResultPath } from '../utils/openResultPath';
import { splitPath } from '../utils/path';

type ContextMenuAction = {
  id: string;
  label: string;
  accelerator?: string;
  action: () => void;
};

type UseContextMenuResult = {
  showContextMenu: (event: ReactMouseEvent<HTMLElement>, targetPaths: string[]) => void;
  showHeaderContextMenu: (event: ReactMouseEvent<HTMLElement>) => void;
};

/**
 * 右键菜单。
 *
 * Tauri v1 没有原生弹出菜单的 JS API（上游用 v2 的 `@tauri-apps/api/menu` + `Menu.popup()`），
 * 这里改为 **Web 侧自绘菜单**：DOM 浮层 + 视口内定位 + 外部点击/Esc/滚动自动关闭。
 * 交互与外观由 App.css 里的 `.app-context-menu*` 样式控制。
 */
let disposeActiveMenu: (() => void) | null = null;

const closeContextMenu = (): void => {
  disposeActiveMenu?.();
  disposeActiveMenu = null;
};

const showDomContextMenu = (clientX: number, clientY: number, items: ContextMenuAction[]): void => {
  closeContextMenu();
  if (items.length === 0) {
    return;
  }

  const menu = document.createElement('div');
  menu.className = 'app-context-menu';
  menu.setAttribute('role', 'menu');

  items.forEach((item) => {
    const entry = document.createElement('button');
    entry.type = 'button';
    entry.className = 'app-context-menu-item';
    entry.setAttribute('role', 'menuitem');
    entry.dataset.menuId = item.id;

    const label = document.createElement('span');
    label.className = 'app-context-menu-label';
    label.textContent = item.label;
    entry.appendChild(label);

    if (item.accelerator) {
      const accelerator = document.createElement('span');
      accelerator.className = 'app-context-menu-accelerator';
      // Linux 用户看到 Ctrl 而不是 Cmd
      accelerator.textContent = item.accelerator.replace(/Cmd/g, 'Ctrl');
      entry.appendChild(accelerator);
    }

    entry.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      closeContextMenu();
      item.action();
    });

    menu.appendChild(entry);
  });

  document.body.appendChild(menu);

  // 定位到鼠标位置，并保证不出视口
  const rect = menu.getBoundingClientRect();
  const margin = 8;
  const x = Math.max(margin, Math.min(clientX, window.innerWidth - rect.width - margin));
  const y = Math.max(margin, Math.min(clientY, window.innerHeight - rect.height - margin));
  menu.style.left = `${x}px`;
  menu.style.top = `${y}px`;

  const onPointerDown = (event: MouseEvent) => {
    if (!menu.contains(event.target as Node)) {
      closeContextMenu();
    }
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === 'Escape') {
      closeContextMenu();
    }
  };
  const onViewportChange = () => closeContextMenu();

  window.addEventListener('mousedown', onPointerDown, true);
  window.addEventListener('keydown', onKeyDown, true);
  window.addEventListener('resize', onViewportChange);
  window.addEventListener('scroll', onViewportChange, true);

  disposeActiveMenu = () => {
    window.removeEventListener('mousedown', onPointerDown, true);
    window.removeEventListener('keydown', onKeyDown, true);
    window.removeEventListener('resize', onViewportChange);
    window.removeEventListener('scroll', onViewportChange, true);
    menu.remove();
  };
};

export function useContextMenu(
  autoFitColumns: (() => void) | null = null,
  onQuickLookRequest?: () => void | Promise<void>,
): UseContextMenuResult {
  const { t } = useTranslation();
  const writeClipboard = useCallback((text: string) => {
    if (!navigator?.clipboard?.writeText) {
      return;
    }
    void navigator.clipboard.writeText(text);
  }, []);

  const buildFileMenuItems = useCallback(
    (targetPathsInput: string[]): ContextMenuAction[] => {
      const targetPaths = targetPathsInput.filter(Boolean);
      if (targetPaths.length === 0) {
        return [];
      }
      const copyLabel =
        targetPaths.length > 1 ? t('contextMenu.copyFiles') : t('contextMenu.copyFile');
      const copyFilenameLabel =
        targetPaths.length > 1 ? t('contextMenu.copyFilenames') : t('contextMenu.copyFilename');
      const copyPathLabel =
        targetPaths.length > 1 ? t('contextMenu.copyPaths') : t('contextMenu.copyPath');

      const items: ContextMenuAction[] = [
        {
          id: 'context_menu.open_item',
          label: t('contextMenu.openItem'),
          accelerator: 'Cmd+O',
          action: () => {
            targetPaths.forEach((itemPath) => openResultPath(itemPath));
          },
        },
        {
          id: 'context_menu.open_in_finder',
          label: t('contextMenu.revealInFinder'),
          accelerator: 'Cmd+R',
          action: () => {
            targetPaths.forEach((itemPath) => {
              void invoke('open_in_finder', { path: itemPath });
            });
          },
        },
        {
          id: 'context_menu.copy_filename',
          label: copyFilenameLabel,
          action: () => {
            const filenames = targetPaths
              .map((itemPath) => splitPath(itemPath).name || itemPath)
              .join(' ');
            writeClipboard(filenames);
          },
        },
        {
          id: 'context_menu.copy_paths',
          label: copyPathLabel,
          accelerator: 'Cmd+Shift+C',
          action: () => {
            writeClipboard(targetPaths.join('\n'));
          },
        },
        {
          id: 'context_menu.copy_files',
          label: copyLabel,
          accelerator: 'Cmd+C',
          action: () => {
            void invoke('copy_files_to_clipboard', { paths: targetPaths }).catch((error) => {
              console.error('Failed to copy files to clipboard', error);
            });
          },
        },
      ];

      if (onQuickLookRequest) {
        items.push({
          id: 'context_menu.quicklook',
          label: t('contextMenu.quickLook'),
          accelerator: 'Space',
          action: () => {
            void onQuickLookRequest();
          },
        });
      }

      return items;
    },
    [onQuickLookRequest, t, writeClipboard],
  );

  const buildHeaderMenuItems = useCallback((): ContextMenuAction[] => {
    if (!autoFitColumns) {
      return [];
    }

    return [
      {
        id: 'context_menu.reset_column_widths',
        label: t('contextMenu.resetColumnWidths'),
        action: () => {
          autoFitColumns();
        },
      },
    ];
  }, [autoFitColumns, t]);

  const showContextMenu = useCallback(
    (event: ReactMouseEvent<HTMLElement>, targetPaths: string[]) => {
      event.preventDefault();
      event.stopPropagation();
      showDomContextMenu(event.clientX, event.clientY, buildFileMenuItems(targetPaths));
    },
    [buildFileMenuItems],
  );

  const showHeaderContextMenu = useCallback(
    (event: ReactMouseEvent<HTMLElement>) => {
      event.preventDefault();
      event.stopPropagation();
      showDomContextMenu(event.clientX, event.clientY, buildHeaderMenuItems());
    },
    [buildHeaderMenuItems],
  );

  return {
    showContextMenu,
    showHeaderContextMenu,
  };
}

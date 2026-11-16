import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { I18nextProvider } from 'react-i18next';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import i18n from '../../i18n/config';
import { useContextMenu } from '../useContextMenu';

const mocks = vi.hoisted(() => ({
  invokeMock: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@tauri-apps/api/tauri', () => ({
  invoke: mocks.invokeMock,
}));

vi.mock('../../utils/openResultPath', () => ({
  openResultPath: vi.fn(),
}));

const wrapper = ({ children }: { children: ReactNode }) => (
  <I18nextProvider i18n={i18n}>{children}</I18nextProvider>
);

const createEvent = () =>
  ({
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
    clientX: 40,
    clientY: 60,
  }) as unknown as React.MouseEvent<HTMLElement>;

/** 从 DOM 浮层里取某个菜单项（Tauri v1 没有原生菜单，菜单是 Web 侧自绘的）。 */
const findMenuItem = (id: string): HTMLButtonElement | null =>
  document.querySelector<HTMLButtonElement>(`.app-context-menu [data-menu-id="${id}"]`);

const menuItems = (): HTMLButtonElement[] =>
  Array.from(document.querySelectorAll<HTMLButtonElement>('.app-context-menu [data-menu-id]'));

const labelOf = (item: HTMLButtonElement): string =>
  item.querySelector('.app-context-menu-label')?.textContent ?? '';

const acceleratorOf = (item: HTMLButtonElement): string =>
  item.querySelector('.app-context-menu-accelerator')?.textContent ?? '';

describe('useContextMenu (DOM overlay menu)', () => {
  beforeEach(async () => {
    document.body.innerHTML = '';
    mocks.invokeMock.mockClear();
    await i18n.changeLanguage('en-US');
  });

  afterEach(() => {
    // 关闭可能残留的浮层，避免影响下一个用例
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    document.body.innerHTML = '';
  });

  it('uses plural Copy Paths label and Ctrl shortcut when multiple paths are selected', async () => {
    const { result } = renderHook(() => useContextMenu(null), { wrapper });

    result.current.showContextMenu(createEvent(), ['/a', '/b']);

    await waitFor(() => {
      expect(menuItems().length).toBeGreaterThan(0);
    });

    const copyPaths = findMenuItem('context_menu.copy_paths');
    expect(copyPaths).not.toBeNull();
    expect(labelOf(copyPaths!)).toBe('Copy Paths');
    // Linux 用户看到 Ctrl，而不是 Cmd
    expect(acceleratorOf(copyPaths!)).toBe('Ctrl+Shift+C');
  });

  it('uses singular Copy Path label when a single path is targeted', async () => {
    const { result } = renderHook(() => useContextMenu(null), { wrapper });

    result.current.showContextMenu(createEvent(), ['/a']);

    await waitFor(() => {
      expect(menuItems().length).toBeGreaterThan(0);
    });

    const copyPaths = findMenuItem('context_menu.copy_paths');
    expect(copyPaths).not.toBeNull();
    expect(labelOf(copyPaths!)).toBe('Copy Path');
  });

  it('uses provided target paths and ignores empty entries', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(globalThis.navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });

    const { result } = renderHook(() => useContextMenu(null), { wrapper });

    result.current.showContextMenu(createEvent(), ['', '/clicked']);

    await waitFor(() => {
      expect(findMenuItem('context_menu.copy_paths')).not.toBeNull();
    });
    expect(labelOf(findMenuItem('context_menu.copy_paths')!)).toBe('Copy Path');

    findMenuItem('context_menu.copy_paths')!.click();

    expect(writeText).toHaveBeenCalledWith('/clicked');
  });

  it('copies all selected paths to the clipboard', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(globalThis.navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });

    const { result } = renderHook(() => useContextMenu(null), { wrapper });

    result.current.showContextMenu(createEvent(), ['/a', '/b']);

    await waitFor(() => {
      expect(findMenuItem('context_menu.copy_paths')).not.toBeNull();
    });

    findMenuItem('context_menu.copy_paths')!.click();

    expect(writeText).toHaveBeenCalledWith('/a\n/b');
  });

  it('delegates copy-files to the shell and closes the menu', async () => {
    const { result } = renderHook(() => useContextMenu(null), { wrapper });

    result.current.showContextMenu(createEvent(), ['/a', '/b']);

    await waitFor(() => {
      expect(findMenuItem('context_menu.copy_files')).not.toBeNull();
    });

    findMenuItem('context_menu.copy_files')!.click();

    expect(mocks.invokeMock).toHaveBeenCalledWith('copy_files_to_clipboard', {
      paths: ['/a', '/b'],
    });
    // 点击后浮层立即关闭
    expect(findMenuItem('context_menu.copy_files')).toBeNull();
  });

  it('adds a Quick Look entry only when a handler is provided', async () => {
    const onQuickLookRequest = vi.fn();
    const { result } = renderHook(() => useContextMenu(null, onQuickLookRequest), { wrapper });

    result.current.showContextMenu(createEvent(), ['/a']);

    await waitFor(() => {
      expect(findMenuItem('context_menu.quicklook')).not.toBeNull();
    });
    findMenuItem('context_menu.quicklook')!.click();
    expect(onQuickLookRequest).toHaveBeenCalled();
  });
});

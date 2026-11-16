import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useIgnorePaths } from '../useIgnorePaths';

const STORAGE_KEY = 'cardinal.ignorePaths';
// 与 useIgnorePaths.ts 的 Linux 默认忽略表保持一致（Rust 侧 index.rs::default_ignore_paths 同步）
const EXPECTED_DEFAULT_IGNORE_PATHS = [
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

const flushEffects = async () => {
  await act(async () => {});
};

describe('useIgnorePaths', () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('hydrates from stored values and filters invalid entries', async () => {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify([' /tmp ', '', 42, '   ', '/var']));
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem');

    const { result } = renderHook(() => useIgnorePaths());

    expect(result.current.ignorePaths).toEqual(['/tmp', '/var']);

    await flushEffects();

    expect(setItemSpy).not.toHaveBeenCalled();
  });

  it('uses defaults and persists when no stored value exists', async () => {
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem');

    const { result } = renderHook(() => useIgnorePaths());

    expect(result.current.ignorePaths).toEqual(EXPECTED_DEFAULT_IGNORE_PATHS);
    expect(result.current.defaultIgnorePaths).toEqual(EXPECTED_DEFAULT_IGNORE_PATHS);

    await flushEffects();

    expect(setItemSpy).toHaveBeenCalledWith(
      STORAGE_KEY,
      JSON.stringify(EXPECTED_DEFAULT_IGNORE_PATHS),
    );
  });

  it('includes pseudo filesystems, caches and immutable system dirs in default ignore paths', () => {
    const { result } = renderHook(() => useIgnorePaths());

    expect(result.current.defaultIgnorePaths).toContain('/proc');
    expect(result.current.defaultIgnorePaths).toContain('/sys');
    expect(result.current.defaultIgnorePaths).toContain('/tmp');
    expect(result.current.defaultIgnorePaths).toContain('/var/log');
    expect(result.current.defaultIgnorePaths).toContain('/usr');
    expect(result.current.defaultIgnorePaths).toContain('/lib64');
    expect(result.current.defaultIgnorePaths).toContain('~/.cache');
    expect(result.current.defaultIgnorePaths).toContain('~/.cargo');
    expect(result.current.defaultIgnorePaths).toContain('~/.m2');
    expect(result.current.defaultIgnorePaths).toContain('~/.local/share/Trash');
  });

  it('keeps an empty stored array without writing defaults', async () => {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(['', '   ']));
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem');

    const { result } = renderHook(() => useIgnorePaths());

    expect(result.current.ignorePaths).toEqual([]);

    await flushEffects();

    expect(setItemSpy).not.toHaveBeenCalled();
  });

  it('cleans and persists updates', async () => {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify([]));
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem');

    const { result } = renderHook(() => useIgnorePaths());

    await flushEffects();

    act(() => {
      result.current.setIgnorePaths([' /tmp ', '', '/var', '   ']);
    });

    expect(result.current.ignorePaths).toEqual(['/tmp', '/var']);
    expect(setItemSpy).toHaveBeenCalledWith(STORAGE_KEY, JSON.stringify(['/tmp', '/var']));
  });

  it('falls back to defaults when stored JSON is invalid', async () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    window.localStorage.setItem(STORAGE_KEY, '{');
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem');

    const { result } = renderHook(() => useIgnorePaths());

    expect(result.current.ignorePaths).toEqual(result.current.defaultIgnorePaths);

    await flushEffects();

    expect(setItemSpy).toHaveBeenCalledWith(
      STORAGE_KEY,
      JSON.stringify(result.current.defaultIgnorePaths),
    );
    warnSpy.mockRestore();
  });
});

import { invoke } from '@tauri-apps/api/tauri';

/**
 * 全局唤起快捷键。
 *
 * 上游用 v2 的 `@tauri-apps/plugin-global-shortcut` 在前端注册；v1 没有该插件，
 * 改为调用 Rust 命令 `set_global_shortcut`（内部用 v1 内置的 GlobalShortcutManager）。
 * 注意 accelerator 语法：v1 用 `CmdOrCtrl+Shift+Space`（v2 前端用的是 `Command+Shift+Space`）。
 */
export const QUICK_LAUNCH_SHORTCUT = 'CmdOrCtrl+Shift+Space';

export async function initializeGlobalShortcuts(): Promise<void> {
  try {
    await invoke('set_global_shortcut', { accelerator: QUICK_LAUNCH_SHORTCUT });
  } catch (error) {
    console.error('Failed to register global shortcuts', error);
  }
}

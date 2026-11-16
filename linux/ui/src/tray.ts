import { invoke } from '@tauri-apps/api/tauri';

/**
 * Tauri v1 迁移说明
 *
 * 上游用 v2 的 `@tauri-apps/api/tray` 在前端创建托盘图标。
 * v1 只能在 Rust 侧创建托盘（`linux/src-tauri/src/native_ui.rs` 的 `build_tray`），
 * 且 v1 **没有运行时移除托盘的 API**，所以这里的启用/禁用只能同步 macOS 的激活策略，
 * Linux 下为无操作（托盘常驻）。这是与上游的已知差异，记录在 PORTING.md。
 */
let trayInitPromise: Promise<void> | null = null;

export function initializeTray(): Promise<void> {
  if (!trayInitPromise) {
    trayInitPromise = invoke('set_tray_activation_policy', { enabled: true })
      .then(() => undefined)
      .catch((error) => {
        console.error('Failed to initialize tray', error);
        trayInitPromise = null;
      });
  }

  return trayInitPromise;
}

export async function setTrayEnabled(enabled: boolean): Promise<void> {
  await invoke('set_tray_activation_policy', { enabled }).catch((error) => {
    console.error('Failed to update tray activation policy', error);
  });

  if (enabled) {
    await initializeTray();
  }
}

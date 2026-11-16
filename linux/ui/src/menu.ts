import { listen } from '@tauri-apps/api/event';
import { openPreferences } from './utils/openPreferences';

/**
 * Tauri v1 迁移说明
 *
 * 上游用 v2 的 `@tauri-apps/api/menu` 在**前端**构建 App 菜单，并随 i18n 语言切换重建。
 * Tauri v1 没有 JS 菜单 API，菜单改为在 Rust 侧用 `Builder::menu()` 构建
 * （见 `linux/src-tauri/src/native_ui.rs`），文案固定简体中文。
 *
 * 前端只剩一件事：把 Rust 菜单的「偏好设置」点击（`menu_preferences` 事件）
 * 转成应用内的 OPEN_PREFERENCES_EVENT。
 */
let menuInitPromise: Promise<void> | null = null;

export function initializeAppMenu(): Promise<void> {
  if (!menuInitPromise) {
    menuInitPromise = listen('menu_preferences', () => {
      openPreferences();
    })
      .then(() => undefined)
      .catch((error) => {
        console.error('Failed to register menu event listener', error);
        menuInitPromise = null;
      });
  }

  return menuInitPromise;
}

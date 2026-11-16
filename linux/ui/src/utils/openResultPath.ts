import { invoke } from '@tauri-apps/api/tauri';

export const openResultPath = (path: string | null | undefined): void => {
  if (!path) {
    return;
  }

  // Rust 侧会依次尝试 dde-open / xdg-open / gio open；全部失败时把原因打到控制台，
  // 避免「点了没反应」无从排查（现场可用 MIAOSOU_TRACE=1 从终端看 stderr）。
  void invoke('open_path', { path }).catch((error) => {
    console.error('Failed to open path', path, error);
  });
};

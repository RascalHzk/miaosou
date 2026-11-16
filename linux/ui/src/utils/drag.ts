/**
 * 原生文件拖拽（把文件从列表拖到其他应用）。
 *
 * 上游用 v2 的 `@crabnebula/tauri-plugin-drag` 实现；**Tauri v1 没有对应能力**
 * （文件拖出是 v2 才引入的 API）。这里的降级策略：把路径复制到剪贴板，
 * 用户可在目标应用里直接粘贴路径。等 P3 评估 GTK 原生 drag source 后再补。
 */
export type NativeFileDragOptions = Readonly<{
  paths: readonly string[];
  icon?: string;
  mode?: 'copy' | 'move';
}>;

export const startNativeFileDrag = async ({ paths }: NativeFileDragOptions): Promise<void> => {
  const normalizedPaths = paths.filter(
    (filePath): filePath is string => typeof filePath === 'string' && filePath.length > 0,
  );
  if (normalizedPaths.length === 0) {
    return;
  }

  try {
    if (navigator?.clipboard?.writeText) {
      await navigator.clipboard.writeText(normalizedPaths.join('\n'));
      console.info('[cardinal] 当前平台不支持拖出文件，已改为复制路径到剪贴板');
    }
  } catch (error) {
    console.warn('startNativeFileDrag fallback failed', error);
  }
};

import { useCallback, useState } from 'react';

export type FullDiskAccessStatus = 'granted' | 'denied';

type UseFullDiskAccessPermissionResult = {
  status: FullDiskAccessStatus;
  isChecking: boolean;
  requestPermission: () => Promise<void>;
};

/**
 * 「完全磁盘访问」是 macOS 的隐私机制。
 *
 * 上游通过 v2 的 `tauri-plugin-macos-permissions` 检查并引导用户授权。
 * 信创目标平台（UOS V20 / Linux）没有这个概念：能否读取目录取决于文件系统权限，
 * v1 也没有对应插件。因此这里统一返回 granted，权限问题改由扫描器在
 * 遇到 EACCES 时按目录跳过（见 P2 的 ignore/权限统计）。
 */
export function useFullDiskAccessPermission(): UseFullDiskAccessPermissionResult {
  const [status] = useState<FullDiskAccessStatus>('granted');
  const [isChecking] = useState(false);

  const requestPermission = useCallback(async () => {
    // Linux 无对应操作
  }, []);

  return { status, isChecking, requestPermission };
}

import { invoke } from '@tauri-apps/api/tauri';

export type WatchConfigPayload = {
  watchRoot: string;
  ignorePaths: string[];
  includePaths: string[];
};

export const setWatchConfig = (payload: WatchConfigPayload): Promise<void> => {
  return invoke('set_watch_config', payload);
};

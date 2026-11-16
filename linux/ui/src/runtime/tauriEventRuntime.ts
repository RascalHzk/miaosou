import { listen } from '@tauri-apps/api/event';
import type { Event as TauriEvent, UnlistenFn } from '@tauri-apps/api/event';
import { getCurrent } from '@tauri-apps/api/window';
import type { FileDropEvent } from '@tauri-apps/api/window';
import type {
  AppLifecycleStatus,
  IconUpdatePayload,
  IconUpdateWirePayload,
  RecentEventPayload,
  StatusBarUpdatePayload,
} from '../types/ipc';
import type { SlabIndex } from '../types/slab';

export type QuickLookKeydownPayload = {
  keyCode: number;
  characters?: string | null;
  modifiers: {
    shift: boolean;
    control: boolean;
    option: boolean;
    command: boolean;
  };
};

type Listener<T> = (payload: T) => void;

/**
 * Tauri v1 的 `FileDropEvent` 与 v2 的 `DragDropEvent` 差异：
 *   - v2 带 `position`（物理像素），v1 **不带**；
 *   - v2 的取消态叫 `cancelled`，v1 叫 `cancel`。
 *
 * 上游 `useAppWindowListeners` 需要用落点坐标判断“是否拖到搜索框上”，
 * 因此这里补一个指针位置跟踪：监听 pointermove 记录最新客户端坐标，
 * 拖放时按 devicePixelRatio 换算成物理像素，保持消费方逻辑不变。
 */
export type FileDropPayload =
  | { type: 'hover'; position: { x: number; y: number }; paths: string[] }
  | { type: 'drop'; position: { x: number; y: number }; paths: string[] }
  | { type: 'cancelled' };

export type WindowDragDropEvent = { payload: FileDropPayload };

const lastPointerPosition = { x: 0, y: 0 };

const installPointerTracker = (): void => {
  if (typeof window === 'undefined') {
    return;
  }
  window.addEventListener(
    'pointermove',
    (event) => {
      lastPointerPosition.x = event.clientX;
      lastPointerPosition.y = event.clientY;
    },
    { passive: true, capture: true },
  );
};

const toFileDropPayload = (event: TauriEvent<FileDropEvent>): FileDropPayload => {
  const scale = (typeof window !== 'undefined' && window.devicePixelRatio) || 1;
  const position = {
    x: lastPointerPosition.x * scale,
    y: lastPointerPosition.y * scale,
  };
  const payload = event.payload;

  if (payload.type === 'drop') {
    return { type: 'drop', paths: payload.paths, position };
  }
  if (payload.type === 'hover') {
    return { type: 'hover', paths: payload.paths, position };
  }
  return { type: 'cancelled' };
};

const statusBarUpdateListeners = new Set<Listener<StatusBarUpdatePayload>>();
const lifecycleStateListeners = new Set<Listener<AppLifecycleStatus>>();
const quickLaunchListeners = new Set<Listener<void>>();
const fsEventsBatchListeners = new Set<Listener<RecentEventPayload[]>>();
const iconUpdateListeners = new Set<Listener<readonly IconUpdatePayload[]>>();
const quickLookKeydownListeners = new Set<Listener<QuickLookKeydownPayload>>();
const windowDragDropListeners = new Set<Listener<WindowDragDropEvent>>();

let initPromise: Promise<void> | null = null;

const emit = <T>(listeners: Set<Listener<T>>, payload: T): void => {
  listeners.forEach((listener) => {
    listener(payload);
  });
};

const subscribe = <T>(listeners: Set<Listener<T>>, listener: Listener<T>): UnlistenFn => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};

const isRecentEventPayload = (value: unknown): value is RecentEventPayload => {
  if (!value || typeof value !== 'object') {
    return false;
  }

  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.path === 'string' &&
    typeof candidate.eventId === 'number' &&
    typeof candidate.timestamp === 'number' &&
    typeof candidate.flagBits === 'number'
  );
};

const normalizeRecentEvents = (payload: unknown): RecentEventPayload[] => {
  if (!Array.isArray(payload)) {
    return [];
  }
  return payload.filter(isRecentEventPayload);
};

const isIconUpdateWirePayload = (value: unknown): value is IconUpdateWirePayload => {
  if (!value || typeof value !== 'object') {
    return false;
  }

  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.slabIndex === 'number' &&
    (typeof candidate.icon === 'string' || typeof candidate.icon === 'undefined')
  );
};

const normalizeIconUpdates = (payload: unknown): IconUpdatePayload[] => {
  if (!Array.isArray(payload)) {
    return [];
  }
  return payload
    .filter(isIconUpdateWirePayload)
    .map((item) => ({ slabIndex: item.slabIndex as SlabIndex, icon: item.icon }));
};

export const initializeTauriEventRuntime = (): Promise<void> => {
  if (initPromise) {
    return initPromise;
  }

  initPromise = (async () => {
    installPointerTracker();

    const setupTasks: Promise<unknown>[] = [
      listen<StatusBarUpdatePayload>('status_bar_update', (event) => {
        const payload = event.payload;
        if (!payload) return;
        emit(statusBarUpdateListeners, payload);
      }).catch((error) => {
        console.error('Failed to register status_bar_update listener', error);
      }),
      listen<AppLifecycleStatus>('app_lifecycle_state', (event) => {
        const status = event.payload;
        if (!status) return;
        emit(lifecycleStateListeners, status);
      }).catch((error) => {
        console.error('Failed to register app_lifecycle_state listener', error);
      }),
      listen('quick_launch', () => {
        emit(quickLaunchListeners, undefined);
      }).catch((error) => {
        console.error('Failed to register quick_launch listener', error);
      }),
      listen<RecentEventPayload[]>('fs_events_batch', (event) => {
        const payload = normalizeRecentEvents(event.payload);
        if (payload.length === 0) return;
        emit(fsEventsBatchListeners, payload);
      }).catch((error) => {
        console.error('Failed to register fs_events_batch listener', error);
      }),
      listen<readonly IconUpdateWirePayload[] | null | undefined>('icon_update', (event) => {
        const payload = normalizeIconUpdates(event.payload);
        if (payload.length === 0) return;
        emit(iconUpdateListeners, payload);
      }).catch((error) => {
        console.error('Failed to register icon_update listener', error);
      }),
      listen<QuickLookKeydownPayload>('quicklook-keydown', (event) => {
        const payload = event.payload;
        if (!payload) return;
        emit(quickLookKeydownListeners, payload);
      }).catch((error) => {
        console.error('Failed to register quicklook-keydown listener', error);
      }),
      getCurrent()
        .onFileDropEvent((event) => {
          emit(windowDragDropListeners, { payload: toFileDropPayload(event) });
        })
        .catch((error) => {
          console.error('Failed to register file-drop listener', error);
        }),
    ];

    await Promise.allSettled(setupTasks);
  })();

  return initPromise;
};

export const subscribeStatusBarUpdate = (
  listener: Listener<StatusBarUpdatePayload>,
): UnlistenFn => {
  void initializeTauriEventRuntime();
  return subscribe(statusBarUpdateListeners, listener);
};

export const subscribeLifecycleState = (listener: Listener<AppLifecycleStatus>): UnlistenFn => {
  void initializeTauriEventRuntime();
  return subscribe(lifecycleStateListeners, listener);
};

export const subscribeQuickLaunch = (listener: () => void): UnlistenFn => {
  void initializeTauriEventRuntime();
  return subscribe(quickLaunchListeners, listener);
};

export const subscribeFSEventsBatch = (listener: Listener<RecentEventPayload[]>): UnlistenFn => {
  void initializeTauriEventRuntime();
  return subscribe(fsEventsBatchListeners, listener);
};

export const subscribeIconUpdate = (
  listener: Listener<readonly IconUpdatePayload[]>,
): UnlistenFn => {
  void initializeTauriEventRuntime();
  return subscribe(iconUpdateListeners, listener);
};

export const subscribeQuickLookKeydown = (
  listener: Listener<QuickLookKeydownPayload>,
): UnlistenFn => {
  void initializeTauriEventRuntime();
  return subscribe(quickLookKeydownListeners, listener);
};

export const subscribeWindowDragDrop = (listener: Listener<WindowDragDropEvent>): UnlistenFn => {
  void initializeTauriEventRuntime();
  return subscribe(windowDragDropListeners, listener);
};

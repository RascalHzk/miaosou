// App 级测试共用的 hook mock：两个 App.* 测试文件都需要同一套「与布局/事件流无关」的
// hook 返回值，抽到这里避免同一段 vi.mock 工厂代码复制两份。
import { vi } from 'vitest';

export const columnResizeMock = () => ({
  colWidths: {
    filename: 200,
    path: 300,
    size: 100,
    modified: 120,
    created: 120,
  },
  onResizeStart: vi.fn(() => vi.fn()),
  autoFitColumns: vi.fn(),
});

export const eventColumnWidthsMock = () => ({
  eventColWidths: {
    time: 120,
    event: 180,
    name: 180,
    path: 260,
  },
  onEventResizeStart: vi.fn(),
  autoFitEventColumns: vi.fn(),
});

export const recentFSEventsMock = () => ({
  filteredEvents: [],
});

export const remoteSortMock = () => ({
  sortState: null,
  displayedResults: [101, 202],
  displayedResultsVersion: 1,
  sortThreshold: 20000,
  setSortThreshold: vi.fn(),
  canSort: true,
  isSorting: false,
  sortDisabledTooltip: null,
  sortButtonsDisabled: false,
  handleSortToggle: vi.fn(),
});

// `useRemoteSort` 模块还导出阈值常量，保持与实现一致。
export const DEFAULT_SORTABLE_RESULT_THRESHOLD = 20000;

import { create } from 'zustand';

type ActiveTab = 'dashboard' | 'chat' | 'plan' | 'tasks' | 'graph' | 'branches' | 'conflicts' | 'certification' | 'settings' | 'skills' | 'loops' | 'reviews';

interface UiState {
  activeTab: ActiveTab;
  selectedObjectiveId: string | null;
  selectedTaskId: string | null;
  selectedNodeId: string | null;
  inspectorOpen: boolean;
  drawerOpen: boolean;
  drawerTab: 'activity' | 'logs' | 'diff' | 'artifacts';
  setActiveTab: (tab: ActiveTab) => void;
  selectObjective: (id: string | null) => void;
  selectTask: (id: string | null) => void;
  selectNode: (id: string | null) => void;
  toggleInspector: () => void;
  toggleDrawer: () => void;
  setDrawerTab: (tab: UiState['drawerTab']) => void;
}

export const useUiStore = create<UiState>((set) => ({
  activeTab: 'dashboard',
  selectedObjectiveId: null,
  selectedTaskId: null,
  selectedNodeId: null,
  inspectorOpen: true,
  drawerOpen: true,
  drawerTab: 'activity',
  setActiveTab: (tab) => set({ activeTab: tab }),
  selectObjective: (id) => set({ selectedObjectiveId: id }),
  selectTask: (id) => set({ selectedTaskId: id, inspectorOpen: true }),
  selectNode: (id) => set({ selectedNodeId: id, inspectorOpen: true }),
  toggleInspector: () => set((s) => ({ inspectorOpen: !s.inspectorOpen })),
  toggleDrawer: () => set((s) => ({ drawerOpen: !s.drawerOpen })),
  setDrawerTab: (tab) => set({ drawerTab: tab }),
}));

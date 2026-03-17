import { create } from 'zustand';
import { persist } from 'zustand/middleware';

interface WorkspaceLayout {
  name: string;
  config: Record<string, unknown>;
  timestamp: number;
}

interface WorkspaceState {
  layouts: WorkspaceLayout[];
  currentLayout: string | null;
  saveLayout: (name: string, config: Record<string, unknown>) => void;
  loadLayout: (name: string) => WorkspaceLayout | undefined;
  deleteLayout: (name: string) => void;
  getLayouts: () => WorkspaceLayout[];
}

export const useWorkspaceStore = create<WorkspaceState>()(
  persist(
    (set, get) => ({
      layouts: [],
      currentLayout: null,

      saveLayout: (name: string, config: Record<string, unknown>) => {
        set((state) => {
          const existingIndex = state.layouts.findIndex((l) => l.name === name);
          const newLayout: WorkspaceLayout = {
            name,
            config,
            timestamp: Date.now(),
          };

          if (existingIndex >= 0) {
            const updatedLayouts = [...state.layouts];
            updatedLayouts[existingIndex] = newLayout;
            return { layouts: updatedLayouts, currentLayout: name };
          }

          return {
            layouts: [...state.layouts, newLayout],
            currentLayout: name,
          };
        });
      },

      loadLayout: (name: string) => {
        const layout = get().layouts.find((l) => l.name === name);
        if (layout) {
          set({ currentLayout: name });
        }
        return layout;
      },

      deleteLayout: (name: string) => {
        set((state) => ({
          layouts: state.layouts.filter((l) => l.name !== name),
          currentLayout: state.currentLayout === name ? null : state.currentLayout,
        }));
      },

      getLayouts: () => get().layouts,
    }),
    {
      name: 'workspace-storage',
    }
  )
);

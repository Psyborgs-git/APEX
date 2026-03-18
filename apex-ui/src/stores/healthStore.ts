import { create } from 'zustand';
import type { SystemHealthDto } from '../lib/types';

interface HealthState {
  health: SystemHealthDto | null;
  setHealth: (health: SystemHealthDto) => void;
}

export const useHealthStore = create<HealthState>((set) => ({
  health: null,
  setHealth: (health) => set({ health }),
}));

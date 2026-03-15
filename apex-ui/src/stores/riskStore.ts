import { create } from 'zustand';
import type { RiskStatusDto } from '../lib/types';

interface RiskState {
  status: RiskStatusDto;
  setStatus: (status: RiskStatusDto) => void;
}

export const useRiskStore = create<RiskState>((set) => ({
  status: {
    session_pnl: 0,
    is_halted: false,
    max_daily_loss: 50000,
  },
  setStatus: (status: RiskStatusDto) => set({ status }),
}));

import { create } from 'zustand';
import type { AlertDto, NewsItemDto } from '../lib/types';

const MAX_NEWS = 200;
const MAX_ALERTS = 100;

interface NewsState {
  items: NewsItemDto[];
  firedAlerts: AlertDto[];
  addItem: (item: NewsItemDto) => void;
  setItems: (items: NewsItemDto[]) => void;
  addFiredAlert: (alert: AlertDto) => void;
  clearFiredAlerts: () => void;
}

export const useNewsStore = create<NewsState>()((set) => ({
  items: [],
  firedAlerts: [],

  addItem: (item: NewsItemDto) =>
    set((state) => {
      if (state.items.some((existing) => existing.id === item.id)) {
        return state;
      }
      return { items: [item, ...state.items].slice(0, MAX_NEWS) };
    }),

  setItems: (items: NewsItemDto[]) => set({ items: items.slice(0, MAX_NEWS) }),

  addFiredAlert: (alert: AlertDto) =>
    set((state) => ({ firedAlerts: [alert, ...state.firedAlerts].slice(0, MAX_ALERTS) })),

  clearFiredAlerts: () => set({ firedAlerts: [] }),
}));

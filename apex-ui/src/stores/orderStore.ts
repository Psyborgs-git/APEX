import { create } from 'zustand';
import type { OrderDto, PositionDto, AccountBalanceDto } from '../lib/types';

interface OrderState {
  openOrders: OrderDto[];
  positions: PositionDto[];
  accountBalance: AccountBalanceDto | null;
  updateOrder: (order: OrderDto) => void;
  setOrders: (orders: OrderDto[]) => void;
  setPositions: (positions: PositionDto[]) => void;
  setAccountBalance: (balance: AccountBalanceDto) => void;
}

export const useOrderStore = create<OrderState>((set) => ({
  openOrders: [],
  positions: [],
  accountBalance: null,

  updateOrder: (order: OrderDto) => {
    set((state) => ({
      openOrders: state.openOrders.map((o) =>
        o.id === order.id ? order : o
      ),
    }));
  },

  setOrders: (orders: OrderDto[]) => set({ openOrders: orders }),
  setPositions: (positions: PositionDto[]) => set({ positions }),
  setAccountBalance: (balance: AccountBalanceDto) => set({ accountBalance: balance }),
}));

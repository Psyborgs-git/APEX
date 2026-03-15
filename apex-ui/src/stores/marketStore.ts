import { create } from 'zustand';
import type { QuoteDto } from '../lib/types';

interface MarketState {
  quotes: Map<string, QuoteDto>;
  watchlist: string[];
  updateQuote: (quote: QuoteDto) => void;
  setWatchlist: (symbols: string[]) => void;
  getQuote: (symbol: string) => QuoteDto | undefined;
}

export const useMarketStore = create<MarketState>((set, get) => ({
  quotes: new Map(),
  watchlist: ['RELIANCE.NS', 'TCS.NS', 'HDFCBANK.NS', 'INFY.NS', 'AAPL', 'MSFT', 'GOOGL'],

  updateQuote: (quote: QuoteDto) => {
    set((state) => {
      const newQuotes = new Map(state.quotes);
      newQuotes.set(quote.symbol, quote);
      return { quotes: newQuotes };
    });
  },

  setWatchlist: (symbols: string[]) => set({ watchlist: symbols }),

  getQuote: (symbol: string) => get().quotes.get(symbol),
}));

import { create } from 'zustand';
import type { MLModelDto } from '../lib/types';

interface MLState {
  models: MLModelDto[];
  isTraining: boolean;
  trainingError: string | null;
  setModels: (models: MLModelDto[]) => void;
  addModel: (model: MLModelDto) => void;
  removeModel: (modelId: string) => void;
  setTraining: (training: boolean) => void;
  setTrainingError: (error: string | null) => void;
  updateModelStatus: (modelId: string, status: MLModelDto['status']) => void;
}

export const useMLStore = create<MLState>((set) => ({
  models: [],
  isTraining: false,
  trainingError: null,
  setModels: (models) => set({ models }),
  addModel: (model) => set((state) => ({ models: [...state.models, model] })),
  removeModel: (modelId) =>
    set((state) => ({ models: state.models.filter((m) => m.model_id !== modelId) })),
  setTraining: (training) => set({ isTraining: training }),
  setTrainingError: (error) => set({ trainingError: error }),
  updateModelStatus: (modelId, status) =>
    set((state) => ({
      models: state.models.map((m) =>
        m.model_id === modelId ? { ...m, status } : m,
      ),
    })),
}));

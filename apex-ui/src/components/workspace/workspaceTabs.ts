export const VALID_TABS = [
  'chart',
  'market',
  'strategy',
  'ml',
  'data',
  'notebook',
  'health',
  'news',
  'blotter',
  'book',
  'graph',
  'scanner',
  'copilot',
] as const

export type CenterTab = (typeof VALID_TABS)[number]

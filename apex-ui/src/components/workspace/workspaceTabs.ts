export const VALID_TABS = ['chart', 'strategy', 'ml', 'data', 'notebook', 'health'] as const

export type CenterTab = (typeof VALID_TABS)[number]

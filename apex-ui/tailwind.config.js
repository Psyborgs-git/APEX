/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      fontFamily: {
        sans: ['IBM Plex Sans', 'sans-serif'],
        mono: ['JetBrains Mono', 'monospace'],
      },
      colors: {
        surface: {
          0: 'var(--surface-0)',
          1: 'var(--surface-1)',
          2: 'var(--surface-2)',
          3: 'var(--surface-3)',
        },
        // RGB-triplet vars so `color/<alpha>` modifiers (bg-accent/15, border-bull/30) compile.
        bull: 'rgb(var(--color-bull-rgb) / <alpha-value>)',
        bear: 'rgb(var(--color-bear-rgb) / <alpha-value>)',
        accent: 'rgb(var(--color-accent-rgb) / <alpha-value>)',
        muted: 'rgb(var(--color-muted-rgb) / <alpha-value>)',
        warning: 'rgb(var(--color-warning-rgb) / <alpha-value>)',
        error: 'rgb(var(--color-error-rgb) / <alpha-value>)',
        success: 'rgb(var(--color-success-rgb) / <alpha-value>)',
        // Legacy alias used across panels — maps to the themed accent.
        primary: {
          400: 'rgb(var(--color-accent-soft-rgb) / <alpha-value>)',
          500: 'rgb(var(--color-accent-rgb) / <alpha-value>)',
          600: 'rgb(var(--color-accent-strong-rgb) / <alpha-value>)',
        },
        // Foreground that stays readable on top of bull/bear/accent fills.
        'on-bull': 'var(--color-on-bull)',
        'on-bear': 'var(--color-on-bear)',
        'on-accent': 'var(--color-on-accent)',
        text: {
          primary: 'var(--text-primary)',
          secondary: 'var(--text-secondary)',
          muted: 'var(--text-muted)',
        },
        border: 'var(--border-color)',
      },
    },
  },
  plugins: [],
};

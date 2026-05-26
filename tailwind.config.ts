import type { Config } from 'tailwindcss';

export default {
  content: ['./index.html', './src/**/*.{svelte,ts}'],
  theme: {
    extend: {
      fontFamily: {
        sans: ['"Plus Jakarta Sans"', 'ui-sans-serif', 'system-ui', 'sans-serif'],
        display: ['"Plus Jakarta Sans"', 'ui-sans-serif', 'system-ui', 'sans-serif']
      },
      colors: {
        ink: '#0b0c0f',
        panel: '#14171d',
        line: '#262b35',
        paper: '#f4f0e8',
        coral: '#ff6b5f',
        mint: '#54d6a4',
        brass: '#d6aa5a',
        tide: '#62b7d9'
      },
      boxShadow: {
        player: '0 24px 80px rgba(0, 0, 0, 0.38)'
      }
    }
  },
  plugins: []
} satisfies Config;

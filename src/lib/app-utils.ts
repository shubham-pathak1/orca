import { convertFileSrc } from '@tauri-apps/api/core';

const CUSTOM_FONT_STYLE_ID = 'orca-custom-font-face';
const DEFAULT_ACCENT_RGB = '245,245,245';

export function readStoredString<T extends string>(key: string, fallback: T, allowed: T[]): T {
  const value = window.localStorage.getItem(key);
  return allowed.includes(value as T) ? (value as T) : fallback;
}

export function fontStack(value: string) {
  if (value === 'System') return 'ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, sans-serif';
  if (value === 'Plus Jakarta Sans') return '"Plus Jakarta Sans", ui-sans-serif, system-ui, sans-serif';
  if (value.startsWith('file:')) return '"OrcaCustomFont", ui-sans-serif, system-ui, sans-serif';
  return `"${value}", ui-sans-serif, system-ui, sans-serif`;
}

export function syncCustomFont(fontFamily: string) {
  document.getElementById(CUSTOM_FONT_STYLE_ID)?.remove();
  if (!fontFamily.startsWith('file:')) return;

  const style = document.createElement('style');
  style.id = CUSTOM_FONT_STYLE_ID;
  style.textContent = `@font-face { font-family: 'OrcaCustomFont'; src: url('${convertFileSrc(fontFamily.slice(5))}'); font-display: swap; }`;
  document.head.appendChild(style);
}

export function applyRootFontSize(value: number) {
  if (typeof document !== 'undefined') {
    document.documentElement.style.fontSize = `${16 * (value / 100)}px`;
  }
}

export async function sampleArtworkAccent(src: string) {
  const image = new Image();
  image.crossOrigin = 'anonymous';
  image.src = src;
  await image.decode();

  const canvas = document.createElement('canvas');
  canvas.width = 48;
  canvas.height = 48;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  if (!context) return DEFAULT_ACCENT_RGB;

  context.drawImage(image, 0, 0, canvas.width, canvas.height);
  const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
  let redTotal = 0;
  let greenTotal = 0;
  let blueTotal = 0;
  let count = 0;

  for (let index = 0; index < pixels.length; index += 16) {
    const red = pixels[index];
    const green = pixels[index + 1];
    const blue = pixels[index + 2];
    const brightness = (red + green + blue) / 3;
    const saturation = Math.max(red, green, blue) - Math.min(red, green, blue);

    if (saturation > 18 && brightness > 34 && brightness < 232) {
      redTotal += red;
      greenTotal += green;
      blueTotal += blue;
      count += 1;
    }
  }

  if (count === 0) return DEFAULT_ACCENT_RGB;
  return `${Math.round(redTotal / count)},${Math.round(greenTotal / count)},${Math.round(blueTotal / count)}`;
}

import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export const formatTime = (totalSeconds: number) => {
  const minutes = Math.floor(totalSeconds / 60).toString().padStart(2, '0');
  const seconds = (totalSeconds % 60).toString().padStart(2, '0');
  return `${minutes}:${seconds}`;
};

/** Modifier key shown in UI for command palette (matches App.tsx handler). */
export function searchModKeyLabel(): string {
  if (typeof navigator === 'undefined') return 'Alt';
  const ua = navigator.userAgent || '';
  const platform = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform
    || navigator.platform
    || '';
  if (/Mac|iPhone|iPad|iPod/i.test(platform) || /Mac OS X/i.test(ua)) {
    return '⌘';
  }
  return 'Alt';
}

/** Prefer reduced motion for subtle UI transitions. */
export function prefersReducedMotion(): boolean {
  if (typeof window === 'undefined' || !window.matchMedia) return false;
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}
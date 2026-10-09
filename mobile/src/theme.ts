/**
 * Systeme visuel de l'application, le meme que l'interface web (web/src/index.css) :
 * fond encre, texte ivoire, accent laiton ; serif pour le nom et les titres.
 * Quatre etats de parole assourdis : ecoute (sarcelle), reflexion (violet),
 * parole (laiton), veille (ivoire eteinte).
 */
import { Platform } from 'react-native';

export const colors = {
  ink950: '#0b0e14',
  ink900: '#10141c',
  ink850: '#151a24',
  ink800: '#1c2230',
  ink700: '#2a3242',
  ink600: '#3a4458',
  /** Contour des composants interactifs : 3:1 sur le fond (WCAG 1.4.11). */
  ink500: '#5b6780',

  ivory50: '#faf7f0',
  ivory100: '#f1ece0',
  ivory300: '#cdc6b6',
  ivory500: '#928b7c',
  ivory700: '#7a7466',

  brass200: '#efd9a4',
  brass300: '#e3c27a',
  brass400: '#cfa552',
  brass500: '#b38a3c',
  brass700: '#6f5424',

  listen300: '#8fdcd2',
  listen400: '#5fc7bb',
  listen600: '#2b8c82',

  think300: '#c5b6f2',
  think400: '#a08fe4',
  think600: '#6a5bb8',

  alert300: '#f3c77a',
  alert400: '#e7ab4a',
  alert600: '#9a6d22',

  danger300: '#f1a0a0',
  danger400: '#e07070',
  danger600: '#9a3f3f',

  ok300: '#a9d9b0',
  ok400: '#7cc08a',
  ok600: '#3f7f4e',
} as const;

/** Polices systeme : serif de l'appareil pour le nom et les titres, monospace pour les identifiants. */
export const fonts = {
  display: Platform.OS === 'android' ? 'serif' : 'Georgia',
  mono: Platform.OS === 'android' ? 'monospace' : 'Menlo',
} as const;

export const radius = { card: 14, field: 10, pill: 999 } as const;

/** Translucidites usuelles, pour ne pas recalculer des rgba a la main. */
export const alpha = {
  brass: (a: number) => `rgba(207, 165, 82, ${a})`,
  listen: (a: number) => `rgba(95, 199, 187, ${a})`,
  think: (a: number) => `rgba(160, 143, 228, ${a})`,
  alert: (a: number) => `rgba(231, 171, 74, ${a})`,
  danger: (a: number) => `rgba(224, 112, 112, ${a})`,
  ok: (a: number) => `rgba(124, 192, 138, ${a})`,
  ink: (a: number) => `rgba(16, 20, 28, ${a})`,
} as const;

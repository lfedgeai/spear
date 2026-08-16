import { useEffect, useState } from 'react'

export type ThemeMode = 'light' | 'dark'

const THEME_STORAGE_KEYS = ['SP_THEME', 'ADMIN_THEME', 'cw-theme'] as const

function isThemeMode(value: string | null): value is ThemeMode {
  return value === 'light' || value === 'dark'
}

function resolveInitialThemeMode(defaultMode: ThemeMode): ThemeMode {
  for (const key of THEME_STORAGE_KEYS) {
    const value = window.localStorage.getItem(key)
    if (isThemeMode(value)) return value
  }
  return defaultMode
}

function applyThemeMode(mode: ThemeMode) {
  const root = document.documentElement
  root.dataset.theme = mode
  root.classList.toggle('dark', mode === 'dark')
  root.classList.toggle('light', mode === 'light')
  root.style.colorScheme = mode
}

function persistThemeMode(mode: ThemeMode) {
  for (const key of THEME_STORAGE_KEYS) {
    window.localStorage.setItem(key, mode)
  }
}

/**
 * Shared theme mode hook for Admin and Console.
 * Admin 与 Console 共用的主题模式 Hook。
 */
export function useSharedThemeMode(defaultMode: ThemeMode = 'light') {
  const [mode, setMode] = useState<ThemeMode>(() => resolveInitialThemeMode(defaultMode))

  useEffect(() => {
    applyThemeMode(mode)
    persistThemeMode(mode)
  }, [mode])

  return { mode, setMode }
}

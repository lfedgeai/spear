import { useCallback, useMemo } from 'react'
import {
  useSharedThemeMode,
  type ThemeMode,
} from '../../../web-admin/src/shared/theme-mode'

export type Theme = ThemeMode

export function useTheme() {
  const { mode: theme, setMode: setTheme } = useSharedThemeMode('light')

  const toggleTheme = useCallback(() => {
    setTheme((t) => (t === 'dark' ? 'light' : 'dark'))
  }, [])

  const label = useMemo(() => (theme === 'dark' ? 'Dark' : 'Light'), [theme])

  return { theme, setTheme, toggleTheme, label }
}

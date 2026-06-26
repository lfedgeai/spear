import { useCallback, useEffect, useMemo, useState } from 'react'

export type Theme = 'dark' | 'light'

function initialTheme(): Theme {
  const saved = window.localStorage.getItem('cw-theme')
  if (saved === 'dark' || saved === 'light') return saved
  if (window.matchMedia?.('(prefers-color-scheme: light)')?.matches) return 'light'
  return 'dark'
}

export function useTheme() {
  const [theme, setTheme] = useState<Theme>(() => initialTheme())

  useEffect(() => {
    document.documentElement.dataset.theme = theme
    window.localStorage.setItem('cw-theme', theme)
  }, [theme])

  const toggleTheme = useCallback(() => {
    setTheme((t) => (t === 'dark' ? 'light' : 'dark'))
  }, [])

  const label = useMemo(() => (theme === 'dark' ? 'Dark' : 'Light'), [theme])

  return { theme, setTheme, toggleTheme, label }
}


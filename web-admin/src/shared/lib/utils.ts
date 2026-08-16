export function cn(...inputs: Array<unknown>) {
  return inputs
    .flatMap((input) => {
      if (!input) return []
      if (typeof input === 'string') return [input]
      if (Array.isArray(input)) return input.filter(Boolean).map(String)
      if (typeof input === 'object') {
        return Object.entries(input as Record<string, unknown>)
          .filter(([, value]) => Boolean(value))
          .map(([key]) => key)
      }
      return [String(input)]
    })
    .join(' ')
}

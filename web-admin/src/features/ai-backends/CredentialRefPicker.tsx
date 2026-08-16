import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'

import { listCredentials } from '@/api/credentials'

export default function CredentialRefPicker(props: {
  value: string
  onChange: (value: string) => void
  providerKind?: string
}) {
  const query = useQuery({
    queryKey: ['ai-credentials'],
    queryFn: listCredentials,
    staleTime: 30_000,
  })

  const credentials = useMemo(() => {
    const rows = query.data?.credentials || []
    return [...rows].sort((a, b) => a.name.localeCompare(b.name))
  }, [query.data?.credentials])

  return (
    <div className="space-y-1">
      <select
        className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
        value={props.value}
        onChange={(event) => props.onChange(event.target.value)}
      >
        <option value="">No credential</option>
        {credentials.map((credential) => (
          <option key={credential.name} value={credential.name}>
            {credential.name}
            {credential.disabled ? ' (disabled)' : ''}
          </option>
        ))}
      </select>
      {query.isLoading ? (
        <div className="text-xs text-[hsl(var(--muted-foreground))]">Loading credentials…</div>
      ) : query.isError || query.data?.success === false ? (
        <div className="text-xs text-[hsl(var(--destructive))]">
          {query.data?.message || 'Failed to load credentials'}
        </div>
      ) : credentials.length === 0 ? (
        <div className="text-xs text-[hsl(var(--muted-foreground))]">
          No credentials found.
        </div>
      ) : props.providerKind?.trim() ? (
        <div className="text-xs text-[hsl(var(--muted-foreground))]">
          Credentials are shared refs and may be reused across backend providers.
        </div>
      ) : null}
    </div>
  )
}

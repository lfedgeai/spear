import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Plus, RefreshCw, Search } from 'lucide-react'
import { toast } from 'sonner'

import { deleteCredential, listCredentials, type CredentialInfo } from '@/api/credentials'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import CredentialEditorDialog from '@/features/ai-models/CredentialEditorDialog'

function formatTimestamp(ts: number) {
  if (!ts) return '-'
  return new Date(ts).toLocaleString()
}

export default function CredentialsPanel() {
  const [queryText, setQueryText] = useState('')
  const [status, setStatus] = useState<'all' | 'enabled' | 'disabled'>('all')
  const [createOpen, setCreateOpen] = useState(false)
  const [editingCredential, setEditingCredential] = useState<CredentialInfo | null>(null)

  const query = useQuery({
    queryKey: ['credentials'],
    queryFn: () => listCredentials(),
    refetchInterval: 15_000,
  })

  const credentials = query.data?.credentials || []
  const filteredCredentials = useMemo(() => {
    const needle = queryText.trim().toLowerCase()
    return [...credentials]
      .filter((credential) => {
        if (status === 'enabled' && credential.disabled) return false
        if (status === 'disabled' && !credential.disabled) return false
        if (!needle) return true
        return [
          credential.name,
          credential.provider_kind,
          credential.description || '',
        ].some((value) => value.toLowerCase().includes(needle))
      })
      .sort((left, right) => right.updated_at_ms - left.updated_at_ms)
  }, [credentials, queryText, status])
  const stats = useMemo(() => {
    const total = credentials.length
    const enabled = credentials.filter((credential) => !credential.disabled).length
    const disabled = total - enabled
    return { total, enabled, disabled }
  }, [credentials])

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-semibold">Credentials</div>
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            Manage reusable secret references for remote AI backends.
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="secondary" onClick={() => setCreateOpen(true)}>
            <Plus className="h-4 w-4" />
            Create
          </Button>
          <Button
            variant="secondary"
            onClick={async () => {
              try {
                await query.refetch()
                toast.success('Credentials refreshed')
              } catch (e) {
                toast.error((e as Error).message)
              }
            }}
          >
            <RefreshCw className="h-4 w-4" />
            Refresh
          </Button>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">Total Credentials</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-semibold">{stats.total}</div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">Enabled</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-semibold text-emerald-600 dark:text-emerald-400">
              {stats.enabled}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">Disabled</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-semibold text-amber-600 dark:text-amber-400">
              {stats.disabled}
            </div>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Credential Registry</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-col gap-2 md:flex-row">
            <div className="relative w-full max-w-xl">
              <Search className="absolute left-2 top-2.5 h-4 w-4 text-[hsl(var(--muted-foreground))]" />
              <Input
                placeholder="Search by name, kind, or description"
                value={queryText}
                onChange={(e) => setQueryText(e.target.value)}
                className="pl-8"
              />
            </div>
            <select
              className="h-9 rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
              value={status}
              onChange={(e) => setStatus(e.target.value as typeof status)}
              aria-label="Credential status"
            >
              <option value="all">All statuses</option>
              <option value="enabled">Enabled</option>
              <option value="disabled">Disabled</option>
            </select>
          </div>

          {query.isError ? (
            <div className="text-sm text-[hsl(var(--destructive))]">
              {(query.error as Error).message}
            </div>
          ) : null}

          {filteredCredentials.length === 0 ? (
            <div className="rounded-[var(--radius)] border border-dashed border-[hsl(var(--border))] bg-[hsl(var(--background))] px-4 py-8 text-center">
              <div className="text-sm font-medium">No credentials found</div>
              <div className="mt-1 text-sm text-[hsl(var(--muted-foreground))]">
                Create a credential to reuse API keys across remote backend definitions.
              </div>
            </div>
          ) : (
            <div className="overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))]">
              <table className="w-full text-sm">
                <thead className="bg-[hsl(var(--secondary))]">
                  <tr className="text-left">
                    <th className="px-3 py-2">Name</th>
                    <th className="px-3 py-2">Kind</th>
                    <th className="px-3 py-2">Version</th>
                    <th className="px-3 py-2">Referenced By</th>
                    <th className="px-3 py-2">Updated</th>
                    <th className="px-3 py-2">Description</th>
                    <th className="px-3 py-2">Status</th>
                    <th className="px-3 py-2">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {filteredCredentials.map((credential) => (
                    <tr key={credential.name} className="border-t border-[hsl(var(--border))]">
                      <td className="px-3 py-2 font-medium">{credential.name}</td>
                      <td className="px-3 py-2">{credential.provider_kind}</td>
                      <td className="px-3 py-2">{credential.version}</td>
                      <td className="px-3 py-2 text-[hsl(var(--muted-foreground))]">
                        {credential.referenced_by_count || 0}
                      </td>
                      <td className="px-3 py-2 text-[hsl(var(--muted-foreground))]">
                        {formatTimestamp(credential.updated_at_ms)}
                      </td>
                      <td className="px-3 py-2 text-[hsl(var(--muted-foreground))]">
                        {credential.description || '-'}
                      </td>
                      <td className="px-3 py-2">
                        <Badge variant={credential.disabled ? 'destructive' : 'success'}>
                          {credential.disabled ? 'disabled' : 'enabled'}
                        </Badge>
                      </td>
                      <td className="px-3 py-2">
                        <div className="flex items-center gap-2">
                          <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => setEditingCredential(credential)}
                          >
                            Edit
                          </Button>
                          <Button
                            variant="destructive"
                            size="sm"
                            onClick={async () => {
                              try {
                                const ok = window.confirm(
                                  `Delete credential ${credential.name}? This will fail if it is still referenced by a backend.`,
                                )
                                if (!ok) return
                                const resp = await deleteCredential(credential.name)
                                if (!resp.success) throw new Error(resp.message || 'Delete failed')
                                toast.success('Credential deleted')
                                await query.refetch()
                              } catch (e) {
                                toast.error((e as Error).message)
                              }
                            }}
                          >
                            Delete
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>

      <CredentialEditorDialog
        open={createOpen}
        mode="create"
        onOpenChange={setCreateOpen}
        onSaved={async () => {
          await query.refetch()
        }}
      />

      <CredentialEditorDialog
        open={!!editingCredential}
        mode="edit"
        credential={editingCredential}
        onOpenChange={(open) => {
          if (!open) setEditingCredential(null)
        }}
        onSaved={async () => {
          await query.refetch()
          setEditingCredential(null)
        }}
      />
    </div>
  )
}

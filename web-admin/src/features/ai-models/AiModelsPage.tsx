import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Plus, Search } from 'lucide-react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { toast } from 'sonner'

import { listAiModels } from '@/api/ai-models'
import { deleteNodeModelDeployment, listNodeModelDeployments } from '@/api/model-deployments'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import CreateLocalBackendDialog from '@/features/ai-models/CreateLocalBackendDialog'
import CreateRemoteBackendDialog from '@/features/ai-models/CreateRemoteBackendDialog'
import LocalDeploymentsPanel from '@/features/ai-models/LocalDeploymentsPanel'

function StatusBadge({ status }: { status: 'available' | 'unavailable' }) {
  if (status === 'available') return <Badge variant="success">{status}</Badge>
  return <Badge variant="destructive">{status}</Badge>
}

export default function AiModelsPage({ hosting }: { hosting: 'local' | 'remote' }) {
  const navigate = useNavigate()
  const [searchParams, setSearchParams] = useSearchParams()
  const [q, setQ] = useState('')
  const [status, setStatus] = useState<'available' | 'unavailable' | ''>('')
  const [createOpen, setCreateOpen] = useState(false)
  const [createRemoteOpen, setCreateRemoteOpen] = useState(false)
  const focusDeploymentId = useMemo(
    () => searchParams.get('deployment_id') || '',
    [searchParams],
  )

  const query = useQuery({
    queryKey: ['ai-models', hosting, q, status],
    queryFn: () =>
      listAiModels({
        hosting,
        q: q || undefined,
        limit: 500,
        offset: 0,
      }),
    refetchInterval: 15_000,
  })

  const rawRows = useMemo(() => query.data?.models || [], [query.data?.models])
  const rows = useMemo(() => {
    if (!status) return rawRows
    return rawRows.filter((m) =>
      status === 'available' ? m.available_nodes > 0 : m.available_nodes === 0,
    )
  }, [rawRows, status])
  const total = query.data?.total_count ?? rows.length
  const title = useMemo(
    () => `AI Models (${rows.length}/${total})`,
    [rows.length, total],
  )

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-semibold">AI Models</div>
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            {hosting === 'local' ? 'Local' : 'Remote'}
          </div>
        </div>
        <div className="flex items-center gap-2">
          {hosting === 'local' ? (
            <Button variant="secondary" onClick={() => setCreateOpen(true)}>
              <Plus className="h-4 w-4" />
              Create
            </Button>
          ) : (
            <Button variant="secondary" onClick={() => setCreateRemoteOpen(true)}>
              <Plus className="h-4 w-4" />
              Create
            </Button>
          )}
          <Button
            variant="secondary"
            onClick={() => {
              query
                .refetch()
                .then(() => toast.success('Refreshed'))
                .catch((e) => toast.error((e as Error).message))
            }}
          >
            Refresh
          </Button>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{title}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="mb-3 flex items-center gap-2">
            <div className="relative w-full max-w-md">
              <Search className="absolute left-2 top-2.5 h-4 w-4 text-[hsl(var(--muted-foreground))]" />
              <Input
                placeholder="Search provider/model"
                value={q}
                onChange={(e) => setQ(e.target.value)}
                className="pl-8"
              />
            </div>
            <select
              className="h-9 rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
              value={status}
              onChange={(e) => setStatus(e.target.value as typeof status)}
              aria-label="Status"
            >
              <option value="">All</option>
              <option value="available">available</option>
              <option value="unavailable">unavailable</option>
            </select>
          </div>

          {query.isError ? (
            <div className="text-sm text-[hsl(var(--destructive))]">
              {(query.error as Error).message}
            </div>
          ) : null}

          <div className="overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))]">
            <table className="w-full text-sm">
              <thead className="bg-[hsl(var(--secondary))]">
                <tr className="text-left">
                  <th className="px-3 py-2">Provider</th>
                  <th className="px-3 py-2">Model</th>
                  <th className="px-3 py-2">Ops</th>
                  <th className="px-3 py-2">Transports</th>
                  <th className="px-3 py-2">Available</th>
                  {hosting === 'local' ? <th className="px-3 py-2">Actions</th> : null}
                </tr>
              </thead>
              <tbody>
                {rows.map((m) => (
                  <tr
                    key={`${m.hosting}::${m.provider}::${m.model}`}
                    className="cursor-pointer border-t border-[hsl(var(--border))] hover:bg-[hsl(var(--accent))]"
                    onClick={() => {
                      navigate(
                        `/ai-models/${encodeURIComponent(hosting)}/${encodeURIComponent(
                          m.provider,
                        )}/${encodeURIComponent(m.model)}`,
                      )
                    }}
                  >
                    <td className="px-3 py-2 font-medium">{m.provider}</td>
                    <td className="px-3 py-2 font-mono text-xs">{m.model}</td>
                    <td className="px-3 py-2">
                      <div className="flex flex-wrap gap-1">
                        {(m.operations || []).slice(0, 6).map((op) => (
                          <Badge key={op} variant="secondary">
                            {op}
                          </Badge>
                        ))}
                        {(m.operations || []).length > 6 ? (
                          <Badge variant="secondary">...</Badge>
                        ) : null}
                      </div>
                    </td>
                    <td className="px-3 py-2">
                      <div className="flex flex-wrap gap-1">
                        {(m.transports || []).map((t) => (
                          <Badge key={t} variant="secondary">
                            {t}
                          </Badge>
                        ))}
                      </div>
                    </td>
                    <td className="px-3 py-2">
                      <div className="flex items-center gap-2">
                        <StatusBadge
                          status={m.available_nodes > 0 ? 'available' : 'unavailable'}
                        />
                        <span className="text-[hsl(var(--muted-foreground))]">
                          {m.available_nodes}/{m.total_nodes}
                        </span>
                      </div>
                    </td>
                    {hosting === 'local' ? (
                      <td className="px-3 py-2">
                        <Button
                          variant="destructive"
                          size="sm"
                          onClick={async (e) => {
                            e.preventDefault()
                            e.stopPropagation()
                            const nodeUuids = Array.from(
                              new Set((m.instances || []).map((i) => i.node_uuid).filter(Boolean)),
                            )
                            if (nodeUuids.length === 0) {
                              toast.error('No node instances found for this model')
                              return
                            }
                            const ok = window.confirm(
                              `Delete local deployment(s) for ${m.provider}/${m.model} on ${nodeUuids.length} node(s)?`,
                            )
                            if (!ok) return

                            try {
                              let deleted = 0
                              for (const node_uuid of nodeUuids) {
                                const resp = await listNodeModelDeployments({
                                  node_uuid,
                                  limit: 500,
                                  offset: 0,
                                })
                                if (!resp.success) {
                                  throw new Error(resp.message || 'List deployments failed')
                                }
                                const targets = (resp.deployments || []).filter(
                                  (d) =>
                                    (d.spec?.provider || '') === m.provider &&
                                    (d.spec?.model || '') === m.model,
                                )
                                for (const d of targets) {
                                  const del = await deleteNodeModelDeployment({
                                    node_uuid,
                                    deployment_id: d.deployment_id,
                                  })
                                  if (!del.success) {
                                    throw new Error(del.message || 'Delete failed')
                                  }
                                  deleted += 1
                                }
                              }

                              if (deleted === 0) {
                                toast.error(
                                  'No matching deployments found (already deleted?)',
                                )
                              } else {
                                toast.success(`Deleted ${deleted} deployment(s)`)
                              }
                              await query.refetch()
                            } catch (err) {
                              toast.error((err as Error).message)
                            }
                          }}
                        >
                          Delete
                        </Button>
                      </td>
                    ) : null}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </CardContent>
      </Card>

      {hosting === 'local' ? (
        <LocalDeploymentsPanel
          focusDeploymentId={focusDeploymentId || undefined}
          defaultOpen={!!focusDeploymentId}
          onClearFocus={() => {
            const sp = new URLSearchParams(searchParams.toString())
            sp.delete('deployment_id')
            setSearchParams(sp, { replace: true })
          }}
        />
      ) : null}

      {hosting === 'local' ? (
        <CreateLocalBackendDialog
          open={createOpen}
          onOpenChange={setCreateOpen}
          onCreated={async () => {
            await query.refetch()
          }}
          onCreatedDeployment={(info: { deployment_id: string; node_uuid: string }) => {
            const sp = new URLSearchParams(searchParams.toString())
            sp.set('deployment_id', info.deployment_id)
            setSearchParams(sp, { replace: true })
          }}
        />
      ) : null}

      {hosting === 'remote' ? (
        <Card>
          <CardHeader>
            <CardTitle>Credential References</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
            <div className="text-sm text-[hsl(var(--muted-foreground))]">
              Manage reusable secrets in the dedicated Credentials page, then select them from
              remote backend forms.
            </div>
            <Button variant="secondary" onClick={() => navigate('/ai-models/credentials')}>
              Open Credentials
            </Button>
          </CardContent>
        </Card>
      ) : null}

      {hosting === 'remote' ? (
        <CreateRemoteBackendDialog
          open={createRemoteOpen}
          onOpenChange={setCreateRemoteOpen}
          onCreated={async () => {
            await query.refetch()
          }}
        />
      ) : null}
    </div>
  )
}

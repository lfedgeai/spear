import { Link, useParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { Copy } from 'lucide-react'
import { toast } from 'sonner'

import {
  listAiBackendPlacements,
  listAiBackends,
  listAiModelViews,
} from '@/api/ai-backends'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'

function StatusBadge({ status }: { status: 'available' | 'unavailable' }) {
  if (status === 'available') return <Badge variant="success">{status}</Badge>
  return <Badge variant="destructive">{status}</Badge>
}

function CopyButton({ value }: { value: string }) {
  return (
    <Button
      variant="secondary"
      size="sm"
      onClick={async () => {
        await navigator.clipboard.writeText(value)
        toast.success('Copied')
      }}
    >
      <Copy className="h-4 w-4" />
      Copy
    </Button>
  )
}

export default function AiModelDetailPage() {
  const { hosting, provider, model } = useParams()
  const h = (hosting || '') as 'local' | 'remote' | ''
  const p = provider || ''
  const m = model || ''

  const viewsQuery = useQuery({
    queryKey: ['ai-model-detail', h, p, m],
    queryFn: async () => {
      const [viewsResp, backendsResp, placementsResp] = await Promise.all([
        listAiModelViews(),
        listAiBackends(),
        listAiBackendPlacements(),
      ])

      if (!viewsResp.success) {
        throw new Error(viewsResp.message || 'Failed to load AI model views')
      }
      if (!backendsResp.success) {
        throw new Error(backendsResp.message || 'Failed to load AI backends')
      }
      if (!placementsResp.success) {
        throw new Error(placementsResp.message || 'Failed to load AI backend placements')
      }

      const view = (viewsResp.views || []).find(
        (item) =>
          item.provider === p &&
          item.model === m &&
          (!h || item.hosting === h),
      )

      return {
        view: view || null,
        backends: backendsResp.backends || [],
        placements: placementsResp.placements || [],
      }
    },
    enabled: !!p && !!m,
    refetchInterval: 15_000,
  })

  const view = viewsQuery.data?.view || null
  const backendsById = new Map((viewsQuery.data?.backends || []).map((backend) => [backend.backend_id, backend]))
  const placements = viewsQuery.data?.placements || []
  const title = view ? `${view.provider} / ${view.model}` : 'AI Model'

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-semibold">{title}</div>
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            <Link to={`/ai-models/${encodeURIComponent(h || 'remote')}`} className="hover:underline">
              AI Models
            </Link>
            <span className="mx-2">/</span>
            <span className="font-mono text-xs">{h || '-'}</span>
            <span className="mx-2">/</span>
            <span className="font-mono text-xs">{p}</span>
            <span className="mx-2">/</span>
            <span className="font-mono text-xs">{m}</span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="secondary" onClick={() => viewsQuery.refetch()}>
            Refresh
          </Button>
          {view ? <CopyButton value={`${view.provider}:${view.model}`} /> : null}
        </div>
      </div>

      {viewsQuery.isLoading ? (
        <div className="text-sm text-[hsl(var(--muted-foreground))]">Loading…</div>
      ) : viewsQuery.isError ? (
        <div className="text-sm text-[hsl(var(--muted-foreground))]">
          {(viewsQuery.error as Error).message || 'Failed to load.'}
        </div>
      ) : !view ? (
        <div className="text-sm text-[hsl(var(--muted-foreground))]">Not found</div>
      ) : (
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-3">
            <Card>
              <CardHeader>
                <CardTitle>Summary</CardTitle>
              </CardHeader>
              <CardContent className="space-y-2">
                <div className="flex items-center justify-between text-sm">
                  <span className="text-[hsl(var(--muted-foreground))]">Status</span>
                  <StatusBadge status={view.ready_nodes > 0 ? 'available' : 'unavailable'} />
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-[hsl(var(--muted-foreground))]">Ready</span>
                  <span>
                    {view.ready_nodes}/{view.total_nodes}
                  </span>
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-[hsl(var(--muted-foreground))]">Enabled</span>
                  <span>
                    {view.enabled_nodes}/{view.total_nodes}
                  </span>
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-[hsl(var(--muted-foreground))]">Hosting</span>
                  <span className="font-mono text-xs">{view.hosting}</span>
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-[hsl(var(--muted-foreground))]">Provider</span>
                  <span className="font-mono text-xs">{view.provider}</span>
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-[hsl(var(--muted-foreground))]">Model</span>
                  <span className="font-mono text-xs">{view.model}</span>
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Capabilities</CardTitle>
              </CardHeader>
              <CardContent className="space-y-3">
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Ops</div>
                  <div className="mt-1 flex flex-wrap gap-1">
                    {(view.operations || []).map((op) => (
                      <Badge key={op} variant="secondary">
                        {op}
                      </Badge>
                    ))}
                  </div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Transports</div>
                  <div className="mt-1 flex flex-wrap gap-1">
                    {(view.transports || []).map((t) => (
                      <Badge key={t} variant="secondary">
                        {t}
                      </Badge>
                    ))}
                  </div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Features</div>
                  <div className="mt-1 flex flex-wrap gap-1">
                    {(view.features || []).map((feature) => (
                      <Badge key={feature} variant="secondary">
                        {feature}
                      </Badge>
                    ))}
                  </div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Backend IDs</div>
                  <div className="mt-1 flex flex-wrap gap-1">
                    {view.backend_ids.map((backendId) => (
                      <Badge key={backendId} variant="secondary">
                        {backendId}
                      </Badge>
                    ))}
                  </div>
                </div>
              </CardContent>
            </Card>
          </div>

          <Card>
            <CardHeader>
              <CardTitle>Instances</CardTitle>
            </CardHeader>
            <CardContent>
              {view.instances && view.instances.length > 0 ? (
                <div className="overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))]">
                  <table className="w-full text-sm">
                    <thead className="bg-[hsl(var(--muted))] text-left text-xs text-[hsl(var(--muted-foreground))]">
                      <tr>
                        <th className="px-3 py-2">Node</th>
                        <th className="px-3 py-2">Runtime</th>
                        <th className="px-3 py-2">Backend ID</th>
                        <th className="px-3 py-2">Kind</th>
                        <th className="px-3 py-2">Placement</th>
                        <th className="px-3 py-2">Backend State</th>
                        <th className="px-3 py-2">Weight</th>
                        <th className="px-3 py-2">Priority</th>
                        <th className="px-3 py-2">Endpoint</th>
                        <th className="px-3 py-2">Runtime Backend</th>
                      </tr>
                    </thead>
                    <tbody>
                      {view.instances.map((row, idx) => {
                        const backend = backendsById.get(row.backend_id)
                        const placement = placements.find(
                          (item) => item.backend_id === row.backend_id && item.node_uuid === row.node_uuid,
                        )
                        return (
                          <tr key={`${row.node_uuid}-${idx}`} className="border-t border-[hsl(var(--border))]">
                            <td className="px-3 py-2 font-mono text-xs">
                              <Link to={`/nodes/${encodeURIComponent(row.node_uuid)}`} className="hover:underline">
                                {row.node_uuid}
                              </Link>
                            </td>
                            <td className="px-3 py-2">
                              <Badge variant={row.available ? 'success' : 'secondary'}>
                                {row.runtime_status || 'unspecified'}
                              </Badge>
                            </td>
                            <td className="px-3 py-2 font-mono text-xs">{row.backend_id}</td>
                            <td className="px-3 py-2 font-mono text-xs">{backend?.backend_kind || '-'}</td>
                            <td className="px-3 py-2 font-mono text-xs">{row.placement_state}</td>
                            <td className="px-3 py-2 font-mono text-xs">{row.backend_state}</td>
                            <td className="px-3 py-2 font-mono text-xs">
                              {placement?.weight_override ?? backend?.spec?.weight ?? '-'}
                            </td>
                            <td className="px-3 py-2 font-mono text-xs">
                              {placement?.priority_override ?? backend?.spec?.priority ?? '-'}
                            </td>
                            <td className="px-3 py-2 font-mono text-xs">{row.endpoint || '-'}</td>
                            <td className="px-3 py-2 text-xs text-[hsl(var(--muted-foreground))]">
                              {row.runtime_backend_name || '-'}
                            </td>
                          </tr>
                        )
                      })}
                    </tbody>
                  </table>
                </div>
              ) : (
                <div className="text-sm text-[hsl(var(--muted-foreground))]">No instances.</div>
              )}
            </CardContent>
          </Card>

          <div className="flex items-center justify-between">
            <div className="text-sm text-[hsl(var(--muted-foreground))]">Raw JSON</div>
          </div>
          <pre className="max-h-[520px] overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))] bg-[hsl(var(--secondary))] p-3 text-xs">
            {JSON.stringify(view, null, 2)}
          </pre>
        </div>
      )}
    </div>
  )
}

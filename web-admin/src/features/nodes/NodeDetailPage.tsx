import { Link, useParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { Copy } from 'lucide-react'
import { toast } from 'sonner'

import { getNodeCredentialSync, getNodeDetail } from '@/api/nodes'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'

function formatTs(ts: number) {
  if (!ts) return '-'
  const d = new Date(ts * 1000)
  return d.toLocaleString()
}

function formatMs(ts?: number | null) {
  if (!ts) return '-'
  return new Date(ts).toLocaleString()
}

function StatusBadge({ status }: { status: string }) {
  const s = (status || '').toLowerCase()
  if (s === 'online' || s === 'active') return <Badge variant="success">{status}</Badge>
  return <Badge variant="destructive">{status || 'unknown'}</Badge>
}

function CredentialSyncBadge({ status }: { status: string }) {
  const s = (status || '').toLowerCase()
  if (s === 'ready') return <Badge variant="success">{status}</Badge>
  if (s === 'syncing') return <Badge variant="secondary">{status}</Badge>
  return <Badge variant="destructive">{status || 'unknown'}</Badge>
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

export default function NodeDetailPage() {
  const { uuid } = useParams()
  const id = uuid || ''

  const q = useQuery({
    queryKey: ['node-detail', id],
    queryFn: () => getNodeDetail(id),
    enabled: !!id,
    refetchInterval: 15_000,
  })
  const credentialSyncQuery = useQuery({
    queryKey: ['node-credential-sync', id],
    queryFn: () => getNodeCredentialSync(id),
    enabled: !!id,
    refetchInterval: 15_000,
  })

  const node = q.data?.node
  const resource = q.data?.resource
  const credentialSync = credentialSyncQuery.data?.monitoring?.credential_sync

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-semibold">{node?.name || 'Node'}</div>
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            <Link to="/nodes" className="hover:underline">
              Nodes
            </Link>
            <span className="mx-2">/</span>
            <span className="font-mono text-xs">{id}</span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="secondary" onClick={() => q.refetch()}>
            Refresh
          </Button>
          {id ? <CopyButton value={id} /> : null}
        </div>
      </div>

      {q.isLoading ? (
        <div className="text-sm text-[hsl(var(--muted-foreground))]">Loading…</div>
      ) : q.isError ? (
        <div className="text-sm text-[hsl(var(--muted-foreground))]">
          Failed to load node detail.
        </div>
      ) : !q.data?.found ? (
        <div className="text-sm text-[hsl(var(--muted-foreground))]">Not found</div>
      ) : (
        <div className="space-y-4">
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>Summary</CardTitle>
              </CardHeader>
              <CardContent className="space-y-2">
                <div className="flex items-center justify-between text-sm">
                  <span className="text-[hsl(var(--muted-foreground))]">Status</span>
                  <StatusBadge status={node?.status || ''} />
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-[hsl(var(--muted-foreground))]">Address</span>
                  <span>
                    {node?.ip_address}:{node?.port}
                  </span>
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-[hsl(var(--muted-foreground))]">Registered</span>
                  <span>{node?.registered_at ? formatTs(node.registered_at) : '-'}</span>
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-[hsl(var(--muted-foreground))]">Last heartbeat</span>
                  <span>{node?.last_heartbeat ? formatTs(node.last_heartbeat) : '-'}</span>
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Resources</CardTitle>
              </CardHeader>
              <CardContent className="space-y-2">
                <div className="flex items-center justify-between text-sm">
                  <span className="text-[hsl(var(--muted-foreground))]">CPU</span>
                  <span>{resource?.cpu_usage_percent ?? '-'}%</span>
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-[hsl(var(--muted-foreground))]">Memory</span>
                  <span>{resource?.memory_usage_percent ?? '-'}%</span>
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-[hsl(var(--muted-foreground))]">Disk</span>
                  <span>{resource?.disk_usage_percent ?? '-'}%</span>
                </div>
              </CardContent>
            </Card>
          </div>

          <Card>
            <CardHeader>
              <CardTitle>Credential Sync</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              {credentialSyncQuery.isLoading ? (
                <div className="text-sm text-[hsl(var(--muted-foreground))]">Loading…</div>
              ) : credentialSyncQuery.isError ? (
                <div className="text-sm text-[hsl(var(--muted-foreground))]">
                  Failed to load credential sync status.
                </div>
              ) : !credentialSyncQuery.data?.success ? (
                <div className="text-sm text-[hsl(var(--muted-foreground))]">
                  {credentialSyncQuery.data?.message || 'Credential sync status unavailable.'}
                </div>
              ) : !credentialSync ? (
                <div className="text-sm text-[hsl(var(--muted-foreground))]">
                  Credential sync status unavailable.
                </div>
              ) : (
                <>
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-[hsl(var(--muted-foreground))]">Status</span>
                    <CredentialSyncBadge status={credentialSync.status} />
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-[hsl(var(--muted-foreground))]">Started</span>
                    <span>{credentialSync.started ? 'yes' : 'no'}</span>
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-[hsl(var(--muted-foreground))]">Watch connected</span>
                    <span>{credentialSync.watch_connected ? 'yes' : 'no'}</span>
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-[hsl(var(--muted-foreground))]">Applied revision</span>
                    <span>{credentialSync.applied_revision}</span>
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-[hsl(var(--muted-foreground))]">Local credentials</span>
                    <span>{credentialSync.credential_count}</span>
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-[hsl(var(--muted-foreground))]">Last success</span>
                    <span>{formatMs(credentialSync.last_success_at_ms)}</span>
                  </div>
                  <div className="flex items-start justify-between gap-4 text-sm">
                    <span className="text-[hsl(var(--muted-foreground))]">Last error</span>
                    <span className="max-w-[70%] text-right">
                      {credentialSync.last_error || '-'}
                    </span>
                  </div>
                </>
              )}
            </CardContent>
          </Card>

          <div className="flex items-center justify-between">
            <div className="text-sm text-[hsl(var(--muted-foreground))]">Raw JSON</div>
          </div>
          <pre className="max-h-[520px] overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))] bg-[hsl(var(--secondary))] p-3 text-xs">
            {JSON.stringify(q.data, null, 2)}
          </pre>
        </div>
      )}
    </div>
  )
}

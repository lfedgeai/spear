import { useMemo, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useInfiniteQuery, useQuery } from '@tanstack/react-query'

import { destroyInstance } from '@/api/control'
import { getInstance, listInstanceExecutions } from '@/api/instanceExecution'
import type { ExecutionSummary } from '@/api/types'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Dialog, DialogContent, DialogHeader } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'

function formatMs(ts: number) {
  if (!ts) return '-'
  return new Date(ts).toLocaleString()
}

function durationMs(startedAtMs: number, completedAtMs: number) {
  if (!startedAtMs) return '-'
  const end = completedAtMs || Date.now()
  const ms = Math.max(0, end - startedAtMs)
  if (ms < 1000) return `${ms}ms`
  return `${(ms / 1000).toFixed(1)}s`
}

function ExecutionStatusBadge({ status }: { status: string }) {
  const s = (status || '').toLowerCase()
  if (s === 'completed') return <Badge variant="success">completed</Badge>
  if (s === 'running') return <Badge>running</Badge>
  if (s === 'pending') return <Badge variant="secondary">pending</Badge>
  if (s === 'failed') return <Badge variant="destructive">failed</Badge>
  if (s === 'cancelled') return <Badge variant="secondary">cancelled</Badge>
  if (s === 'timeout') return <Badge variant="secondary">timeout</Badge>
  return <Badge variant="destructive">{status || 'unknown'}</Badge>
}

export default function InstanceDetailPage() {
  const { instanceId } = useParams()
  const id = instanceId || ''
  const navigate = useNavigate()
  const [destroyOpen, setDestroyOpen] = useState(false)
  const [destroyReason, setDestroyReason] = useState('')
  const [destroyLoading, setDestroyLoading] = useState(false)
  const [destroyError, setDestroyError] = useState('')

  const instanceQuery = useQuery({
    queryKey: ['instance-detail', id],
    queryFn: () => getInstance(id),
    enabled: !!id,
    retry: false,
    refetchInterval: 15_000,
  })

  const executionsQuery = useInfiniteQuery({
    queryKey: ['instance-executions', id],
    queryFn: ({ pageParam }) =>
      listInstanceExecutions({
        instance_id: id,
        limit: 50,
        page_token: pageParam || undefined,
      }),
    enabled: !!id,
    initialPageParam: '',
    getNextPageParam: (lastPage) => {
      if (!lastPage.success) return undefined
      return lastPage.next_page_token || undefined
    },
  })

  const rows: ExecutionSummary[] = useMemo(() => {
    const pages = executionsQuery.data?.pages || []
    const all: ExecutionSummary[] = []
    for (const p of pages) {
      if (!p.success) continue
      all.push(...(p.executions || []))
    }
    return all
  }, [executionsQuery.data])

  const inferredTaskId = useMemo(() => rows[0]?.task_id || '', [rows])

  const liveInstance = useMemo(() => {
    const data = instanceQuery.data
    if (!data || !data.success || !data.found || !data.instance) return null
    return data.instance
  }, [instanceQuery.data])

  const isInstanceActive = Boolean(instanceQuery.data?.success && instanceQuery.data?.active)
  const resolvedNodeUuid = liveInstance?.node_uuid || ''
  const currentStatus = liveInstance?.status || (instanceQuery.data?.found ? 'terminated' : 'absent')

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-semibold">Instance</div>
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            <Link to="/tasks" className="hover:underline">
              Tasks
            </Link>
            <span className="mx-2">/</span>
            <span className="font-mono text-xs">{id}</span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="destructive"
            onClick={() => {
              setDestroyError('')
              setDestroyReason('')
              setDestroyOpen(true)
            }}
            disabled={!isInstanceActive || !resolvedNodeUuid}
          >
            Destroy
          </Button>
          {inferredTaskId ? (
            <Link to={`/tasks/${encodeURIComponent(inferredTaskId)}`}>
              <Button variant="secondary">View task</Button>
            </Link>
          ) : null}
          <Button
            variant="secondary"
            onClick={() => {
              void instanceQuery.refetch()
              void executionsQuery.refetch()
            }}
          >
            Refresh
          </Button>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Current state</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid gap-3 sm:grid-cols-3">
            <div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">Status</div>
              <div className="text-sm">{currentStatus}</div>
            </div>
            <div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">Node</div>
              <div className="font-mono text-xs">{resolvedNodeUuid || '-'}</div>
            </div>
            <div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">Last seen</div>
              <div className="text-sm">
                {liveInstance ? formatMs(liveInstance.last_seen_ms) : '-'}
              </div>
            </div>
          </div>
          {!isInstanceActive ? (
            <div className="mt-3 text-sm text-[hsl(var(--muted-foreground))]">
              This instance is no longer active on the node. Historical executions may still be available below.
            </div>
          ) : null}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Recent executions</CardTitle>
        </CardHeader>
        <CardContent>
          {!executionsQuery.data ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">
              {executionsQuery.isLoading ? 'Loading…' : 'No data'}
            </div>
          ) : executionsQuery.data.pages.some((p) => !p.success) ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">
              {executionsQuery.data.pages.find((p) => !p.success)?.message ||
                'Failed to load executions'}
            </div>
          ) : rows.length === 0 ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">No executions.</div>
          ) : (
            <div className="overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))]">
              <table className="w-full text-sm">
                <thead className="bg-[hsl(var(--muted))] text-left text-xs text-[hsl(var(--muted-foreground))]">
                  <tr>
                    <th className="px-3 py-2">Execution</th>
                    <th className="px-3 py-2">Status</th>
                    <th className="px-3 py-2">Function</th>
                    <th className="px-3 py-2">Started</th>
                    <th className="px-3 py-2">Completed</th>
                    <th className="px-3 py-2">Duration</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((r) => (
                    <tr
                      key={r.execution_id}
                      className={cn(
                        'cursor-pointer border-t border-[hsl(var(--border))] hover:bg-[hsl(var(--accent))]',
                      )}
                      onClick={() =>
                        navigate(`/executions/${encodeURIComponent(r.execution_id)}`)
                      }
                      role="button"
                      tabIndex={0}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault()
                          navigate(`/executions/${encodeURIComponent(r.execution_id)}`)
                        }
                      }}
                    >
                      <td className="px-3 py-2 font-mono text-xs">
                        <Link
                          to={`/executions/${encodeURIComponent(r.execution_id)}`}
                          className="hover:underline"
                          onClick={(e) => e.stopPropagation()}
                        >
                          {r.execution_id}
                        </Link>
                      </td>
                      <td className="px-3 py-2">
                        <ExecutionStatusBadge status={r.status} />
                      </td>
                      <td className="px-3 py-2 text-xs text-[hsl(var(--muted-foreground))]">
                        {r.function_name || '-'}
                      </td>
                      <td className="px-3 py-2 text-xs text-[hsl(var(--muted-foreground))]">
                        {formatMs(r.started_at_ms)}
                      </td>
                      <td className="px-3 py-2 text-xs text-[hsl(var(--muted-foreground))]">
                        {formatMs(r.completed_at_ms)}
                      </td>
                      <td className="px-3 py-2 text-xs text-[hsl(var(--muted-foreground))]">
                        {durationMs(r.started_at_ms, r.completed_at_ms)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {executionsQuery.hasNextPage ? (
            <div className="mt-3">
              <Button
                variant="secondary"
                onClick={() => executionsQuery.fetchNextPage()}
                disabled={!executionsQuery.hasNextPage || executionsQuery.isFetchingNextPage}
              >
                {executionsQuery.isFetchingNextPage ? 'Loading…' : 'Load more'}
              </Button>
            </div>
          ) : null}
        </CardContent>
      </Card>

      <Dialog open={destroyOpen} onOpenChange={setDestroyOpen}>
        <DialogContent className="w-[min(520px,calc(100vw-24px))]">
          <DialogHeader
            title="Destroy instance"
            description="Best-effort. This terminates all running executions on the instance."
          />
          <div className="space-y-3">
            <div className="grid grid-cols-2 gap-3 text-sm">
              <div>
                <div className="text-xs text-[hsl(var(--muted-foreground))]">Instance</div>
                <div className="font-mono text-xs">{id}</div>
              </div>
              <div>
                <div className="text-xs text-[hsl(var(--muted-foreground))]">Node</div>
                <div className="font-mono text-xs">{resolvedNodeUuid || '-'}</div>
              </div>
            </div>
            <div className="space-y-1">
              <div className="text-xs text-[hsl(var(--muted-foreground))]">Reason (optional)</div>
              <Input
                value={destroyReason}
                onChange={(ev) => setDestroyReason(ev.target.value)}
                placeholder="Reason"
              />
            </div>
            {destroyError ? (
              <div className="text-sm text-[hsl(var(--destructive))]">{destroyError}</div>
            ) : null}
            <div className="flex justify-end gap-2">
              <Button
                variant="secondary"
                onClick={() => setDestroyOpen(false)}
                disabled={destroyLoading}
              >
                Cancel
              </Button>
              <Button
                variant="destructive"
                disabled={destroyLoading || !isInstanceActive || !resolvedNodeUuid}
                onClick={async () => {
                  if (!isInstanceActive || !resolvedNodeUuid) return
                  setDestroyError('')
                  setDestroyLoading(true)
                  try {
                    const resp = await destroyInstance({
                      instance_id: id,
                      node_uuid: resolvedNodeUuid,
                      reason: destroyReason.trim() ? destroyReason.trim() : undefined,
                    })
                    if (!resp.success) {
                      setDestroyError(resp.message || 'Destroy failed')
                      return
                    }
                    setDestroyOpen(false)
                    void instanceQuery.refetch()
                    void executionsQuery.refetch()
                  } catch (err) {
                    setDestroyError(
                      String((err as { message?: string } | null)?.message || err || 'Destroy failed'),
                    )
                  } finally {
                    setDestroyLoading(false)
                  }
                }}
              >
                {destroyLoading ? 'Destroying…' : 'Destroy'}
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}

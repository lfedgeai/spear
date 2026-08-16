import { useEffect, useMemo, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useInfiniteQuery, useQuery } from '@tanstack/react-query'

import { destroyInstance } from '@/api/control'
import { getInstance, listInstanceExecutions } from '@/api/instanceExecution'
import type { ExecutionSummary } from '@/api/types'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { ExecutionTable } from '@/features/executions/ExecutionTable'
import { ReasonConfirmDialog } from '@/features/shared/ReasonConfirmDialog'
import { InstanceStatusBadge } from '@/features/shared/status-badges'

function formatMs(ts: number) {
  if (!ts) return '-'
  return new Date(ts).toLocaleString()
}

export default function InstanceDetailPage() {
  const { instanceId } = useParams()
  const id = instanceId || ''
  const navigate = useNavigate()
  const [destroyOpen, setDestroyOpen] = useState(false)
  const [destroyReason, setDestroyReason] = useState('')
  const [destroyLoading, setDestroyLoading] = useState(false)
  const [destroyError, setDestroyError] = useState('')
  const [destroyRequested, setDestroyRequested] = useState(false)

  const instanceQuery = useQuery({
    queryKey: ['instance-detail', id],
    queryFn: () => getInstance(id),
    enabled: !!id,
    retry: false,
    refetchInterval: destroyRequested ? 1_500 : 15_000,
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
  const resolvedTaskId = liveInstance?.task_id || inferredTaskId
  const currentStatus =
    destroyRequested && isInstanceActive
      ? 'terminating'
      : liveInstance?.status || (instanceQuery.data?.found ? 'terminated' : 'absent')

  useEffect(() => {
    if (!destroyRequested) return
    if (!instanceQuery.data?.success) return
    if (instanceQuery.data.found && instanceQuery.data.active) return
    if (resolvedTaskId) {
      navigate(`/tasks/${encodeURIComponent(resolvedTaskId)}`, { replace: true })
    }
  }, [destroyRequested, resolvedTaskId, instanceQuery.data, navigate])

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
            disabled={!isInstanceActive || !resolvedNodeUuid || destroyRequested}
          >
            {destroyRequested ? 'Destroying…' : 'Destroy'}
          </Button>
          {resolvedTaskId ? (
            <Link to={`/tasks/${encodeURIComponent(resolvedTaskId)}`}>
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
          <CardTitle>Replica state</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
            <div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">Replica status</div>
              <div className="mt-2">
                <InstanceStatusBadge status={currentStatus} />
              </div>
            </div>
            <div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">Task</div>
              <div className="font-mono text-xs">
                {resolvedTaskId ? (
                  <Link to={`/tasks/${encodeURIComponent(resolvedTaskId)}`} className="hover:underline">
                    {resolvedTaskId}
                  </Link>
                ) : (
                  '-'
                )}
            </div>
            </div>
            <div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">Node</div>
              <div className="font-mono text-xs">{resolvedNodeUuid || '-'}</div>
            </div>
            <div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">Current execution</div>
              <div className="font-mono text-xs">
                {liveInstance?.current_execution_id ? (
                  <Link
                    to={`/executions/${encodeURIComponent(liveInstance.current_execution_id)}`}
                    className="hover:underline"
                  >
                    {liveInstance.current_execution_id}
                  </Link>
                ) : (
                  '-'
                )}
              </div>
            </div>
            <div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">Last seen</div>
              <div className="text-sm">{liveInstance ? formatMs(liveInstance.last_seen_ms) : '-'}</div>
            </div>
            <div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">Created</div>
              <div className="text-sm">{liveInstance ? formatMs(liveInstance.created_at_ms) : '-'}</div>
            </div>
            <div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">Updated</div>
              <div className="text-sm">{liveInstance ? formatMs(liveInstance.updated_at_ms) : '-'}</div>
            </div>
          </div>
          {isInstanceActive ? (
            <div className="mt-3 text-sm text-[hsl(var(--muted-foreground))]">
              This instance is one replica of the task. Destroying it terminates running executions
              on this replica, and the system may create a replacement if the task still needs
              replicas.
            </div>
          ) : null}
          {!isInstanceActive ? (
            <div className="mt-3 text-sm text-[hsl(var(--muted-foreground))]">
              This replica is no longer active on the node. Historical executions may still be
              available below.
            </div>
          ) : null}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Executions on this replica</CardTitle>
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
            <ExecutionTable
              rows={rows}
              columns={['execution', 'status', 'function', 'started', 'completed', 'duration']}
              onRowClick={(row) =>
                navigate(`/executions/${encodeURIComponent(row.execution_id)}`)
              }
            />
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

      <ReasonConfirmDialog
        open={destroyOpen}
        onOpenChange={setDestroyOpen}
        title="Destroy replica"
        description="This removes one replica instance. Running executions on it will be terminated, and the system may replace it to meet the task replica target."
        details={[
          { label: 'Instance', value: id },
          { label: 'Node', value: resolvedNodeUuid || '-' },
        ]}
        reason={destroyReason}
        onReasonChange={setDestroyReason}
        error={destroyError}
        confirmLabel="Destroy"
        confirmingLabel="Destroying…"
        confirmDisabled={!isInstanceActive || !resolvedNodeUuid}
        confirming={destroyLoading}
        onConfirm={async () => {
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
            setDestroyRequested(true)
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
      />
    </div>
  )
}

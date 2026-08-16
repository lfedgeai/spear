import { useMemo, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'

import { destroyInstance } from '@/api/control'
import { getTaskDetail } from '@/api/tasks'
import type { TaskDetail as TaskDetailResponse } from '@/api/types'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { ReasonConfirmDialog } from '@/features/shared/ReasonConfirmDialog'
import { TaskStatusBadge } from '@/features/shared/status-badges'
import {
  summarizeTaskInstances,
  TaskInstancesPanel,
} from '@/features/tasks/TaskInstancesSection'
import DeleteTaskDialog from '@/features/tasks/DeleteTaskDialog'
import { useTaskInstances } from '@/features/tasks/useTaskInstances'

function formatSec(ts: number | undefined) {
  if (!ts) return '-'
  return new Date(ts * 1000).toLocaleString()
}

export default function TaskDetailPage() {
  const { taskId } = useParams()
  const id = taskId || ''
  const navigate = useNavigate()
  const [destroyOpen, setDestroyOpen] = useState(false)
  const [destroyTarget, setDestroyTarget] = useState<{ instanceId: string; nodeUuid: string } | null>(null)
  const [destroyReason, setDestroyReason] = useState('')
  const [destroyLoading, setDestroyLoading] = useState(false)
  const [destroyError, setDestroyError] = useState('')
  const [deleteOpen, setDeleteOpen] = useState(false)

  const taskQuery = useQuery({
    queryKey: ['task-detail', id],
    queryFn: () => getTaskDetail(id),
    enabled: !!id,
  })

  const {
    query: instancesQuery,
    instances,
    displayedInstances,
    destroyPendingIds,
    loadErrorMessage,
    markDestroyPending,
  } = useTaskInstances(id)

  const title = useMemo(() => `Task ${id}`, [id])
  const task = (taskQuery.data as TaskDetailResponse | undefined)?.task || null
  const taskName = task?.name || ''
  const desiredReplicas = task?.desired_replicas ?? 0
  const activeReplicas = instances.length
  const readyReplicas = instances.filter((row) => {
    const status = row.status.toLowerCase()
    return status === 'running' || status === 'idle'
  }).length
  const isReconciling = desiredReplicas > activeReplicas
  const taskCapabilities = (task?.capabilities || []).filter(Boolean)
  const resultUris = (task?.result_uris || []).filter(Boolean)
  const instanceSummary = summarizeTaskInstances(displayedInstances)
  const isDeleting = (task?.status || '').toLowerCase() === 'deleting'

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-semibold">{title}</div>
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            <Link
              to="/tasks"
              className="text-[hsl(var(--muted-foreground))] hover:underline"
            >
              Tasks
            </Link>
            <span className="mx-2">/</span>
            <span className="font-mono text-xs">{id}</span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Link to={`/executions?task_id=${encodeURIComponent(id)}`}>
            <Button variant="secondary" disabled={!taskQuery.data?.found}>
              Execution history
            </Button>
          </Link>
          <Button
            variant="destructive"
            onClick={() => setDeleteOpen(true)}
            disabled={!taskQuery.data?.found}
          >
            Delete
          </Button>
          <Button variant="secondary" onClick={() => taskQuery.refetch()}>
            Refresh
          </Button>
        </div>
      </div>

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>Replica status</CardTitle>
            <Button variant="secondary" onClick={() => taskQuery.refetch()}>
              Refresh
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          {taskQuery.isLoading ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">Loading…</div>
          ) : taskQuery.isError ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">
              Failed to load task detail.
            </div>
          ) : taskQuery.data?.found && task ? (
            <div className="space-y-4">
              <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                <div className="rounded-[var(--radius)] border border-[hsl(var(--border))] p-3">
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Task status</div>
                  <div className="mt-2">
                    <TaskStatusBadge status={task.status} />
                  </div>
                </div>
                <div className="rounded-[var(--radius)] border border-[hsl(var(--border))] p-3">
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Desired replicas</div>
                  <div className="mt-2 text-2xl font-semibold">{desiredReplicas}</div>
                </div>
                <div className="rounded-[var(--radius)] border border-[hsl(var(--border))] p-3">
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Active replicas</div>
                  <div className="mt-2 text-2xl font-semibold">{activeReplicas}</div>
                  <div className="mt-1 text-xs text-[hsl(var(--muted-foreground))]">
                    ready {readyReplicas}
                  </div>
                </div>
                <div className="rounded-[var(--radius)] border border-[hsl(var(--border))] p-3">
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Reconcile state</div>
                  <div className="mt-2">
                    <Badge variant={isReconciling ? 'secondary' : 'success'}>
                      {isReconciling ? 'reconciling' : 'at target'}
                    </Badge>
                  </div>
                </div>
              </div>

              <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Task ID</div>
                  <div className="font-mono text-xs">{task.task_id}</div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Endpoint</div>
                  <div className="font-mono text-xs">{task.endpoint || '-'}</div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Version</div>
                  <div className="text-sm">{task.version || '-'}</div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Priority</div>
                  <div className="text-sm">{task.priority || '-'}</div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Scheduling</div>
                  <div className="text-sm">{String(task.scheduling_strategy || '-')}</div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Last heartbeat</div>
                  <div className="text-sm">{formatSec(task.last_heartbeat)}</div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Registered</div>
                  <div className="text-sm">{formatSec(task.registered_at)}</div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Executable</div>
                  <div className="text-sm">{task.executable_type || '-'}</div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Last result</div>
                  <div className="text-sm">{task.last_result_status || '-'}</div>
                </div>
              </div>

              {task.description ? (
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Description</div>
                  <div className="text-sm">{task.description}</div>
                </div>
              ) : null}

              {taskCapabilities.length > 0 ? (
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Capabilities</div>
                  <div className="mt-2 flex flex-wrap gap-2">
                    {taskCapabilities.map((capability) => (
                      <Badge key={capability} variant="secondary">
                        {capability}
                      </Badge>
                    ))}
                  </div>
                </div>
              ) : null}

              {resultUris.length > 0 ? (
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Result URIs</div>
                  <div className="mt-2 space-y-1">
                    {resultUris.map((uri) => (
                      <div key={uri} className="font-mono text-xs">
                        {uri}
                      </div>
                    ))}
                  </div>
                </div>
              ) : null}

              {isDeleting ? (
                <div className="rounded-[var(--radius)] border border-[hsl(var(--border))] bg-[hsl(var(--muted))] p-3">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <div className="text-sm font-medium">Cleanup status</div>
                      <div className="text-xs text-[hsl(var(--muted-foreground))]">
                        Runtime cleanup is in progress. Task deletion completes after all active
                        replicas disappear and runtime cleanup is acknowledged.
                      </div>
                    </div>
                    <Badge variant={instanceSummary.total === 0 ? 'success' : 'secondary'}>
                      {instanceSummary.total === 0 ? 'runtime cleaned' : 'cleanup pending'}
                    </Badge>
                  </div>
                  <div className="mt-3 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                    <div>
                      <div className="text-xs text-[hsl(var(--muted-foreground))]">
                        Instances remaining
                      </div>
                      <div className="mt-1 text-lg font-semibold">{instanceSummary.total}</div>
                    </div>
                    <div>
                      <div className="text-xs text-[hsl(var(--muted-foreground))]">
                        Active executions remaining
                      </div>
                      <div className="mt-1 text-lg font-semibold">
                        {instanceSummary.activeExecutions}
                      </div>
                    </div>
                    <div>
                      <div className="text-xs text-[hsl(var(--muted-foreground))]">
                        Deletion requested
                      </div>
                      <div className="mt-1 text-sm">{formatSec(task.deletion_requested_at)}</div>
                    </div>
                    <div>
                      <div className="text-xs text-[hsl(var(--muted-foreground))]">
                        Deletion reason
                      </div>
                      <div className="mt-1 text-sm">{task.deletion_reason || '-'}</div>
                    </div>
                  </div>
                </div>
              ) : null}
            </div>
          ) : (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">Not found</div>
          )}
        </CardContent>
      </Card>

      <TaskInstancesPanel
        title="Current replicas"
        headerActions={
          <>
            <Link to={`/tasks/${encodeURIComponent(id)}/instances`}>
              <Button variant="secondary">View all instances</Button>
            </Link>
            <Button
              variant="secondary"
              onClick={() => {
                instancesQuery.refetch()
              }}
            >
              Refresh
            </Button>
          </>
        }
        instances={displayedInstances}
        showSummaryCards
        emptyMessage="No replicas are currently reported for this task."
        destroyPendingIds={destroyPendingIds}
        onSelectInstance={(instanceId) =>
          navigate(`/instances/${encodeURIComponent(instanceId)}`)
        }
        onDestroy={(row) => {
          setDestroyError('')
          setDestroyReason('')
          setDestroyTarget({
            instanceId: row.instance_id,
            nodeUuid: row.node_uuid,
          })
          setDestroyOpen(true)
        }}
        isLoading={instancesQuery.isLoading}
        hasData={!!instancesQuery.data}
        loadErrorMessage={
          instancesQuery.data?.pages.some((p) => !p.success)
            ? loadErrorMessage || 'Failed to load instances'
            : undefined
        }
        helpText="Destroying a replica terminates its running execution. If this task still needs replicas, SPEARlet should automatically create a replacement."
        hasNextPage={instancesQuery.hasNextPage}
        isFetchingNextPage={instancesQuery.isFetchingNextPage}
        onLoadMore={() => instancesQuery.fetchNextPage()}
      />

      <ReasonConfirmDialog
        open={destroyOpen}
        onOpenChange={setDestroyOpen}
        title="Destroy replica"
        description="This removes one replica instance. Running executions on it will be terminated, and the system may replace it to meet the task replica target."
        details={[
          { label: 'Instance', value: destroyTarget?.instanceId || '-' },
          { label: 'Node', value: destroyTarget?.nodeUuid || '-' },
        ]}
        reason={destroyReason}
        onReasonChange={setDestroyReason}
        error={destroyError}
        confirmLabel="Destroy"
        confirmingLabel="Destroying…"
        confirmDisabled={!destroyTarget}
        confirming={destroyLoading}
        onConfirm={async () => {
          if (!destroyTarget) return
          setDestroyLoading(true)
          setDestroyError('')
          try {
            const res = await destroyInstance({
              instance_id: destroyTarget.instanceId,
              node_uuid: destroyTarget.nodeUuid,
              reason: destroyReason.trim() || undefined,
            })
            if (!res.success) throw new Error(res.message || 'Destroy failed')
            markDestroyPending(destroyTarget.instanceId)
            setDestroyOpen(false)
            void instancesQuery.refetch()
          } catch (e) {
            setDestroyError((e as Error).message)
          } finally {
            setDestroyLoading(false)
          }
        }}
      />
      <DeleteTaskDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        taskId={id}
        taskName={taskName}
        onDeleted={() => {
          setDeleteOpen(false)
          navigate('/tasks')
        }}
      />
    </div>
  )
}

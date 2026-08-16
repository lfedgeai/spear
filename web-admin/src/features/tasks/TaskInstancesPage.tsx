import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'

import { destroyInstance } from '@/api/control'
import { getTaskDetail } from '@/api/tasks'
import type { TaskDetail as TaskDetailResponse } from '@/api/types'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { ReasonConfirmDialog } from '@/features/shared/ReasonConfirmDialog'
import {
  summarizeTaskInstances,
  TaskInstanceSummaryCards,
  TaskInstancesPanel,
} from '@/features/tasks/TaskInstancesSection'
import { useTaskInstances } from '@/features/tasks/useTaskInstances'

function formatSec(ts: number | undefined) {
  if (!ts) return '-'
  return new Date(ts * 1000).toLocaleString()
}

export default function TaskInstancesPage() {
  const { taskId } = useParams()
  const id = taskId || ''
  const navigate = useNavigate()
  const [destroyOpen, setDestroyOpen] = useState(false)
  const [destroyTarget, setDestroyTarget] = useState<{ instanceId: string; nodeUuid: string } | null>(
    null,
  )
  const [destroyReason, setDestroyReason] = useState('')
  const [destroyLoading, setDestroyLoading] = useState(false)
  const [destroyError, setDestroyError] = useState('')

  const taskQuery = useQuery({
    queryKey: ['task-detail', id],
    queryFn: () => getTaskDetail(id),
    enabled: !!id,
  })

  const task = (taskQuery.data as TaskDetailResponse | undefined)?.task || null
  const {
    query: instancesQuery,
    displayedInstances,
    destroyPendingIds,
    loadErrorMessage,
    markDestroyPending,
  } = useTaskInstances(id)
  const summary = summarizeTaskInstances(displayedInstances)
  const isDeleting = (task?.status || '').toLowerCase() === 'deleting'

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-semibold">Task instances</div>
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            <Link to="/tasks" className="hover:underline">
              Tasks
            </Link>
            <span className="mx-2">/</span>
            <Link to={`/tasks/${encodeURIComponent(id)}`} className="font-mono text-xs hover:underline">
              {id}
            </Link>
            <span className="mx-2">/</span>
            <span className="font-mono text-xs">instances</span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Link to={`/tasks/${encodeURIComponent(id)}`}>
            <Button variant="secondary">Back to task</Button>
          </Link>
          <Button
            variant="secondary"
            onClick={() => {
              taskQuery.refetch()
              instancesQuery.refetch()
            }}
          >
            Refresh
          </Button>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Replica overview</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <TaskInstanceSummaryCards instances={displayedInstances} />
          {task ? (
            <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4 text-sm">
              <div>
                <div className="text-xs text-[hsl(var(--muted-foreground))]">Task</div>
                <div>{task.name || task.task_id}</div>
              </div>
              <div>
                <div className="text-xs text-[hsl(var(--muted-foreground))]">Desired replicas</div>
                <div>{task.desired_replicas ?? 0}</div>
              </div>
              <div>
                <div className="text-xs text-[hsl(var(--muted-foreground))]">Endpoint</div>
                <div className="font-mono text-xs">{task.endpoint || '-'}</div>
              </div>
              <div>
                <div className="text-xs text-[hsl(var(--muted-foreground))]">Task status</div>
                <div>{task.status || '-'}</div>
              </div>
            </div>
          ) : null}
          {isDeleting ? (
            <div className="rounded-[var(--radius)] border border-[hsl(var(--border))] bg-[hsl(var(--muted))] p-3">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <div className="text-sm font-medium">Cleanup progress</div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">
                    Task deletion waits for all remaining runtime replicas to drain and stop.
                  </div>
                </div>
                <Badge variant={summary.total === 0 ? 'success' : 'secondary'}>
                  {summary.total === 0 ? 'instances cleared' : 'cleanup pending'}
                </Badge>
              </div>
              <div className="mt-3 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Instances remaining</div>
                  <div className="mt-1 text-lg font-semibold">{summary.total}</div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">
                    Active executions remaining
                  </div>
                  <div className="mt-1 text-lg font-semibold">{summary.activeExecutions}</div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Deletion requested</div>
                  <div className="mt-1 text-sm">{formatSec(task?.deletion_requested_at)}</div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Deletion reason</div>
                  <div className="mt-1 text-sm">{task?.deletion_reason || '-'}</div>
                </div>
              </div>
            </div>
          ) : null}
        </CardContent>
      </Card>

      <TaskInstancesPanel
        title="All instances for this task"
        instances={displayedInstances}
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
        helpText="This page shows task-scoped runtime replicas. Use it to confirm convergence and cleanup progress after task deletion."
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
            setDestroyTarget(null)
            setDestroyReason('')
            instancesQuery.refetch()
          } catch (e) {
            setDestroyError((e as Error).message)
          } finally {
            setDestroyLoading(false)
          }
        }}
      />
    </div>
  )
}

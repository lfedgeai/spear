import { Link } from 'react-router-dom'

import type { InstanceSummary } from '@/api/types'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { InstanceStatusBadge } from '@/features/shared/status-badges'
import { cn } from '@/lib/utils'

function formatInstanceMs(ts?: number) {
  if (!ts) return '-'
  return new Date(ts).toLocaleString()
}

export function summarizeTaskInstances(instances: InstanceSummary[]) {
  const ready = instances.filter((row) => {
    const status = row.status.toLowerCase()
    return status === 'running' || status === 'idle' || status === 'ready'
  }).length
  const activeExecutions = instances.filter((row) => !!row.current_execution_id).length
  const terminating = instances.filter((row) => {
    const status = row.status.toLowerCase()
    return status === 'terminating' || status === 'stopping' || status === 'terminated'
  }).length
  return {
    total: instances.length,
    ready,
    activeExecutions,
    terminating,
  }
}

export function TaskInstanceSummaryCards({ instances }: { instances: InstanceSummary[] }) {
  const summary = summarizeTaskInstances(instances)
  const items = [
    { label: 'Instances total', value: summary.total },
    { label: 'Ready replicas', value: summary.ready },
    { label: 'Active executions', value: summary.activeExecutions },
    { label: 'Terminating', value: summary.terminating },
  ]
  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
      {items.map((item) => (
        <div
          key={item.label}
          className="rounded-[var(--radius)] border border-[hsl(var(--border))] p-3"
        >
          <div className="text-xs text-[hsl(var(--muted-foreground))]">{item.label}</div>
          <div className="mt-2 text-2xl font-semibold">{item.value}</div>
        </div>
      ))}
    </div>
  )
}

function TaskInstancesTable(props: {
  instances: InstanceSummary[]
  emptyMessage: string
  destroyPendingIds: string[]
  onDestroy: (row: InstanceSummary) => void
  onSelectInstance: (instanceId: string) => void
}) {
  if (props.instances.length === 0) {
    return (
      <div className="text-sm text-[hsl(var(--muted-foreground))]">{props.emptyMessage}</div>
    )
  }

  return (
    <div className="overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))]">
      <table className="w-full text-sm">
        <thead className="bg-[hsl(var(--muted))] text-left text-xs text-[hsl(var(--muted-foreground))]">
          <tr>
            <th className="px-3 py-2">Instance</th>
            <th className="px-3 py-2">Node</th>
            <th className="px-3 py-2">Status</th>
            <th className="px-3 py-2">Created</th>
            <th className="px-3 py-2">Updated</th>
            <th className="px-3 py-2">Last seen</th>
            <th className="px-3 py-2">Current execution</th>
            <th className="px-3 py-2"></th>
          </tr>
        </thead>
        <tbody>
          {props.instances.map((row) => (
            <tr
              key={row.instance_id}
              className={cn(
                'cursor-pointer border-t border-[hsl(var(--border))] hover:bg-[hsl(var(--accent))]',
              )}
              onClick={() => props.onSelectInstance(row.instance_id)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault()
                  props.onSelectInstance(row.instance_id)
                }
              }}
            >
              <td className="px-3 py-2 font-mono text-xs">
                <Link
                  to={`/instances/${encodeURIComponent(row.instance_id)}`}
                  className="hover:underline"
                  onClick={(e) => e.stopPropagation()}
                >
                  {row.instance_id}
                </Link>
              </td>
              <td className="px-3 py-2 font-mono text-xs">
                {row.node_uuid ? (
                  <Link
                    to={`/nodes/${encodeURIComponent(row.node_uuid)}`}
                    className="hover:underline"
                    onClick={(e) => e.stopPropagation()}
                  >
                    {row.node_uuid}
                  </Link>
                ) : (
                  '-'
                )}
              </td>
              <td className="px-3 py-2">
                <InstanceStatusBadge status={row.status} />
              </td>
              <td className="px-3 py-2 text-xs text-[hsl(var(--muted-foreground))]">
                {formatInstanceMs(row.created_at_ms)}
              </td>
              <td className="px-3 py-2 text-xs text-[hsl(var(--muted-foreground))]">
                {formatInstanceMs(row.updated_at_ms)}
              </td>
              <td className="px-3 py-2 text-xs text-[hsl(var(--muted-foreground))]">
                {formatInstanceMs(row.last_seen_ms)}
              </td>
              <td className="px-3 py-2 font-mono text-xs">
                {row.current_execution_id ? (
                  <Link
                    to={`/executions/${encodeURIComponent(row.current_execution_id)}`}
                    className="hover:underline"
                    onClick={(e) => e.stopPropagation()}
                  >
                    {row.current_execution_id}
                  </Link>
                ) : (
                  '-'
                )}
              </td>
              <td className="px-3 py-2 text-right">
                <div className="flex items-center justify-end gap-2">
                  <Button
                    size="sm"
                    variant="destructive"
                    onClick={(e) => {
                      e.stopPropagation()
                      props.onDestroy(row)
                    }}
                    disabled={!row.node_uuid || props.destroyPendingIds.includes(row.instance_id)}
                  >
                    {props.destroyPendingIds.includes(row.instance_id) ? 'Destroying…' : 'Destroy'}
                  </Button>
                  <Link
                    to={`/instances/${encodeURIComponent(row.instance_id)}`}
                    className="text-xs text-[hsl(var(--muted-foreground))] hover:underline"
                    onClick={(e) => e.stopPropagation()}
                  >
                    View
                  </Link>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

export function TaskInstancesPanel(props: {
  title: string
  headerActions?: React.ReactNode
  instances: InstanceSummary[]
  showSummaryCards?: boolean
  emptyMessage: string
  destroyPendingIds: string[]
  onDestroy: (row: InstanceSummary) => void
  onSelectInstance: (instanceId: string) => void
  isLoading: boolean
  hasData: boolean
  loadErrorMessage?: string
  helpText?: string
  hasNextPage?: boolean
  isFetchingNextPage?: boolean
  onLoadMore?: () => void
}) {
  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>{props.title}</CardTitle>
          {props.headerActions ? (
            <div className="flex items-center gap-2">{props.headerActions}</div>
          ) : null}
        </div>
      </CardHeader>
      <CardContent>
        {props.showSummaryCards ? (
          <div className="mb-4">
            <TaskInstanceSummaryCards instances={props.instances} />
          </div>
        ) : null}
        {!props.hasData ? (
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            {props.isLoading ? 'Loading…' : 'No data'}
          </div>
        ) : props.loadErrorMessage ? (
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            {props.loadErrorMessage}
          </div>
        ) : (
          <TaskInstancesTable
            instances={props.instances}
            emptyMessage={props.emptyMessage}
            destroyPendingIds={props.destroyPendingIds}
            onSelectInstance={props.onSelectInstance}
            onDestroy={props.onDestroy}
          />
        )}
        {props.helpText ? (
          <div className="mt-3 text-xs text-[hsl(var(--muted-foreground))]">{props.helpText}</div>
        ) : null}
        {props.hasNextPage ? (
          <div className="mt-3">
            <Button
              variant="secondary"
              onClick={props.onLoadMore}
              disabled={!props.hasNextPage || props.isFetchingNextPage}
            >
              {props.isFetchingNextPage ? 'Loading…' : 'Load more'}
            </Button>
          </div>
        ) : null}
      </CardContent>
    </Card>
  )
}

import { useMemo, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'

import { listExecutionHistory } from '@/api/instanceExecution'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { ExecutionTable } from '@/features/executions/ExecutionTable'

export default function ExecutionHistoryPage() {
  const [params, setParams] = useSearchParams()
  const [taskFilter, setTaskFilter] = useState(params.get('task_id') || '')
  const [statusFilter, setStatusFilter] = useState(params.get('status') || '')

  const q = useQuery({
    queryKey: ['execution-history', params.toString()],
    queryFn: () =>
      listExecutionHistory({
        task_id: params.get('task_id') || undefined,
        status: params.get('status') || undefined,
        limit: 100,
      }),
  })

  // Keep the applied filters in the URL so the archive view is linkable and
  // back/forward navigation preserves the current search.
  const rows = useMemo(() => q.data?.executions || [], [q.data])

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-semibold">Execution History</div>
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            Archived execution records across tasks and replicas.
          </div>
        </div>
        <Button variant="secondary" onClick={() => q.refetch()}>
          Refresh
        </Button>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Filters</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="grid gap-3 md:grid-cols-3">
            <Input
              value={taskFilter}
              onChange={(e) => setTaskFilter(e.target.value)}
              placeholder="Filter by task id"
            />
            <Input
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value)}
              placeholder="Filter by status"
            />
            <div className="flex items-center gap-2">
              <Button
                onClick={() => {
                  const next = new URLSearchParams()
                  if (taskFilter.trim()) next.set('task_id', taskFilter.trim())
                  if (statusFilter.trim()) next.set('status', statusFilter.trim())
                  setParams(next)
                }}
              >
                Apply
              </Button>
              <Button
                variant="secondary"
                onClick={() => {
                  setTaskFilter('')
                  setStatusFilter('')
                  setParams(new URLSearchParams())
                }}
              >
                Clear
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Archived executions</CardTitle>
        </CardHeader>
        <CardContent>
          {!q.data ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">
              {q.isLoading ? 'Loading…' : 'No data'}
            </div>
          ) : q.data.success === false ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">
              {q.data.message || 'Failed to load execution history'}
            </div>
          ) : rows.length === 0 ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">
              No archived executions matched the current filters.
            </div>
          ) : (
            <ExecutionTable
              rows={rows}
              columns={[
                'execution',
                'task',
                'status',
                'function',
                'instance',
                'node',
                'started',
                'completed',
              ]}
            />
          )}
        </CardContent>
      </Card>
    </div>
  )
}

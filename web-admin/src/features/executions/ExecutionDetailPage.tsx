import { useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'

import { terminateExecution } from '@/api/control'
import { getExecution } from '@/api/instanceExecution'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import ExecutionLogsDialog from '@/features/executions/ExecutionLogsDialog'
import { ReasonConfirmDialog } from '@/features/shared/ReasonConfirmDialog'
import { ExecutionStatusBadge } from '@/features/shared/status-badges'

function formatMs(ts: number) {
  if (!ts) return '-'
  return new Date(ts).toLocaleString()
}

function KVTable(props: { data: Record<string, string> }) {
  const entries = Object.entries(props.data || {})
  if (entries.length === 0) {
    return <div className="text-sm text-[hsl(var(--muted-foreground))]">No metadata.</div>
  }
  return (
    <div className="overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))]">
      <table className="w-full text-sm">
        <thead className="bg-[hsl(var(--muted))] text-left text-xs text-[hsl(var(--muted-foreground))]">
          <tr>
            <th className="px-3 py-2">Key</th>
            <th className="px-3 py-2">Value</th>
          </tr>
        </thead>
        <tbody>
          {entries.map(([k, v]) => (
            <tr key={k} className="border-t border-[hsl(var(--border))]">
              <td className="px-3 py-2 font-mono text-xs">{k}</td>
              <td className="px-3 py-2 font-mono text-xs">{v}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

export default function ExecutionDetailPage() {
  const { executionId } = useParams()
  const id = executionId || ''
  const [logsOpen, setLogsOpen] = useState(false)
  const [terminateOpen, setTerminateOpen] = useState(false)
  const [terminateReason, setTerminateReason] = useState('')
  const [terminateLoading, setTerminateLoading] = useState(false)
  const [terminateError, setTerminateError] = useState('')

  const q = useQuery({
    queryKey: ['execution-detail', id],
    queryFn: () => getExecution(id),
    enabled: !!id,
  })

  const data = q.data
  const found = data?.success && data.found && data.execution
  const e = found ? data.execution : null
  const canTerminate =
    !!e && (String(e.status).toLowerCase() === 'running' || String(e.status).toLowerCase() === 'pending')

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-semibold">Execution run</div>
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            <Link to="/executions" className="hover:underline">
              Execution History
            </Link>
            <span className="mx-2">/</span>
            <span className="font-mono text-xs">{id}</span>
          </div>
          {e?.task_id ? (
            <div className="mt-1 text-xs text-[hsl(var(--muted-foreground))]">
              Task:{' '}
              <Link
                to={`/tasks/${encodeURIComponent(e.task_id)}`}
                className="font-mono hover:underline"
              >
                {e.task_id}
              </Link>
            </div>
          ) : null}
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="destructive"
            onClick={() => {
              setTerminateError('')
              setTerminateReason('')
              setTerminateOpen(true)
            }}
            disabled={!canTerminate}
          >
            Terminate
          </Button>
          <Button variant="secondary" onClick={() => q.refetch()}>
            Refresh
          </Button>
          <Button
            variant="secondary"
            onClick={() => setLogsOpen(true)}
            disabled={!e?.log_ref}
          >
            Logs
          </Button>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Run summary</CardTitle>
        </CardHeader>
        <CardContent>
          {q.isLoading ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">Loading…</div>
          ) : !data ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">No data</div>
          ) : !data.success ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">
              {data.message || 'Failed to load execution'}
            </div>
          ) : !found ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">Not found</div>
          ) : (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <ExecutionStatusBadge status={e!.status} />
                <span className="font-mono text-xs text-[hsl(var(--muted-foreground))]">
                  {e!.execution_id}
                </span>
              </div>
              <div className="text-sm text-[hsl(var(--muted-foreground))]">
                A single execution produced by one invocation on one task replica.
              </div>

              <div className="grid grid-cols-2 gap-3 text-sm">
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Task</div>
                  <div className="font-mono text-xs">
                    <Link to={`/tasks/${encodeURIComponent(e!.task_id)}`} className="hover:underline">
                      {e!.task_id}
                    </Link>
                  </div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Replica instance</div>
                  <div className="font-mono text-xs">
                    <Link
                      to={`/instances/${encodeURIComponent(e!.instance_id)}`}
                      className="hover:underline"
                    >
                      {e!.instance_id}
                    </Link>
                  </div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Node</div>
                  <div className="font-mono text-xs">
                    {e!.node_uuid ? (
                      <Link
                        to={`/nodes/${encodeURIComponent(e!.node_uuid)}`}
                        className="hover:underline"
                      >
                        {e!.node_uuid}
                      </Link>
                    ) : (
                      '-'
                    )}
                  </div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Function</div>
                  <div className="font-mono text-xs">{e!.function_name || '-'}</div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Started</div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">
                    {formatMs(e!.started_at_ms)}
                  </div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">Completed</div>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">
                    {formatMs(e!.completed_at_ms)}
                  </div>
                </div>
              </div>

              <div>
                <div className="text-xs text-[hsl(var(--muted-foreground))]">Log ref</div>
                {e!.log_ref ? (
                  <div className="mt-1 rounded-[var(--radius)] border border-[hsl(var(--border))] bg-[hsl(var(--secondary))] p-3 text-xs">
                    <div className="font-mono">{e!.log_ref.uri_prefix}</div>
                    <div className="mt-2 grid grid-cols-2 gap-2 text-[hsl(var(--muted-foreground))]">
                      <div>backend: {e!.log_ref.backend}</div>
                      <div>content_type: {e!.log_ref.content_type}</div>
                      <div>compression: {e!.log_ref.compression || '-'}</div>
                    </div>
                  </div>
                ) : (
                  <div className="text-sm text-[hsl(var(--muted-foreground))]">-</div>
                )}
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Metadata</CardTitle>
        </CardHeader>
        <CardContent>
          {e ? <KVTable data={e.metadata || {}} /> : null}
        </CardContent>
      </Card>

      <ExecutionLogsDialog
        open={logsOpen}
        onOpenChange={setLogsOpen}
        executionId={id}
      />

      <ReasonConfirmDialog
        open={terminateOpen}
        onOpenChange={setTerminateOpen}
        title="Terminate execution"
        description="This stops the current execution attempt. It does not remove the task replica itself."
        details={[{ label: 'Execution', value: id }]}
        reason={terminateReason}
        onReasonChange={setTerminateReason}
        error={terminateError}
        confirmLabel="Terminate"
        confirmingLabel="Terminating…"
        confirming={terminateLoading}
        onConfirm={async () => {
          setTerminateError('')
          setTerminateLoading(true)
          try {
            const resp = await terminateExecution({
              execution_id: id,
              reason: terminateReason.trim() ? terminateReason.trim() : undefined,
            })
            if (!resp.success) {
              setTerminateError(resp.message || 'Terminate failed')
              return
            }
            setTerminateOpen(false)
            void q.refetch()
          } catch (err) {
            setTerminateError(
              String((err as { message?: string } | null)?.message || err || 'Terminate failed'),
            )
          } finally {
            setTerminateLoading(false)
          }
        }}
      />
    </div>
  )
}

import { Link } from 'react-router-dom'

import type { ExecutionSummary } from '@/api/types'
import { ExecutionStatusBadge } from '@/features/shared/status-badges'
import { cn } from '@/lib/utils'

type ExecutionTableColumn =
  | 'execution'
  | 'task'
  | 'status'
  | 'function'
  | 'instance'
  | 'node'
  | 'started'
  | 'completed'
  | 'duration'

type ExecutionTableProps = {
  rows: ExecutionSummary[]
  columns: ExecutionTableColumn[]
  onRowClick?: (row: ExecutionSummary) => void
}

function formatMs(ts?: number) {
  if (!ts) return '-'
  return new Date(ts).toLocaleString()
}

function durationMs(startedAtMs?: number, completedAtMs?: number) {
  if (!startedAtMs || !completedAtMs || completedAtMs < startedAtMs) return '-'
  return `${completedAtMs - startedAtMs} ms`
}

function headerLabel(column: ExecutionTableColumn) {
  switch (column) {
    case 'execution':
      return 'Execution'
    case 'task':
      return 'Task'
    case 'status':
      return 'Status'
    case 'function':
      return 'Function'
    case 'instance':
      return 'Instance'
    case 'node':
      return 'Node'
    case 'started':
      return 'Started'
    case 'completed':
      return 'Completed'
    case 'duration':
      return 'Duration'
  }
}

function cellContent(row: ExecutionSummary, column: ExecutionTableColumn) {
  switch (column) {
    case 'execution':
      return (
        <Link
          to={`/executions/${encodeURIComponent(row.execution_id)}`}
          className="hover:underline"
          onClick={(event) => event.stopPropagation()}
        >
          {row.execution_id}
        </Link>
      )
    case 'task':
      return row.task_id ? (
        <Link
          to={`/tasks/${encodeURIComponent(row.task_id)}`}
          className="hover:underline"
          onClick={(event) => event.stopPropagation()}
        >
          {row.task_id}
        </Link>
      ) : (
        '-'
      )
    case 'status':
      return <ExecutionStatusBadge status={row.status} />
    case 'function':
      return row.function_name || '-'
    case 'instance':
      return row.instance_id ? (
        <Link
          to={`/instances/${encodeURIComponent(row.instance_id)}`}
          className="hover:underline"
          onClick={(event) => event.stopPropagation()}
        >
          {row.instance_id}
        </Link>
      ) : (
        '-'
      )
    case 'node':
      return row.node_uuid || '-'
    case 'started':
      return formatMs(row.started_at_ms)
    case 'completed':
      return formatMs(row.completed_at_ms)
    case 'duration':
      return durationMs(row.started_at_ms, row.completed_at_ms)
  }
}

function cellClassName(column: ExecutionTableColumn) {
  switch (column) {
    case 'execution':
    case 'task':
    case 'instance':
    case 'node':
      return 'px-3 py-2 font-mono text-xs'
    case 'status':
      return 'px-3 py-2'
    default:
      return 'px-3 py-2 text-xs text-[hsl(var(--muted-foreground))]'
  }
}

export function ExecutionTable(props: ExecutionTableProps) {
  return (
    <div className="overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))]">
      <table className="w-full text-sm">
        <thead className="bg-[hsl(var(--muted))] text-left text-xs text-[hsl(var(--muted-foreground))]">
          <tr>
            {props.columns.map((column) => (
              <th key={column} className="px-3 py-2">
                {headerLabel(column)}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {props.rows.map((row) => (
            <tr
              key={row.execution_id}
              className={cn(
                'border-t border-[hsl(var(--border))]',
                props.onRowClick ? 'cursor-pointer hover:bg-[hsl(var(--accent))]' : undefined,
              )}
              onClick={props.onRowClick ? () => props.onRowClick?.(row) : undefined}
              role={props.onRowClick ? 'button' : undefined}
              tabIndex={props.onRowClick ? 0 : undefined}
              onKeyDown={
                props.onRowClick
                  ? (event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        event.preventDefault()
                        props.onRowClick?.(row)
                      }
                    }
                  : undefined
              }
            >
              {props.columns.map((column) => (
                <td key={column} className={cellClassName(column)}>
                  {cellContent(row, column)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

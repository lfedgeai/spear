import { Badge } from '@/components/ui/badge'

function normalized(status: string) {
  return (status || '').toLowerCase()
}

export function TaskStatusBadge({ status }: { status: string }) {
  const s = normalized(status)
  if (s === 'active' || s === 'registered') return <Badge variant="success">{status}</Badge>
  if (s === 'inactive' || s === 'deleting') return <Badge variant="secondary">{status}</Badge>
  return <Badge variant="destructive">{status || 'unknown'}</Badge>
}

export function ExecutionStatusBadge({ status }: { status: string }) {
  const s = normalized(status)
  if (s === 'completed') return <Badge variant="success">completed</Badge>
  if (s === 'running') return <Badge>running</Badge>
  if (s === 'pending') return <Badge variant="secondary">pending</Badge>
  if (s === 'cancelled' || s === 'timeout') return <Badge variant="secondary">{status}</Badge>
  return <Badge variant="destructive">{status || 'unknown'}</Badge>
}

export function InstanceStatusBadge({ status }: { status: string }) {
  const s = normalized(status)
  if (s === 'running') return <Badge variant="success">running</Badge>
  if (s === 'idle' || s === 'ready') return <Badge>ready</Badge>
  if (s === 'terminating' || s === 'stopping') return <Badge variant="secondary">{status}</Badge>
  if (s === 'terminated' || s === 'absent') return <Badge variant="secondary">{status}</Badge>
  return <Badge variant="destructive">{status || 'unknown'}</Badge>
}

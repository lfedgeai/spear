/**
 * Node status panel for unified AI backend detail pages.
 * 统一 AI backend 详情页的节点状态面板。
 */

import { Link } from 'react-router-dom'

import type { AiBackendNodeStatusSnapshot } from '@/api/ai-backends'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'

function StatusBadge(props: { status: string; available: boolean }) {
  if (props.available && props.status === 'ready') return <Badge variant="success">ready</Badge>
  if (props.status === 'error') return <Badge variant="destructive">error</Badge>
  if (props.status === 'degraded') return <Badge variant="secondary">degraded</Badge>
  return <Badge variant="secondary">{props.status}</Badge>
}

export default function NodeStatusPanel(props: {
  statuses: AiBackendNodeStatusSnapshot[]
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Node Status</CardTitle>
      </CardHeader>
      <CardContent>
        {props.statuses.length > 0 ? (
          <div className="overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))]">
            <table className="w-full text-sm">
              <thead className="bg-[hsl(var(--muted))] text-left text-xs text-[hsl(var(--muted-foreground))]">
                <tr>
                  <th className="px-3 py-2">Node</th>
                  <th className="px-3 py-2">Status</th>
                  <th className="px-3 py-2">Runtime Backend</th>
                  <th className="px-3 py-2">Endpoint</th>
                  <th className="px-3 py-2">Observed Gen</th>
                  <th className="px-3 py-2">Reason</th>
                </tr>
              </thead>
              <tbody>
                {props.statuses.map((status) => (
                  <tr key={`${status.backend_id}:${status.node_uuid}`} className="border-t border-[hsl(var(--border))]">
                    <td className="px-3 py-2 font-mono text-xs">
                      <Link
                        to={`/nodes/${encodeURIComponent(status.node_uuid)}`}
                        className="hover:underline"
                      >
                        {status.node_uuid}
                      </Link>
                    </td>
                    <td className="px-3 py-2">
                      <StatusBadge status={status.status} available={status.available} />
                    </td>
                    <td className="px-3 py-2 font-mono text-xs">
                      {status.runtime_backend_name || '-'}
                    </td>
                    <td className="px-3 py-2 font-mono text-xs">{status.endpoint || '-'}</td>
                    <td className="px-3 py-2 text-xs">{status.observed_generation}</td>
                    <td className="px-3 py-2 text-xs text-[hsl(var(--muted-foreground))]">
                      {status.status_reason || '-'}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="text-sm text-[hsl(var(--muted-foreground))]">No node status reported.</div>
        )}
      </CardContent>
    </Card>
  )
}

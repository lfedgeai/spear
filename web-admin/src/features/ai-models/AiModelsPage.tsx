import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Boxes, Search } from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { toast } from 'sonner'

import { listAiModelViews } from '@/api/ai-backends'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'

function StatusBadge({ status }: { status: 'available' | 'unavailable' }) {
  if (status === 'available') return <Badge variant="success">{status}</Badge>
  return <Badge variant="destructive">{status}</Badge>
}

export default function AiModelsPage({ hosting }: { hosting: 'local' | 'remote' }) {
  const navigate = useNavigate()
  const [q, setQ] = useState('')
  const [status, setStatus] = useState<'available' | 'unavailable' | ''>('')
  const [pageSize, setPageSize] = useState(20)
  const [page, setPage] = useState(1)

  const query = useQuery({
    queryKey: ['ai-models', hosting, q, status, pageSize, page],
    queryFn: () =>
      listAiModelViews({
        hosting,
        q: q || undefined,
        status: status || undefined,
        limit: pageSize,
        offset: (page - 1) * pageSize,
      }),
    refetchInterval: 15_000,
  })

  const rows = query.data?.views || []
  const total = query.data?.total_count || 0
  const totalPages = Math.max(1, Math.ceil(total / pageSize))
  const currentPage = Math.min(page, totalPages)
  const title = useMemo(
    () => `AI Models (${rows.length}/${total})`,
    [rows.length, total],
  )

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-semibold">AI Models</div>
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            {hosting === 'local' ? 'Local' : 'Remote'} read model
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="secondary" onClick={() => navigate('/ai-backends')}>
            <Boxes className="h-4 w-4" />
            Open AI Backends
          </Button>
          <Button
            variant="secondary"
            onClick={() => {
              query
                .refetch()
                .then(() => toast.success('Refreshed'))
                .catch((e) => toast.error((e as Error).message))
            }}
          >
            Refresh
          </Button>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{title}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="mb-3 flex items-center gap-2">
            <div className="relative w-full max-w-md">
              <Search className="absolute left-2 top-2.5 h-4 w-4 text-[hsl(var(--muted-foreground))]" />
              <Input
                placeholder="Search provider/model"
                value={q}
                onChange={(e) => {
                  setQ(e.target.value)
                  setPage(1)
                }}
                className="pl-8"
              />
            </div>
            <select
              className="h-9 rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
              value={status}
              onChange={(e) => {
                setStatus(e.target.value as typeof status)
                setPage(1)
              }}
              aria-label="Status"
            >
              <option value="">All</option>
              <option value="available">available</option>
              <option value="unavailable">unavailable</option>
            </select>
            <select
              className="h-9 rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
              value={String(pageSize)}
              onChange={(e) => {
                setPageSize(Number(e.target.value))
                setPage(1)
              }}
              aria-label="Page size"
            >
              <option value="10">10 / page</option>
              <option value="20">20 / page</option>
              <option value="50">50 / page</option>
              <option value="100">100 / page</option>
            </select>
          </div>

          {query.isError ? (
            <div className="text-sm text-[hsl(var(--destructive))]">
              {(query.error as Error).message}
            </div>
          ) : null}

          <div className="overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))]">
            <table className="w-full text-sm">
              <thead className="bg-[hsl(var(--secondary))]">
                <tr className="text-left">
                  <th className="px-3 py-2">Provider</th>
                  <th className="px-3 py-2">Model</th>
                  <th className="px-3 py-2">Ops</th>
                  <th className="px-3 py-2">Transports</th>
                  <th className="px-3 py-2">Available</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((m) => (
                  <tr
                    key={`${m.hosting}::${m.provider}::${m.model}`}
                    className="cursor-pointer border-t border-[hsl(var(--border))] hover:bg-[hsl(var(--accent))]"
                    onClick={() => {
                      navigate(
                        `/ai-models/${encodeURIComponent(hosting)}/${encodeURIComponent(
                          m.provider,
                        )}/${encodeURIComponent(m.model)}`,
                      )
                    }}
                  >
                    <td className="px-3 py-2 font-medium">{m.provider}</td>
                    <td className="px-3 py-2 font-mono text-xs">{m.model}</td>
                    <td className="px-3 py-2">
                      <div className="flex flex-wrap gap-1">
                        {(m.operations || []).slice(0, 6).map((op) => (
                          <Badge key={op} variant="secondary">
                            {op}
                          </Badge>
                        ))}
                        {(m.operations || []).length > 6 ? (
                          <Badge variant="secondary">...</Badge>
                        ) : null}
                      </div>
                    </td>
                    <td className="px-3 py-2">
                      <div className="flex flex-wrap gap-1">
                        {(m.transports || []).map((t) => (
                          <Badge key={t} variant="secondary">
                            {t}
                          </Badge>
                        ))}
                      </div>
                    </td>
                    <td className="px-3 py-2">
                      <div className="flex items-center gap-2">
                        <StatusBadge
                          status={m.ready_nodes > 0 ? 'available' : 'unavailable'}
                        />
                        <span className="text-[hsl(var(--muted-foreground))]">
                          {m.ready_nodes}/{m.total_nodes}
                        </span>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {total > pageSize ? (
            <div className="mt-3 flex items-center justify-between gap-3 text-sm">
              <div className="text-[hsl(var(--muted-foreground))]">
                Page {currentPage} / {totalPages} in {hosting} ({total} matched)
              </div>
              <div className="flex items-center gap-2">
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={currentPage <= 1}
                  onClick={() => setPage((current) => Math.max(1, current - 1))}
                >
                  Previous
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={currentPage >= totalPages}
                  onClick={() => setPage((current) => Math.min(totalPages, current + 1))}
                >
                  Next
                </Button>
              </div>
            </div>
          ) : null}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Control Plane</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            AI Models is now a read-only aggregate view. Create, edit, place, enable, disable,
            and delete backends from AI Backends.
          </div>
          <div className="flex items-center gap-2">
            <Button variant="secondary" onClick={() => navigate('/ai-backends')}>
              Open AI Backends
            </Button>
            {hosting === 'remote' ? (
              <Button variant="secondary" onClick={() => navigate('/ai-backends/credentials')}>
                Open Credentials
              </Button>
            ) : null}
          </div>
        </CardContent>
      </Card>
    </div>
  )
}

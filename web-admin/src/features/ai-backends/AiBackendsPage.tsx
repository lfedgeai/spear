/**
 * Unified AI backends list page.
 * 统一 AI backend 列表页。
 */

import { Link } from 'react-router-dom'
import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'

import {
  createAiBackend,
  deleteAiBackend,
  listAiBackends,
  listAiBackendNodeStatuses,
  listAiModelViews,
  preflightAiBackend,
  setAiBackendDesiredState,
  upsertAiBackendPlacement,
  updateAiBackend,
  type AiBackendNodeStatusSnapshot,
  type AiBackendSummary,
  type WriteAiBackendInput,
} from '@/api/ai-backends'
import { listNodes } from '@/api/nodes'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'

import AiBackendEditorDialog from './AiBackendEditorDialog'
import LocalBackendCreateDialog from './LocalBackendCreateDialog'
import type { PlacementPolicyInput } from './AiBackendEditorForm'
import RemoteBackendCreateDialog from './RemoteBackendCreateDialog'

function StateBadge(props: { state: string }) {
  if (props.state === 'enabled') return <Badge variant="success">enabled</Badge>
  if (props.state === 'disabled') return <Badge variant="secondary">disabled</Badge>
  return <Badge variant="secondary">{props.state}</Badge>
}

function AvailabilityBadge(props: { readyNodes: number }) {
  if (props.readyNodes > 0) return <Badge variant="success">available</Badge>
  return <Badge variant="secondary">unavailable</Badge>
}

function summarizeBackendStatusReason(statuses: AiBackendNodeStatusSnapshot[] | undefined): string {
  if (!statuses || statuses.length === 0) {
    return 'no node status'
  }

  const priorityOrder: Array<AiBackendNodeStatusSnapshot['status']> = [
    'error',
    'degraded',
    'reconciling',
    'pending',
    'disabled',
    'ready',
    'unspecified',
  ]

  const sorted = [...statuses].sort(
    (left, right) => priorityOrder.indexOf(left.status) - priorityOrder.indexOf(right.status),
  )
  const primary = sorted[0]
  const normalizedReason = primary.status_reason?.trim()
  const sameReasonCount = statuses.filter((status) => {
    return (
      status.status === primary.status &&
      (status.status_reason?.trim() || '') === (normalizedReason || '')
    )
  }).length

  const label = normalizedReason || primary.status
  if (statuses.length === 1) {
    return label
  }
  if (sameReasonCount > 1) {
    return `${label} (${sameReasonCount} nodes)`
  }
  return `${label} (${primary.node_uuid})`
}

export default function AiBackendsPage() {
  const queryClient = useQueryClient()
  const [editDialogOpen, setEditDialogOpen] = useState(false)
  const [remoteCreateOpen, setRemoteCreateOpen] = useState(false)
  const [localCreateOpen, setLocalCreateOpen] = useState(false)
  const [editingBackend, setEditingBackend] = useState<AiBackendSummary | null>(null)
  const [createMenuOpen, setCreateMenuOpen] = useState(false)
  const [viewMode, setViewMode] = useState<'backends' | 'models'>('backends')
  const [search, setSearch] = useState('')
  const [hostingFilter, setHostingFilter] = useState<'all' | 'remote' | 'local'>('all')
  const [stateFilter, setStateFilter] = useState<'all' | 'enabled' | 'disabled'>('all')
  const [availabilityFilter, setAvailabilityFilter] = useState<'all' | 'available' | 'unavailable'>(
    'all',
  )
  const [pageSize, setPageSize] = useState(20)
  const [page, setPage] = useState(1)

  const query = useQuery({
    queryKey: ['ai-backends', search, hostingFilter, stateFilter, pageSize, page],
    queryFn: () =>
      listAiBackends({
        q: search || undefined,
        hosting: hostingFilter === 'all' ? undefined : hostingFilter,
        desired_state: stateFilter === 'all' ? undefined : stateFilter,
        limit: pageSize,
        offset: (page - 1) * pageSize,
      }),
    refetchInterval: 15_000,
  })
  const modelQuery = useQuery({
    queryKey: ['ai-model-views', search, hostingFilter, availabilityFilter, pageSize, page],
    queryFn: () =>
      listAiModelViews({
        q: search || undefined,
        hosting: hostingFilter === 'all' ? undefined : hostingFilter,
        status: availabilityFilter === 'all' ? undefined : availabilityFilter,
        limit: pageSize,
        offset: (page - 1) * pageSize,
      }),
    refetchInterval: 15_000,
  })

  const createMutation = useMutation({
    mutationFn: async (input: { backend: WriteAiBackendInput; placementPolicy?: PlacementPolicyInput }) => {
      const targetNodeUuids =
        input.placementPolicy?.scope === 'all_nodes'
          ? (await listNodes({ sort_by: 'last_heartbeat', order: 'desc', limit: 200 })).nodes.map(
              (node) => node.uuid,
            )
          : input.placementPolicy?.node_uuids || []

      if (input.placementPolicy && targetNodeUuids.length === 0) {
        throw new Error('No nodes available for placement')
      }

      const supportsRemotePreflight =
        input.backend.hosting === 'remote' &&
        ['openai', 'openai_compatible', 'ollama'].includes(input.backend.provider)
      const supportsLocalPreflight =
        input.backend.hosting === 'local' && input.backend.provider === 'llamacpp'
      if ((supportsRemotePreflight || supportsLocalPreflight) && targetNodeUuids.length > 0) {
        const preflight = await preflightAiBackend({
          backend: input.backend,
          node_uuids: targetNodeUuids,
          verification_policy:
            targetNodeUuids.length <= 1 ? 'single_node_strict' : 'sampled_strict',
          requested_checks:
            input.backend.hosting === 'remote'
              ? ['connectivity', 'auth', 'model_access']
              : undefined,
        })
        if (!preflight.success) {
          throw new Error(preflight.message || 'AI backend preflight failed')
        }
      }

      const response = await createAiBackend(input.backend)
      if (!response.success) throw new Error(response.message || 'Failed to create AI backend')
      const backendId = response.backend?.backend_id
      if (!backendId) throw new Error('Backend created without backend_id')

      if (input.placementPolicy) {
        await Promise.all(
          targetNodeUuids.map(async (nodeUuid) => {
            const placementResponse = await upsertAiBackendPlacement({
              backend_id: backendId,
              node_uuid: nodeUuid,
              desired_state: input.placementPolicy?.desired_state,
              weight_override: input.placementPolicy?.weight_override,
              priority_override: input.placementPolicy?.priority_override,
            })
            if (!placementResponse.success) {
              throw new Error(placementResponse.message || `Failed to create placement for ${nodeUuid}`)
            }
          }),
        )
      }
      return response
    },
    onSuccess: async () => {
      toast.success('AI backend created')
      await query.refetch()
      await queryClient.invalidateQueries({ queryKey: ['ai-model-views'] })
    },
  })

  const updateMutation = useMutation({
    mutationFn: async (input: { backendId: string; payload: WriteAiBackendInput }) => {
      const response = await updateAiBackend(input.backendId, input.payload)
      if (!response.success) throw new Error(response.message || 'Failed to update AI backend')
      return response
    },
    onSuccess: async () => {
      toast.success('AI backend updated')
      await query.refetch()
      await queryClient.invalidateQueries({ queryKey: ['ai-model-views'] })
    },
  })

  const deleteMutation = useMutation({
    mutationFn: async (backendId: string) => {
      const response = await deleteAiBackend(backendId)
      if (!response.success) throw new Error(response.message || 'Failed to delete AI backend')
      return response
    },
    onSuccess: async () => {
      toast.success('AI backend deleted')
      await query.refetch()
      await queryClient.invalidateQueries({ queryKey: ['ai-model-views'] })
    },
  })

  const stateMutation = useMutation({
    mutationFn: async (input: { backendId: string; desiredState: 'enabled' | 'disabled' }) => {
      const response = await setAiBackendDesiredState(input.backendId, input.desiredState)
      if (!response.success) {
        throw new Error(response.message || 'Failed to change desired state')
      }
      return response
    },
    onSuccess: async () => {
      toast.success('Desired state updated')
      await query.refetch()
      await queryClient.invalidateQueries({ queryKey: ['ai-model-views'] })
    },
  })

  const backends = query.data?.backends || []
  const modelViews = modelQuery.data?.views || []
  const statusReasonQuery = useQuery({
    queryKey: ['ai-backend-status-reasons', backends.map((backend) => backend.backend_id)],
    enabled: viewMode === 'backends' && backends.length > 0,
    refetchInterval: 15_000,
    queryFn: async () => {
      const entries = await Promise.all(
        backends.map(async (backend) => {
          const response = await listAiBackendNodeStatuses(backend.backend_id)
          return [
            backend.backend_id,
            summarizeBackendStatusReason(response.success ? response.statuses : undefined),
          ] as const
        }),
      )
      return Object.fromEntries(entries)
    },
  })
  const statusReasonByBackendId = statusReasonQuery.data || {}
  const totalCount =
    viewMode === 'backends' ? query.data?.total_count || 0 : modelQuery.data?.total_count || 0
  const totalPages = Math.max(1, Math.ceil(totalCount / pageSize))
  const currentPage = Math.min(page, totalPages)

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-semibold">AI Backends</div>
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            Control-plane write resources with an embedded aggregated model view.
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            onClick={() => (viewMode === 'backends' ? query.refetch() : modelQuery.refetch())}
          >
            Refresh
          </Button>
          <div className="relative">
            <Button onClick={() => setCreateMenuOpen((open) => !open)}>Create Backend</Button>
            {createMenuOpen ? (
              <div className="absolute right-0 z-10 mt-2 w-56 rounded-[var(--radius)] border border-[hsl(var(--border))] bg-[hsl(var(--background))] p-1 shadow-md">
                <button
                  className="flex w-full flex-col rounded-[calc(var(--radius)-4px)] px-3 py-2 text-left hover:bg-[hsl(var(--muted))]"
                  onClick={() => {
                    setCreateMenuOpen(false)
                    setEditingBackend(null)
                    setRemoteCreateOpen(true)
                  }}
                >
                  <span className="text-sm font-medium">Create Remote Backend</span>
                  <span className="text-xs text-[hsl(var(--muted-foreground))]">
                    For OpenAI, Ollama, and other external endpoints.
                  </span>
                </button>
                <button
                  className="flex w-full flex-col rounded-[calc(var(--radius)-4px)] px-3 py-2 text-left hover:bg-[hsl(var(--muted))]"
                  onClick={() => {
                    setCreateMenuOpen(false)
                    setEditingBackend(null)
                    setLocalCreateOpen(true)
                  }}
                >
                  <span className="text-sm font-medium">Create Local Backend</span>
                  <span className="text-xs text-[hsl(var(--muted-foreground))]">
                    For node-local runtimes like llama.cpp and vLLM.
                  </span>
                </button>
              </div>
            ) : null}
          </div>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>View</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-wrap items-center gap-2">
          <Button
            variant={viewMode === 'backends' ? 'default' : 'secondary'}
            onClick={() => {
              setViewMode('backends')
              setPage(1)
            }}
          >
            Backends
          </Button>
          <Button
            variant={viewMode === 'models' ? 'default' : 'secondary'}
            onClick={() => {
              setViewMode('models')
              setPage(1)
            }}
          >
            Model Views
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Filters</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-3 md:grid-cols-4">
          <div className="space-y-1">
            <div className="text-sm font-medium">Search</div>
            <Input
              value={search}
              onChange={(event) => {
                setSearch(event.target.value)
                setPage(1)
              }}
              placeholder="name / provider / model / backend id"
            />
          </div>
          <div className="space-y-1">
            <div className="text-sm font-medium">Hosting</div>
            <select
              className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
              value={hostingFilter}
              onChange={(event) => {
                setHostingFilter(event.target.value as 'all' | 'remote' | 'local')
                setPage(1)
              }}
            >
              <option value="all">all</option>
              <option value="remote">remote</option>
              <option value="local">local</option>
            </select>
          </div>
          <div className="space-y-1">
            <div className="text-sm font-medium">
              {viewMode === 'backends' ? 'Desired state' : 'Availability'}
            </div>
            <select
              className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
              value={viewMode === 'backends' ? stateFilter : availabilityFilter}
              onChange={(event) => {
                if (viewMode === 'backends') {
                  setStateFilter(event.target.value as 'all' | 'enabled' | 'disabled')
                } else {
                  setAvailabilityFilter(event.target.value as 'all' | 'available' | 'unavailable')
                }
                setPage(1)
              }}
            >
              <option value="all">all</option>
              {viewMode === 'backends' ? (
                <>
                  <option value="enabled">enabled</option>
                  <option value="disabled">disabled</option>
                </>
              ) : (
                <>
                  <option value="available">available</option>
                  <option value="unavailable">unavailable</option>
                </>
              )}
            </select>
          </div>
          <div className="space-y-1">
            <div className="text-sm font-medium">Page size</div>
            <select
              className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
              value={String(pageSize)}
              onChange={(event) => {
                setPageSize(Number(event.target.value))
                setPage(1)
              }}
            >
              <option value="10">10</option>
              <option value="20">20</option>
              <option value="50">50</option>
              <option value="100">100</option>
            </select>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>
            {viewMode === 'backends'
              ? `Backends (${backends.length}/${totalCount})`
              : `Model Views (${modelViews.length}/${totalCount})`}
          </CardTitle>
        </CardHeader>
        <CardContent>
          {viewMode === 'backends' && query.isLoading ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">Loading…</div>
          ) : viewMode === 'models' && modelQuery.isLoading ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">Loading…</div>
          ) : viewMode === 'backends' && query.isError ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">Failed to load.</div>
          ) : viewMode === 'models' && modelQuery.isError ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">Failed to load.</div>
          ) : viewMode === 'backends' && !query.data?.success ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">
              {query.data?.message || 'Failed to load.'}
            </div>
          ) : viewMode === 'models' && !modelQuery.data?.success ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">
              {modelQuery.data?.message || 'Failed to load.'}
            </div>
          ) : totalCount > 0 && viewMode === 'backends' ? (
            <div className="overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))]">
              <table className="w-full text-sm">
                <thead className="bg-[hsl(var(--muted))] text-left text-xs text-[hsl(var(--muted-foreground))]">
                  <tr>
                    <th className="px-3 py-2">Name</th>
                    <th className="px-3 py-2">Provider / Model</th>
                    <th className="px-3 py-2">Hosting</th>
                    <th className="px-3 py-2">Kind</th>
                    <th className="px-3 py-2">Desired State</th>
                    <th className="px-3 py-2">Status reason</th>
                    <th className="px-3 py-2">Generation</th>
                    <th className="px-3 py-2">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {backends.map((backend) => (
                    <tr key={backend.backend_id} className="border-t border-[hsl(var(--border))]">
                      <td className="px-3 py-2">
                        <div className="font-medium">
                          <Link
                            to={`/ai-backends/${encodeURIComponent(backend.backend_id)}`}
                            className="hover:underline"
                          >
                            {backend.display_name}
                          </Link>
                        </div>
                        <div className="font-mono text-xs text-[hsl(var(--muted-foreground))]">
                          {backend.backend_id}
                        </div>
                      </td>
                      <td className="px-3 py-2">
                        <div className="font-mono text-xs">{backend.provider}</div>
                        <div className="font-mono text-xs text-[hsl(var(--muted-foreground))]">
                          {backend.model}
                        </div>
                      </td>
                      <td className="px-3 py-2 font-mono text-xs">{backend.hosting}</td>
                      <td className="px-3 py-2 font-mono text-xs">{backend.backend_kind}</td>
                      <td className="px-3 py-2">
                        <StateBadge state={backend.desired_state} />
                      </td>
                      <td className="px-3 py-2">
                        <div className="max-w-[240px] truncate text-xs text-[hsl(var(--muted-foreground))]">
                          {statusReasonQuery.isLoading
                            ? 'Loading…'
                            : statusReasonByBackendId[backend.backend_id] || 'no node status'}
                        </div>
                      </td>
                      <td className="px-3 py-2 text-xs">{backend.generation}</td>
                      <td className="px-3 py-2">
                        <div className="flex flex-wrap items-center gap-2">
                          <Button
                            variant="secondary"
                            size="sm"
                            onClick={() => {
                              setEditingBackend(backend)
                            setEditDialogOpen(true)
                            }}
                          >
                            Edit
                          </Button>
                          <Button
                            variant="secondary"
                            size="sm"
                            onClick={async () => {
                              await stateMutation.mutateAsync({
                                backendId: backend.backend_id,
                                desiredState:
                                  backend.desired_state === 'enabled' ? 'disabled' : 'enabled',
                              })
                            }}
                          >
                            {backend.desired_state === 'enabled' ? 'Disable' : 'Enable'}
                          </Button>
                          <Button
                            variant="destructive"
                            size="sm"
                            onClick={async () => {
                              if (!window.confirm(`Delete backend ${backend.display_name}?`)) return
                              await deleteMutation.mutateAsync(backend.backend_id)
                            }}
                          >
                            Delete
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : totalCount > 0 && viewMode === 'models' ? (
            <div className="overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))]">
              <table className="w-full text-sm">
                <thead className="bg-[hsl(var(--muted))] text-left text-xs text-[hsl(var(--muted-foreground))]">
                  <tr>
                    <th className="px-3 py-2">Provider / Model</th>
                    <th className="px-3 py-2">Hosting</th>
                    <th className="px-3 py-2">Availability</th>
                    <th className="px-3 py-2">Ready Nodes</th>
                    <th className="px-3 py-2">Enabled Nodes</th>
                    <th className="px-3 py-2">Backends</th>
                    <th className="px-3 py-2">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {modelViews.map((view) => (
                    <tr
                      key={`${view.hosting}:${view.provider}:${view.model}`}
                      className="border-t border-[hsl(var(--border))]"
                    >
                      <td className="px-3 py-2">
                        <div className="font-medium">{view.provider}</div>
                        <div className="font-mono text-xs text-[hsl(var(--muted-foreground))]">
                          {view.model}
                        </div>
                      </td>
                      <td className="px-3 py-2 font-mono text-xs">{view.hosting}</td>
                      <td className="px-3 py-2">
                        <AvailabilityBadge readyNodes={view.ready_nodes} />
                      </td>
                      <td className="px-3 py-2 text-xs">
                        {view.ready_nodes} / {view.total_nodes}
                      </td>
                      <td className="px-3 py-2 text-xs">{view.enabled_nodes}</td>
                      <td className="px-3 py-2 text-xs">{view.backend_ids.length}</td>
                      <td className="px-3 py-2">
                        <Link
                          to={`/ai-models/${encodeURIComponent(view.hosting)}/${encodeURIComponent(view.provider)}/${encodeURIComponent(view.model)}`}
                          className="text-sm text-[hsl(var(--primary))] hover:underline"
                        >
                          View details
                        </Link>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (viewMode === 'backends' && query.data?.success) ||
            (viewMode === 'models' && modelQuery.data?.success) ? (
            <div className="text-sm text-[hsl(var(--muted-foreground))]">
              {viewMode === 'backends'
                ? 'No AI backends matched the current filters.'
                : 'No model views matched the current filters.'}
            </div>
          ) : null}
          {totalCount > pageSize ? (
            <div className="mt-3 flex items-center justify-between gap-3 text-sm">
              <div className="text-[hsl(var(--muted-foreground))]">
                Page {currentPage} / {totalPages}
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

      <RemoteBackendCreateDialog
        open={remoteCreateOpen}
        onOpenChange={setRemoteCreateOpen}
        onSubmit={async (input, placementPolicy) => {
          await createMutation.mutateAsync({ backend: input, placementPolicy })
        }}
      />

      <LocalBackendCreateDialog
        open={localCreateOpen}
        onOpenChange={setLocalCreateOpen}
        onSubmit={async (input, placementPolicy) => {
          await createMutation.mutateAsync({ backend: input, placementPolicy })
        }}
      />

      <AiBackendEditorDialog
        open={editDialogOpen}
        backend={editingBackend}
        onOpenChange={setEditDialogOpen}
        onSubmit={async (input, placementPolicy) => {
          if (editingBackend?.backend_id) {
            await updateMutation.mutateAsync({
              backendId: editingBackend.backend_id,
              payload: input,
            })
            return
          }
          await createMutation.mutateAsync({ backend: input, placementPolicy })
        }}
      />
    </div>
  )
}

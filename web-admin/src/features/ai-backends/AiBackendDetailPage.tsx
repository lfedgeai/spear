/**
 * Unified AI backend detail page.
 * 统一 AI backend 详情页。
 */

import { Link, useParams } from 'react-router-dom'
import { useState, type ReactNode } from 'react'
import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query'
import { Copy } from 'lucide-react'
import { toast } from 'sonner'

import {
  deleteAiBackend,
  getAiBackend,
  type AiBackendRemoteInput,
  listAiBackendNodeStatuses,
  listAiBackendPlacements,
  listAiModelViews,
  setAiBackendDesiredState,
  updateAiBackend,
  type AiBackendSummary,
  type AiModelView,
  type WriteAiBackendInput,
} from '@/api/ai-backends'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'

import AiBackendEditorDialog from './AiBackendEditorDialog'
import NodeStatusPanel from './NodeStatusPanel'
import PlacementPanel from './PlacementPanel'

function StateBadge(props: { state: string }) {
  if (props.state === 'enabled') return <Badge variant="success">enabled</Badge>
  if (props.state === 'disabled') return <Badge variant="secondary">disabled</Badge>
  return <Badge variant="secondary">{props.state}</Badge>
}

function SummaryRow(props: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3 text-sm">
      <span className="text-[hsl(var(--muted-foreground))]">{props.label}</span>
      <span>{props.value}</span>
    </div>
  )
}

type BackendDetailSummaryView = {
  operations: string[]
  features: string[]
  transports: string[]
  baseUrl: string
  credentialRef: string
}

type BackendDetailProviderField = {
  label: string
  value: string
}

function isOpenAiRemoteInput(
  value?: AiBackendRemoteInput,
): value is Extract<AiBackendRemoteInput, { provider_family: 'open_ai_compatible' }> {
  return value?.provider_family === 'open_ai_compatible'
}

function isOllamaRemoteInput(
  value?: AiBackendRemoteInput,
): value is Extract<AiBackendRemoteInput, { provider_family: 'ollama' }> {
  return value?.provider_family === 'ollama'
}

function isKnownRemoteProvider(provider: string): boolean {
  return provider === 'openai' || provider === 'ollama'
}

export function resolveBackendDetailSummary(backend: AiBackendSummary): BackendDetailSummaryView {
  const remoteOpenAi = isOpenAiRemoteInput(backend.remote) ? backend.remote : undefined
  const remoteOllama = isOllamaRemoteInput(backend.remote) ? backend.remote : undefined
  const normalizedProvider = backend.provider.trim().toLowerCase()

  return {
    operations: isKnownRemoteProvider(normalizedProvider)
      ? remoteOpenAi?.config.operations || remoteOllama?.config.operations || []
      : backend.spec?.operations || [],
    features: isKnownRemoteProvider(normalizedProvider)
      ? remoteOpenAi?.config.features || remoteOllama?.config.features || []
      : backend.spec?.features || [],
    transports: isKnownRemoteProvider(normalizedProvider)
      ? remoteOpenAi?.config.transports || remoteOllama?.config.transports || []
      : backend.spec?.transports || [],
    baseUrl: isKnownRemoteProvider(normalizedProvider)
      ? remoteOpenAi?.config.base_url || remoteOllama?.config.base_url || ''
      : backend.spec?.base_url || '',
    credentialRef: normalizedProvider === 'openai'
      ? remoteOpenAi?.config.credential_ref || ''
      : !isKnownRemoteProvider(normalizedProvider)
        ? backend.credential_ref || backend.spec?.credential_ref || ''
        : '',
  }
}

export function resolveBackendProviderFields(backend: AiBackendSummary): BackendDetailProviderField[] {
  if (backend.local?.provider_family === 'llama_cpp') {
    const fields: BackendDetailProviderField[] = []
    const config = backend.local.config
    if (config.model_url) fields.push({ label: 'Model URL', value: config.model_url })
    if (config.model_path) fields.push({ label: 'Model Path', value: config.model_path })
    if (config.skip_download !== undefined) {
      fields.push({ label: 'Skip download', value: String(config.skip_download) })
    }
    if (config.download_timeout_s !== undefined) {
      fields.push({ label: 'Download timeout', value: String(config.download_timeout_s) })
    }
    if (config.threads !== undefined) fields.push({ label: 'Threads', value: String(config.threads) })
    if (config.ctx_size !== undefined) {
      fields.push({ label: 'Context size', value: String(config.ctx_size) })
    }
    return fields
  }

  if (backend.local?.provider_family === 'vllm') {
    const fields: BackendDetailProviderField[] = []
    const config = backend.local.config
    if (config.mode) fields.push({ label: 'Mode', value: config.mode })
    if (config.managed_externally !== undefined) {
      fields.push({ label: 'Managed externally', value: String(config.managed_externally) })
    }
    return fields
  }

  if (backend.remote?.provider_family === 'open_ai_compatible') {
    return [{ label: 'Provider family', value: 'open_ai_compatible' }]
  }

  if (backend.remote?.provider_family === 'ollama') {
    return [{ label: 'Provider family', value: 'ollama' }]
  }

  return []
}

function RelatedModelViewsPanel(props: {
  backendId: string
  provider: string
  model: string
  hosting: string
  views: AiModelView[]
}) {
  const matchedViews = props.views.filter(
    (view) =>
      view.provider === props.provider &&
      view.model === props.model &&
      view.hosting === props.hosting &&
      view.backend_ids.includes(props.backendId),
  )

  return (
    <Card>
      <CardHeader>
        <CardTitle>Read Model Views</CardTitle>
      </CardHeader>
      <CardContent>
        {matchedViews.length > 0 ? (
          <div className="space-y-3">
            {matchedViews.map((view) => (
              <div
                key={`${view.hosting}:${view.provider}:${view.model}`}
                className="rounded-[var(--radius)] border border-[hsl(var(--border))] p-3"
              >
                <div className="flex items-center justify-between text-sm">
                  <span className="font-medium">
                    {view.provider} / {view.model}
                  </span>
                  <span className="font-mono text-xs">{view.hosting}</span>
                </div>
                <div className="mt-2 grid grid-cols-3 gap-3 text-sm">
                  <SummaryRow label="Enabled nodes" value={view.enabled_nodes} />
                  <SummaryRow label="Ready nodes" value={view.ready_nodes} />
                  <SummaryRow label="Total nodes" value={view.total_nodes} />
                </div>
              </div>
            ))}
          </div>
        ) : (
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            No related read-model rows yet.
          </div>
        )}
      </CardContent>
    </Card>
  )
}

export default function AiBackendDetailPage() {
  const { backendId = '' } = useParams()
  const [dialogOpen, setDialogOpen] = useState(false)
  const queryClient = useQueryClient()

  const detailQuery = useQuery({
    queryKey: ['ai-backend', backendId],
    queryFn: () => getAiBackend(backendId),
    enabled: !!backendId,
    refetchInterval: 15_000,
  })

  const [placementsQuery, statusesQuery, modelViewsQuery] = useQueries({
    queries: [
      {
        queryKey: ['ai-backend-placements', backendId],
        queryFn: () => listAiBackendPlacements({ backend_id: backendId }),
        enabled: !!backendId,
        refetchInterval: 15_000,
      },
      {
        queryKey: ['ai-backend-statuses', backendId],
        queryFn: () => listAiBackendNodeStatuses(backendId),
        enabled: !!backendId,
        refetchInterval: 15_000,
      },
      {
        queryKey: ['ai-model-views'],
        queryFn: () => listAiModelViews(),
        enabled: !!backendId,
        refetchInterval: 15_000,
      },
    ],
  })

  const backend = detailQuery.data?.backend || null
  const detailSummary = backend ? resolveBackendDetailSummary(backend) : null
  const providerFields = backend ? resolveBackendProviderFields(backend) : []

  const updateMutation = useMutation({
    mutationFn: async (input: WriteAiBackendInput) => {
      const response = await updateAiBackend(backendId, input)
      if (!response.success) throw new Error(response.message || 'Failed to update AI backend')
      return response
    },
    onSuccess: async () => {
      toast.success('AI backend updated')
      await Promise.all([
        detailQuery.refetch(),
        placementsQuery.refetch(),
        statusesQuery.refetch(),
        modelViewsQuery.refetch(),
      ])
      await queryClient.invalidateQueries({ queryKey: ['ai-backends'] })
    },
  })

  const stateMutation = useMutation({
    mutationFn: async (desiredState: 'enabled' | 'disabled') => {
      const response = await setAiBackendDesiredState(backendId, desiredState)
      if (!response.success) throw new Error(response.message || 'Failed to change desired state')
      return response
    },
    onSuccess: async () => {
      toast.success('Desired state updated')
      await Promise.all([
        detailQuery.refetch(),
        placementsQuery.refetch(),
        statusesQuery.refetch(),
        modelViewsQuery.refetch(),
      ])
      await queryClient.invalidateQueries({ queryKey: ['ai-backends'] })
    },
  })

  const deleteMutation = useMutation({
    mutationFn: async () => {
      const response = await deleteAiBackend(backendId)
      if (!response.success) throw new Error(response.message || 'Failed to delete AI backend')
      return response
    },
    onSuccess: async () => {
      toast.success('AI backend deleted')
      window.location.hash = '#/ai-backends'
    },
  })

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-semibold">
            {backend?.display_name || 'AI Backend'}
          </div>
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            <Link to="/ai-backends" className="hover:underline">
              AI Backends
            </Link>
            <span className="mx-2">/</span>
            <span className="font-mono text-xs">{backendId}</span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="secondary" onClick={() => detailQuery.refetch()}>
            Refresh
          </Button>
          {backend ? (
            <Button
              variant="secondary"
              onClick={async () => {
                await navigator.clipboard.writeText(backend.backend_id)
                toast.success('Copied')
              }}
            >
              <Copy className="h-4 w-4" />
              Copy ID
            </Button>
          ) : null}
          {backend ? (
            <Button variant="secondary" onClick={() => setDialogOpen(true)}>
              Edit
            </Button>
          ) : null}
          {backend ? (
            <Button
              variant="secondary"
              onClick={async () => {
                await stateMutation.mutateAsync(
                  backend.desired_state === 'enabled' ? 'disabled' : 'enabled',
                )
              }}
            >
              {backend.desired_state === 'enabled' ? 'Disable' : 'Enable'}
            </Button>
          ) : null}
          {backend ? (
            <Button
              variant="destructive"
              onClick={async () => {
                if (!window.confirm(`Delete backend ${backend.display_name}?`)) return
                await deleteMutation.mutateAsync()
              }}
            >
              Delete
            </Button>
          ) : null}
        </div>
      </div>

      {detailQuery.isLoading ? (
        <div className="text-sm text-[hsl(var(--muted-foreground))]">Loading…</div>
      ) : detailQuery.isError ? (
        <div className="text-sm text-[hsl(var(--muted-foreground))]">Failed to load.</div>
      ) : !detailQuery.data?.success ? (
        <div className="text-sm text-[hsl(var(--muted-foreground))]">
          {detailQuery.data?.message || 'Failed to load.'}
        </div>
      ) : !detailQuery.data.found || !backend ? (
        <div className="text-sm text-[hsl(var(--muted-foreground))]">Not found</div>
      ) : (
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-3">
            <Card>
              <CardHeader>
                <CardTitle>Summary</CardTitle>
              </CardHeader>
              <CardContent className="space-y-2">
                <SummaryRow label="Backend ID" value={<span className="font-mono text-xs">{backend.backend_id}</span>} />
                <SummaryRow label="Display name" value={backend.display_name} />
                <SummaryRow label="Desired state" value={<StateBadge state={backend.desired_state} />} />
                <SummaryRow label="Hosting" value={<span className="font-mono text-xs">{backend.hosting}</span>} />
                <SummaryRow label="Provider" value={<span className="font-mono text-xs">{backend.provider}</span>} />
                <SummaryRow label="Model" value={<span className="font-mono text-xs">{backend.model}</span>} />
                <SummaryRow label="Kind" value={<span className="font-mono text-xs">{backend.backend_kind}</span>} />
                <SummaryRow label="Generation" value={backend.generation} />
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Spec</CardTitle>
              </CardHeader>
              <CardContent className="space-y-2">
                <SummaryRow
                  label="Operations"
                  value={<span className="font-mono text-xs">{detailSummary?.operations.join(', ') || '-'}</span>}
                />
                <SummaryRow
                  label="Features"
                  value={<span className="font-mono text-xs">{detailSummary?.features.join(', ') || '-'}</span>}
                />
                <SummaryRow
                  label="Transports"
                  value={<span className="font-mono text-xs">{detailSummary?.transports.join(', ') || '-'}</span>}
                />
                <SummaryRow
                  label="Base URL"
                  value={<span className="font-mono text-xs">{detailSummary?.baseUrl || '-'}</span>}
                />
                <SummaryRow
                  label="Credential ref"
                  value={<span className="font-mono text-xs">{detailSummary?.credentialRef || '-'}</span>}
                />
                <SummaryRow label="Weight" value={backend.spec?.weight ?? '-'} />
                <SummaryRow label="Priority" value={backend.spec?.priority ?? '-'} />
              </CardContent>
            </Card>
          </div>

          {providerFields.length > 0 ? (
            <Card>
              <CardHeader>
                <CardTitle>Provider Config</CardTitle>
              </CardHeader>
              <CardContent className="space-y-2">
                {providerFields.map((field) => (
                  <SummaryRow
                    key={field.label}
                    label={field.label}
                    value={<span className="font-mono text-xs">{field.value || '-'}</span>}
                  />
                ))}
              </CardContent>
            </Card>
          ) : null}

          <PlacementPanel
            backendId={backend.backend_id}
            placements={placementsQuery.data?.placements || []}
            onRefetch={async () => {
              await Promise.all([
                placementsQuery.refetch(),
                statusesQuery.refetch(),
                modelViewsQuery.refetch(),
                detailQuery.refetch(),
              ])
            }}
          />

          <NodeStatusPanel statuses={statusesQuery.data?.statuses || []} />

          <RelatedModelViewsPanel
            backendId={backend.backend_id}
            provider={backend.provider}
            model={backend.model}
            hosting={backend.hosting}
            views={modelViewsQuery.data?.views || []}
          />

          <Card>
            <CardHeader>
              <CardTitle>Raw JSON</CardTitle>
            </CardHeader>
            <CardContent>
              <pre className="max-h-[520px] overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))] bg-[hsl(var(--secondary))] p-3 text-xs">
                {JSON.stringify(backend, null, 2)}
              </pre>
            </CardContent>
          </Card>
        </div>
      )}

      <AiBackendEditorDialog
        open={dialogOpen}
        backend={backend}
        onOpenChange={setDialogOpen}
        onSubmit={async (input) => {
          await updateMutation.mutateAsync(input)
        }}
      />
    </div>
  )
}

/**
 * Editor dialog for unified AI backends.
 * 统一 AI backend 的编辑对话框。
 */

import { useEffect, useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { toast } from 'sonner'

import type { AiBackendSummary, AiBackendDesiredState, AiBackendHosting, WriteAiBackendInput } from '@/api/ai-backends'
import { listNodes } from '@/api/nodes'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import {
  backendKindOptionsFor,
  buildPayload,
  buildPlacementPolicy,
  defaultBackendKindFor,
  defaultPlacementScopeForHosting,
  defaultProviderForHosting,
  defaultsForKind,
  FEATURE_OPTIONS,
  formFromBackend,
  type AiBackendEditorFormState as FormState,
  metadataBooleanField,
  metadataStringField,
  OPERATION_OPTIONS,
  type PlacementPolicyInput,
  providerOptionsForHosting,
  setMetadataStringField,
  splitCsv,
  toggleCsvValue,
  TRANSPORT_OPTIONS,
  emptyForm,
  validateForm,
} from './AiBackendEditorForm'
import LocalLlamaCppFields from './LocalLlamaCppFields'
import PlacementSection from './PlacementSection'
import RemoteBackendFields from './RemoteBackendFields'

export default function AiBackendEditorDialog(props: {
  open: boolean
  backend?: AiBackendSummary | null
  initialHosting?: AiBackendHosting
  title?: string
  description?: string
  submitLabel?: string
  onOpenChange: (open: boolean) => void
  onSubmit: (input: WriteAiBackendInput, placementPolicy?: PlacementPolicyInput) => Promise<void>
}) {
  const [form, setForm] = useState<FormState>(() =>
    props.backend ? formFromBackend(props.backend) : emptyForm(props.initialHosting ?? 'remote'),
  )
  const isEditing = !!props.backend?.backend_id
  const forcedHosting = !isEditing ? props.initialHosting : undefined
  const nodesQuery = useQuery({
    queryKey: ['nodes-for-placement'],
    queryFn: () => listNodes({ sort_by: 'last_heartbeat', order: 'desc', limit: 200 }),
    enabled: props.open && !isEditing,
    staleTime: 15_000,
  })
  const nodes = nodesQuery.data?.nodes || []

  useEffect(() => {
    if (!props.open) return
    setForm(props.backend ? formFromBackend(props.backend) : emptyForm(props.initialHosting ?? 'remote'))
  }, [props.backend, props.initialHosting, props.open])

  useEffect(() => {
    if (!props.open || isEditing) return
    if (form.placement_scope !== 'single_node') return
    if (form.placement_node_uuid.trim()) return
    if (nodes.length === 1) {
      setForm((current) => ({ ...current, placement_node_uuid: nodes[0].uuid }))
    }
  }, [form.placement_node_uuid, form.placement_scope, isEditing, nodes, props.open])

  const validationErrors = useMemo(
    () => validateForm(form, { isEditing, nodes }),
    [form, isEditing, nodes],
  )
  const canSubmit = validationErrors.length === 0
  const disableReason = validationErrors[0] || ''
  const selectedOperations = useMemo(() => splitCsv(form.operations), [form.operations])
  const selectedFeatures = useMemo(() => splitCsv(form.features), [form.features])
  const selectedPlacementNodes = useMemo(
    () => splitCsv(form.placement_node_uuids),
    [form.placement_node_uuids],
  )
  const providerOptions = useMemo(
    () => providerOptionsForHosting(form.hosting, form.provider),
    [form.hosting, form.provider],
  )
  const backendKindOptions = useMemo(
    () => backendKindOptionsFor(form.hosting, form.provider, form.backend_kind),
    [form.backend_kind, form.hosting, form.provider],
  )

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent>
        <DialogHeader
          title={props.title ?? (isEditing ? 'Edit AI backend' : 'Create AI backend')}
          description={
            props.description ?? 'One row maps to one canonical backend_id in the new control plane.'
          }
        />

        <div className="min-h-0 flex-1 overflow-y-auto pr-1">
          <div className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <div className="text-sm font-medium">Display name</div>
              <Input
                value={form.display_name}
                onChange={(event) => setForm((current) => ({ ...current, display_name: event.target.value }))}
                placeholder="e.g. OpenAI Production"
              />
            </div>
            {forcedHosting ? (
              <div className="space-y-1">
                <div className="text-sm font-medium">Hosting</div>
                <div className="flex h-9 items-center rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--muted))] px-3 text-sm">
                  {forcedHosting}
                </div>
              </div>
            ) : (
              <div className="space-y-1">
                <div className="text-sm font-medium">Hosting</div>
                <select
                  className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
                  value={form.hosting}
                  onChange={(event) =>
                    setForm((current) => {
                      const hosting = event.target.value as AiBackendHosting
                      const nextOptions = providerOptionsForHosting(hosting)
                      const nextProvider = nextOptions.includes(current.provider)
                        ? current.provider
                        : defaultProviderForHosting(hosting)
                      const nextKindOptions = backendKindOptionsFor(hosting, nextProvider)
                      const nextKind = nextKindOptions.includes(current.backend_kind)
                        ? current.backend_kind
                        : defaultBackendKindFor(hosting, nextProvider)
                      const nextDefaults = defaultsForKind(nextKind)
                      return {
                        ...current,
                        hosting,
                        provider: nextProvider,
                        model: current.model.trim() ? current.model : nextDefaults.model,
                        backend_kind: nextKind,
                        operations: nextDefaults.operations.join(', '),
                        transports: nextDefaults.transports,
                        features: nextDefaults.features,
                        base_url: current.base_url.trim() ? current.base_url : nextDefaults.base_url,
                        placement_scope: defaultPlacementScopeForHosting(hosting),
                        placement_node_uuid:
                          hosting === 'local' && nodes.length === 1 ? nodes[0].uuid : '',
                        placement_node_uuids: '',
                        model_url: '',
                        metadata: setMetadataStringField(current.metadata, 'model_url', ''),
                      }
                    })
                  }
                >
                  <option value="remote">remote</option>
                  <option value="local">local</option>
                </select>
              </div>
            )}
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <div className="text-sm font-medium">Provider</div>
              <select
                className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
                value={form.provider}
                onChange={(event) =>
                  setForm((current) => {
                    const provider = event.target.value
                    const backendKind = defaultBackendKindFor(current.hosting, provider)
                    const nextDefaults = defaultsForKind(backendKind)
                    return {
                      ...current,
                      provider,
                      model: current.model.trim() ? current.model : nextDefaults.model,
                      backend_kind: backendKind,
                      operations: nextDefaults.operations.join(', '),
                      transports: nextDefaults.transports,
                      features: nextDefaults.features,
                      base_url: current.base_url.trim() ? current.base_url : nextDefaults.base_url,
                    }
                  })
                }
              >
                {providerOptions.map((provider) => (
                  <option key={provider} value={provider}>
                    {provider}
                  </option>
                ))}
              </select>
            </div>
            <div className="space-y-1">
              <div className="text-sm font-medium">Model</div>
              <Input
                value={form.model}
                onChange={(event) => setForm((current) => ({ ...current, model: event.target.value }))}
                placeholder="e.g. gpt-4.1"
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <div className="text-sm font-medium">Backend kind</div>
              <select
                className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
                value={form.backend_kind}
                onChange={(event) =>
                  setForm((current) => {
                    const backendKind = event.target.value
                    const nextDefaults = defaultsForKind(backendKind)
                    return {
                      ...current,
                      model: current.model.trim() ? current.model : nextDefaults.model,
                      backend_kind: backendKind,
                      operations: nextDefaults.operations.join(', '),
                      transports: nextDefaults.transports,
                      features: nextDefaults.features,
                      base_url: current.base_url.trim() ? current.base_url : nextDefaults.base_url,
                    }
                  })
                }
              >
                {backendKindOptions.map((backendKind) => (
                  <option key={backendKind} value={backendKind}>
                    {backendKind}
                  </option>
                ))}
              </select>
            </div>
            <div className="space-y-1">
              <div className="text-sm font-medium">Desired state</div>
              <select
                className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
                value={form.desired_state}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    desired_state: event.target.value as AiBackendDesiredState,
                  }))
                }
              >
                <option value="enabled">enabled</option>
                <option value="disabled">disabled</option>
              </select>
            </div>
          </div>

          {form.hosting === 'remote' ? (
            <RemoteBackendFields form={form} setForm={setForm} />
          ) : null}

          {form.hosting === 'local' && form.provider === 'llamacpp' ? (
            <LocalLlamaCppFields form={form} setForm={setForm} />
          ) : null}

          <div className="space-y-1">
            <div className="text-sm font-medium">Operations</div>
            <div className="grid grid-cols-1 gap-2 rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] px-3 py-2 md:grid-cols-2">
              {OPERATION_OPTIONS.map((operation) => (
                <label key={operation} className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={selectedOperations.includes(operation)}
                    onChange={(event) =>
                      setForm((current) => ({
                        ...current,
                        operations: toggleCsvValue(
                          current.operations,
                          operation,
                          event.target.checked,
                        ),
                      }))
                    }
                  />
                  <span>{operation}</span>
                </label>
              ))}
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <div className="text-sm font-medium">Features</div>
              <div className="grid grid-cols-1 gap-2 rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] px-3 py-2">
                {FEATURE_OPTIONS.map((feature) => (
                  <label key={feature} className="flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={selectedFeatures.includes(feature)}
                      onChange={(event) =>
                        setForm((current) => ({
                          ...current,
                          features: toggleCsvValue(
                            current.features,
                            feature,
                            event.target.checked,
                          ),
                        }))
                      }
                    />
                    <span>{feature}</span>
                  </label>
                ))}
              </div>
            </div>
            <div className="space-y-1">
              <div className="text-sm font-medium">Transports</div>
              <select
                className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
                value={form.transports}
                onChange={(event) =>
                  setForm((current) => ({ ...current, transports: event.target.value }))
                }
              >
                {TRANSPORT_OPTIONS.map((transport) => (
                  <option key={transport} value={transport}>
                    {transport}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <div className="text-sm font-medium">Weight</div>
              <Input
                value={form.weight}
                onChange={(event) => setForm((current) => ({ ...current, weight: event.target.value }))}
                placeholder="100"
              />
            </div>
            <div className="space-y-1">
              <div className="text-sm font-medium">Priority</div>
              <Input
                value={form.priority}
                onChange={(event) => setForm((current) => ({ ...current, priority: event.target.value }))}
                placeholder="0"
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <div className="text-sm font-medium">Labels</div>
              <textarea
                className="min-h-[88px] w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 py-2 text-sm"
                value={form.labels}
                onChange={(event) =>
                  setForm((current) => ({ ...current, labels: event.target.value }))
                }
                placeholder={'env=prod\nteam=ml-platform'}
              />
              <div className="text-xs text-[hsl(var(--muted-foreground))]">
                One `key=value` pair per line.
              </div>
            </div>
            <div className="space-y-1">
              <div className="text-sm font-medium">Metadata</div>
              <textarea
                className="min-h-[88px] w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 py-2 font-mono text-xs"
                value={form.metadata}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    metadata: event.target.value,
                    model_url:
                      current.hosting === 'local' && current.provider === 'llamacpp'
                        ? metadataStringField(event.target.value, 'model_url')
                        : current.model_url,
                    model_path:
                      current.hosting === 'local' && current.provider === 'llamacpp'
                        ? metadataStringField(event.target.value, 'model_path')
                        : current.model_path,
                    skip_download:
                      current.hosting === 'local' && current.provider === 'llamacpp'
                        ? metadataBooleanField(event.target.value, 'skip_download')
                        : current.skip_download,
                    download_timeout_s:
                      current.hosting === 'local' && current.provider === 'llamacpp'
                        ? metadataStringField(event.target.value, 'download_timeout_s')
                        : current.download_timeout_s,
                    threads:
                      current.hosting === 'local' && current.provider === 'llamacpp'
                        ? metadataStringField(event.target.value, 'threads')
                        : current.threads,
                    ctx_size:
                      current.hosting === 'local' && current.provider === 'llamacpp'
                        ? metadataStringField(event.target.value, 'ctx_size')
                        : current.ctx_size,
                  }))
                }
                placeholder={'{\n  "region": "us-east-1"\n}'}
              />
              <div className="text-xs text-[hsl(var(--muted-foreground))]">
                JSON object stored on the backend record.
              </div>
            </div>
          </div>

          {form.hosting === 'local' && form.backend_kind.toLowerCase().includes('vllm') ? (
            <div className="rounded-[var(--radius)] border border-[hsl(var(--border))] bg-[hsl(var(--muted))] px-3 py-2 text-xs text-[hsl(var(--muted-foreground))]">
              vLLM remains a scaffolded path in the current control plane. Keep it only for planned
              placeholders or external endpoint wiring.
            </div>
          ) : null}

          {!isEditing ? (
            <PlacementSection
              form={form}
              setForm={setForm}
              nodes={nodes}
              selectedPlacementNodes={selectedPlacementNodes}
              isLoadingNodes={nodesQuery.isLoading}
            />
          ) : null}

          {validationErrors.length > 0 ? (
            <div className="rounded-[var(--radius)] border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))/0.08] px-3 py-2 text-xs text-[hsl(var(--destructive))]">
              {validationErrors.join(' ')}
            </div>
          ) : null}

        </div>
        </div>
        <div className="mt-4 flex shrink-0 items-center justify-end gap-2 border-t border-[hsl(var(--border))] pt-3">
          <Button variant="secondary" onClick={() => props.onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            disabled={!canSubmit}
            title={canSubmit ? '' : disableReason}
            onClick={async () => {
              try {
                await props.onSubmit(
                  buildPayload(form),
                  !isEditing ? buildPlacementPolicy(form, nodes) : undefined,
                )
                props.onOpenChange(false)
              } catch (error) {
                toast.error((error as Error).message)
              }
            }}
          >
            {props.submitLabel ?? 'Save'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

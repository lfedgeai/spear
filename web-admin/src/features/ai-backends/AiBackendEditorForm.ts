import type {
  AiBackendSummary,
  AiBackendDesiredState,
  AiBackendHosting,
  WriteAiBackendInput,
} from '@/api/ai-backends'
import type { NodeSummary } from '@/api/types'

export type PlacementScope = 'all_nodes' | 'single_node' | 'selected_nodes'

export type PlacementPolicyInput = {
  scope: PlacementScope
  desired_state: AiBackendDesiredState
  node_uuids: string[]
  weight_override?: number
  priority_override?: number
}

export type AiBackendEditorFormState = {
  display_name: string
  provider: string
  model: string
  hosting: AiBackendHosting
  backend_kind: string
  credential_ref: string
  desired_state: AiBackendDesiredState
  base_url: string
  operations: string
  features: string
  transports: string
  weight: string
  priority: string
  labels: string
  metadata: string
  model_url: string
  model_path: string
  skip_download: boolean
  download_timeout_s: string
  threads: string
  ctx_size: string
  placement_scope: PlacementScope
  placement_state: AiBackendDesiredState
  placement_node_uuid: string
  placement_node_uuids: string
  placement_weight_override: string
  placement_priority_override: string
}

export const TRANSPORT_OPTIONS: string[] = ['http', 'websocket']
export const FEATURE_OPTIONS: string[] = ['stream', 'supports_tools', 'supports_json_schema']
export const OPERATION_OPTIONS: string[] = [
  'chat_completions',
  'embeddings',
  'image_generation',
  'speech_to_text',
  'text_to_speech',
]

const REMOTE_PROVIDER_OPTIONS: string[] = ['openai', 'ollama']
const LOCAL_PROVIDER_OPTIONS: string[] = ['llamacpp', 'vllm']

const BACKEND_KIND_OPTIONS: Record<AiBackendHosting, Record<string, string[]>> = {
  remote: {
    openai: ['openai_chat_completion', 'openai_realtime_ws'],
    ollama: ['ollama_chat'],
  },
  local: {
    llamacpp: ['llamacpp'],
    vllm: ['vllm'],
  },
}

const BACKEND_KIND_DEFAULTS: Record<
  string,
  { operations: string[]; transports: string; features: string; base_url: string; model: string }
> = {
  openai_chat_completion: {
    operations: ['chat_completions'],
    transports: 'http',
    features: '',
    base_url: 'https://api.openai.com/v1',
    model: '',
  },
  openai_realtime_ws: {
    operations: ['speech_to_text'],
    transports: 'websocket',
    features: '',
    base_url: 'https://api.openai.com/v1',
    model: 'gpt-4o-mini-transcribe',
  },
  ollama_chat: {
    operations: ['chat_completions'],
    transports: 'http',
    features: '',
    base_url: 'http://127.0.0.1:11434',
    model: '',
  },
  llamacpp: {
    operations: ['chat_completions'],
    transports: 'http',
    features: '',
    base_url: '',
    model: '',
  },
  vllm: {
    operations: ['chat_completions'],
    transports: 'http',
    features: '',
    base_url: '',
    model: '',
  },
}

export function providerOptionsForHosting(hosting: AiBackendHosting, currentProvider?: string) {
  const base = hosting === 'local' ? [...LOCAL_PROVIDER_OPTIONS] : [...REMOTE_PROVIDER_OPTIONS]
  const normalizedCurrent = (currentProvider || '').trim()
  if (normalizedCurrent && !base.includes(normalizedCurrent)) {
    return [normalizedCurrent, ...base]
  }
  return base
}

export function defaultProviderForHosting(hosting: AiBackendHosting): string {
  return hosting === 'local' ? 'llamacpp' : 'openai'
}

export function backendKindOptionsFor(
  hosting: AiBackendHosting,
  provider: string,
  currentKind?: string,
) {
  const options = [...(BACKEND_KIND_OPTIONS[hosting][provider] || [])]
  const normalizedCurrent = (currentKind || '').trim()
  if (normalizedCurrent && !options.includes(normalizedCurrent)) {
    return [normalizedCurrent, ...options]
  }
  return options
}

export function defaultBackendKindFor(hosting: AiBackendHosting, provider: string) {
  return backendKindOptionsFor(hosting, provider)[0] || ''
}

export function defaultsForKind(kind: string) {
  return BACKEND_KIND_DEFAULTS[kind] || {
    operations: [],
    transports: '',
    features: '',
    base_url: '',
    model: '',
  }
}

export function splitCsv(value: string): string[] {
  return value
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean)
}

export function toggleCsvValue(csv: string, value: string, checked: boolean) {
  const current = new Set(splitCsv(csv))
  if (checked) {
    current.add(value)
  } else {
    current.delete(value)
  }
  return Array.from(current).join(', ')
}

function joinCsv(values?: string[] | null): string {
  return (values || []).join(', ')
}

export function emptyForm(hosting: AiBackendHosting = 'remote'): AiBackendEditorFormState {
  const provider = defaultProviderForHosting(hosting)
  const backendKind = defaultBackendKindFor(hosting, provider)
  const defaults = defaultsForKind(backendKind)
  return {
    display_name: '',
    provider,
    model: '',
    hosting,
    backend_kind: backendKind,
    credential_ref: '',
    desired_state: 'enabled',
    base_url: defaults.base_url,
    operations: defaults.operations.join(', '),
    features: defaults.features,
    transports: defaults.transports,
    weight: '100',
    priority: '0',
    labels: '',
    metadata: '{}',
    model_url: '',
    model_path: '',
    skip_download: false,
    download_timeout_s: '',
    threads: '',
    ctx_size: '',
    placement_scope: hosting === 'local' ? 'single_node' : 'all_nodes',
    placement_state: 'enabled',
    placement_node_uuid: '',
    placement_node_uuids: '',
    placement_weight_override: '',
    placement_priority_override: '',
  }
}

export function metadataStringField(value: string, key: string): string {
  try {
    const parsed = parseMetadata(value)
    const raw = parsed[key]
    return typeof raw === 'string' ? raw : ''
  } catch {
    return ''
  }
}

export function metadataBooleanField(value: string, key: string): boolean {
  try {
    const parsed = parseMetadata(value)
    const raw = parsed[key]
    if (typeof raw === 'boolean') return raw
    if (typeof raw === 'string') return raw.trim() === '1' || raw.trim().toLowerCase() === 'true'
    return false
  } catch {
    return false
  }
}

export function setMetadataStringField(value: string, key: string, nextValue: string): string {
  let parsed: Record<string, unknown>
  try {
    parsed = parseMetadata(value)
  } catch {
    parsed = {}
  }
  const trimmed = nextValue.trim()
  if (trimmed) {
    parsed[key] = trimmed
  } else {
    delete parsed[key]
  }
  return JSON.stringify(parsed, null, 2)
}

export function setMetadataBooleanField(value: string, key: string, nextValue: boolean): string {
  let parsed: Record<string, unknown>
  try {
    parsed = parseMetadata(value)
  } catch {
    parsed = {}
  }
  if (nextValue) {
    parsed[key] = '1'
  } else {
    delete parsed[key]
  }
  return JSON.stringify(parsed, null, 2)
}

export function formFromBackend(backend?: AiBackendSummary | null): AiBackendEditorFormState {
  if (!backend) return emptyForm()
  const metadata = JSON.stringify(backend.metadata || {}, null, 2)
  return {
    display_name: backend.display_name || '',
    provider: backend.provider || '',
    model: backend.model || '',
    hosting: backend.hosting === 'local' ? 'local' : 'remote',
    backend_kind: backend.backend_kind || backend.spec?.kind || 'openai-compatible',
    credential_ref: backend.credential_ref || backend.spec?.credential_ref || '',
    desired_state: backend.desired_state === 'disabled' ? 'disabled' : 'enabled',
    base_url: backend.spec?.base_url || '',
    operations: joinCsv(backend.spec?.operations),
    features: joinCsv(backend.spec?.features),
    transports: joinCsv(backend.spec?.transports),
    weight: String(backend.spec?.weight ?? 100),
    priority: String(backend.spec?.priority ?? 0),
    labels: Object.entries(backend.labels || {})
      .map(([key, value]) => `${key}=${value}`)
      .join('\n'),
    metadata,
    model_url: metadataStringField(metadata, 'model_url'),
    model_path: metadataStringField(metadata, 'model_path'),
    skip_download: metadataBooleanField(metadata, 'skip_download'),
    download_timeout_s: metadataStringField(metadata, 'download_timeout_s'),
    threads: metadataStringField(metadata, 'threads'),
    ctx_size: metadataStringField(metadata, 'ctx_size'),
    placement_scope: backend.hosting === 'local' ? 'single_node' : 'all_nodes',
    placement_state: 'enabled',
    placement_node_uuid: '',
    placement_node_uuids: '',
    placement_weight_override: '',
    placement_priority_override: '',
  }
}

export function defaultPlacementScopeForHosting(hosting: AiBackendHosting): PlacementScope {
  return hosting === 'local' ? 'single_node' : 'all_nodes'
}

export function toggleNodeValue(csv: string, nodeUuid: string, checked: boolean) {
  return toggleCsvValue(csv, nodeUuid, checked)
}

export function formatNodeLabel(node: NodeSummary) {
  if (node.name?.trim()) {
    return `${node.name.trim()} (${node.uuid})`
  }
  return node.uuid
}

export function parseLabels(value: string): Record<string, string> {
  const labels: Record<string, string> = {}
  for (const line of value.split('\n')) {
    const trimmed = line.trim()
    if (!trimmed) continue
    const sep = trimmed.indexOf('=')
    if (sep <= 0) {
      throw new Error(`Invalid label line: ${trimmed}`)
    }
    const key = trimmed.slice(0, sep).trim()
    const labelValue = trimmed.slice(sep + 1).trim()
    if (!key) {
      throw new Error(`Invalid label line: ${trimmed}`)
    }
    labels[key] = labelValue
  }
  return labels
}

export function parseMetadata(value: string): Record<string, unknown> {
  const trimmed = value.trim()
  if (!trimmed) return {}
  const parsed = JSON.parse(trimmed) as unknown
  if (!parsed || Array.isArray(parsed) || typeof parsed !== 'object') {
    throw new Error('Metadata must be a JSON object')
  }
  return parsed as Record<string, unknown>
}

export function buildPayload(form: AiBackendEditorFormState): WriteAiBackendInput {
  const metadata = parseMetadata(form.metadata)
  const modelUrl = form.model_url.trim()
  if (modelUrl) {
    metadata.model_url = modelUrl
  } else {
    delete metadata.model_url
  }
  const modelPath = form.model_path.trim()
  if (modelPath) {
    metadata.model_path = modelPath
  } else {
    delete metadata.model_path
  }
  if (form.skip_download) {
    metadata.skip_download = '1'
  } else {
    delete metadata.skip_download
  }
  const downloadTimeout = form.download_timeout_s.trim()
  if (downloadTimeout) {
    metadata.download_timeout_s = downloadTimeout
  } else {
    delete metadata.download_timeout_s
  }
  const threads = form.threads.trim()
  if (threads) {
    metadata.threads = threads
  } else {
    delete metadata.threads
  }
  const ctxSize = form.ctx_size.trim()
  if (ctxSize) {
    metadata.ctx_size = ctxSize
  } else {
    delete metadata.ctx_size
  }
  return {
    display_name: form.display_name.trim(),
    provider: form.provider.trim(),
    model: form.model.trim(),
    hosting: form.hosting,
    backend_kind: form.backend_kind.trim(),
    credential_ref: form.credential_ref.trim() || undefined,
    desired_state: form.desired_state,
    spec: {
      base_url: form.base_url.trim() || undefined,
      operations: splitCsv(form.operations),
      features: splitCsv(form.features),
      transports: splitCsv(form.transports),
      weight: Number(form.weight || '100'),
      priority: Number(form.priority || '0'),
    },
    labels: parseLabels(form.labels),
    metadata,
  }
}

function isValidHttpUrl(value: string): boolean {
  try {
    const url = new URL(value)
    return url.protocol === 'http:' || url.protocol === 'https:'
  } catch {
    return false
  }
}

export function validateForm(
  form: AiBackendEditorFormState,
  options?: { isEditing?: boolean; nodes?: NodeSummary[] },
): string[] {
  const errors: string[] = []
  const isEditing = !!options?.isEditing
  const nodes = options?.nodes || []
  if (!form.display_name.trim()) errors.push('Display name is required')
  if (!form.provider.trim()) errors.push('Provider is required')
  if (!form.model.trim()) errors.push('Model is required')
  if (!form.backend_kind.trim()) errors.push('Backend kind is required')
  if (
    form.hosting === 'local' &&
    form.provider === 'llamacpp' &&
    !form.model_url.trim() &&
    !form.model_path.trim()
  ) {
    errors.push('Model URL or Model Path is required for local llamacpp backends')
  }
  if (splitCsv(form.operations).length === 0) errors.push('At least one operation is required')
  if (splitCsv(form.transports).length === 0) errors.push('At least one transport is required')

  const baseUrl = form.base_url.trim()
  if (form.hosting === 'remote' && !baseUrl) {
    errors.push('Base URL is required for remote backends')
  } else if (baseUrl && !isValidHttpUrl(baseUrl)) {
    errors.push('Base URL must be a valid http(s) URL')
  }
  if (form.model_url.trim() && !isValidHttpUrl(form.model_url.trim())) {
    errors.push('Model URL must be a valid http(s) URL')
  }
  const integerFields: Array<{ label: string; value: string; nonNegative?: boolean; positive?: boolean }> = [
    { label: 'Download timeout', value: form.download_timeout_s, positive: true },
    { label: 'Threads', value: form.threads, positive: true },
    { label: 'Context size', value: form.ctx_size, positive: true },
  ]
  for (const field of integerFields) {
    const trimmed = field.value.trim()
    if (!trimmed) continue
    const parsed = Number(trimmed)
    if (!Number.isFinite(parsed) || !Number.isInteger(parsed)) {
      errors.push(`${field.label} must be an integer`)
      continue
    }
    if (field.positive && parsed <= 0) {
      errors.push(`${field.label} must be greater than 0`)
    }
  }

  const weight = Number(form.weight)
  if (!Number.isFinite(weight) || !Number.isInteger(weight) || weight < 0) {
    errors.push('Weight must be a non-negative integer')
  }

  const priority = Number(form.priority)
  if (!Number.isFinite(priority) || !Number.isInteger(priority)) {
    errors.push('Priority must be an integer')
  }

  try {
    parseLabels(form.labels)
  } catch (error) {
    errors.push((error as Error).message)
  }

  try {
    parseMetadata(form.metadata)
  } catch (error) {
    errors.push((error as Error).message)
  }

  if (!isEditing) {
    if (nodes.length === 0) {
      errors.push('At least one node is required to create placement')
    } else if (form.placement_scope === 'single_node' && !form.placement_node_uuid.trim()) {
      errors.push('Single node placement requires a selected node')
    } else if (
      form.placement_scope === 'selected_nodes' &&
      splitCsv(form.placement_node_uuids).length === 0
    ) {
      errors.push('Selected nodes placement requires at least one node')
    }

    const placementWeight = form.placement_weight_override.trim()
    if (placementWeight) {
      const weightOverride = Number(placementWeight)
      if (!Number.isFinite(weightOverride) || !Number.isInteger(weightOverride) || weightOverride < 0) {
        errors.push('Placement weight override must be a non-negative integer')
      }
    }

    const placementPriority = form.placement_priority_override.trim()
    if (placementPriority) {
      const priorityOverride = Number(placementPriority)
      if (!Number.isFinite(priorityOverride) || !Number.isInteger(priorityOverride)) {
        errors.push('Placement priority override must be an integer')
      }
    }
  }

  return errors
}

export function buildPlacementPolicy(
  form: AiBackendEditorFormState,
  nodes: NodeSummary[],
): PlacementPolicyInput {
  const nodeUuids =
    form.placement_scope === 'all_nodes'
      ? nodes.map((node) => node.uuid)
      : form.placement_scope === 'single_node'
        ? [form.placement_node_uuid.trim()].filter(Boolean)
        : splitCsv(form.placement_node_uuids)

  const placement: PlacementPolicyInput = {
    scope: form.placement_scope,
    desired_state: form.placement_state,
    node_uuids: nodeUuids,
  }
  if (form.placement_weight_override.trim()) {
    placement.weight_override = Number(form.placement_weight_override)
  }
  if (form.placement_priority_override.trim()) {
    placement.priority_override = Number(form.placement_priority_override)
  }
  return placement
}

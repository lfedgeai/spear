import { describe, expect, it } from 'vitest'

import type { AiBackendSummary } from '@/api/ai-backends'
import {
  buildPayload,
  emptyForm,
  formFromBackend,
  validateForm,
} from '@/features/ai-backends/AiBackendEditorForm'

describe('buildPayload', () => {
  it('serializes local llamacpp inputs into structured local config and removes duplicate metadata keys', () => {
    const form = emptyForm('local')
    form.display_name = 'Local llama.cpp'
    form.provider = 'llamacpp'
    form.model = 'llama3.1'
    form.backend_kind = 'llamacpp'
    form.base_url = 'http://127.0.0.1:8080/v1'
    form.operations = 'chat_completions'
    form.transports = 'http'
    form.model_url = 'https://models.example.com/llama3.1.gguf'
    form.threads = '8'
    form.ctx_size = '4096'
    form.metadata = JSON.stringify(
      {
        model_url: 'stale-url',
        threads: '4',
        ctx_size: '2048',
        region: 'lab-a',
      },
      null,
      2,
    )

    const payload = buildPayload(form)

    expect(payload.local).toEqual({
      provider_family: 'llama_cpp',
      config: {
        model_url: 'https://models.example.com/llama3.1.gguf',
        model_path: undefined,
        skip_download: undefined,
        download_timeout_s: undefined,
        threads: 8,
        ctx_size: 4096,
      },
    })
    expect(payload.metadata).toEqual({
      region: 'lab-a',
    })
    expect(payload.spec).toEqual(
      expect.objectContaining({
        base_url: 'http://127.0.0.1:8080/v1',
        operations: ['chat_completions'],
        transports: ['http'],
      }),
    )
  })

  it('serializes remote openai inputs into structured remote config and clears duplicate legacy spec fields', () => {
    const form = emptyForm('remote')
    form.display_name = 'Remote OpenAI'
    form.provider = 'openai'
    form.model = 'gpt-4o-mini'
    form.backend_kind = 'openai_chat_completion'
    form.credential_ref = 'openai-prod'
    form.base_url = 'https://api.openai.com/v1'
    form.operations = 'chat_completions, embeddings'
    form.features = 'stream'
    form.transports = 'http'
    form.weight = '120'
    form.priority = '5'
    form.metadata = JSON.stringify(
      {
        region: 'us-east-1',
      },
      null,
      2,
    )

    const payload = buildPayload(form)

    expect(payload.credential_ref).toBeUndefined()
    expect(payload.remote).toEqual({
      provider_family: 'open_ai_compatible',
      config: {
        base_url: 'https://api.openai.com/v1',
        credential_ref: 'openai-prod',
        operations: ['chat_completions', 'embeddings'],
        features: ['stream'],
        transports: ['http'],
      },
    })
    expect(payload.spec).toEqual({
      base_url: undefined,
      operations: [],
      features: [],
      transports: [],
      weight: 120,
      priority: 5,
    })
    expect(payload.metadata).toEqual({
      region: 'us-east-1',
    })
  })

  it('rejects unsupported providers instead of preserving legacy transition payloads', () => {
    const form = emptyForm('remote')
    form.display_name = 'Custom Remote'
    form.provider = 'anthropic-compatible'
    form.model = 'claude-compatible'
    form.backend_kind = 'custom_http'
    form.credential_ref = 'custom-secret'
    form.base_url = 'https://custom.example.com/v1'
    form.operations = 'chat_completions'
    form.features = 'supports_tools'
    form.transports = 'http'
    form.metadata = JSON.stringify(
      {
        tenant: 'lab',
      },
      null,
      2,
    )

    expect(validateForm(form)).toContain(
      'Provider anthropic-compatible is not supported for remote backends',
    )
    expect(() => buildPayload(form)).toThrow(
      'Unsupported provider for remote backends: anthropic-compatible',
    )
  })

  it('rehydrates local llamacpp form fields from typed local config before legacy metadata fallback', () => {
    const backend: AiBackendSummary = {
      backend_id: 'backend-local-1',
      display_name: 'Local llama.cpp',
      provider: 'llamacpp',
      model: 'llama3.1',
      hosting: 'local',
      backend_kind: 'llamacpp',
      desired_state: 'enabled',
      management_mode: 'sms_local',
      credential_ref: null,
      spec: {
        name: 'backend-local-1',
        kind: 'llamacpp',
        operations: ['chat_completions'],
        features: [],
        transports: ['http'],
        weight: 100,
        priority: 0,
        base_url: 'http://127.0.0.1:8080/v1',
        provider: 'llamacpp',
        model: 'llama3.1',
        credential_ref: null,
        origin: 0,
        deployment_id: '',
      },
      local: {
        provider_family: 'llama_cpp',
        config: {
          model_url: 'https://models.example.com/llama3.1.gguf',
          model_path: '/models/llama3.1.gguf',
          skip_download: false,
          download_timeout_s: 90,
          threads: 8,
          ctx_size: 4096,
        },
      },
      metadata: {
        model_url: 'stale-url',
        model_path: '/legacy/path.gguf',
        skip_download: true,
        download_timeout_s: '15',
        threads: '2',
        ctx_size: '1024',
      },
      labels: {},
      generation: 1,
      created_at_ms: 0,
      updated_at_ms: 0,
    }

    const form = formFromBackend(backend)

    expect(form.model_url).toBe('https://models.example.com/llama3.1.gguf')
    expect(form.model_path).toBe('/models/llama3.1.gguf')
    expect(form.skip_download).toBe(false)
    expect(form.download_timeout_s).toBe('90')
    expect(form.threads).toBe('8')
    expect(form.ctx_size).toBe('4096')
  })

  it('rehydrates remote openai form fields from typed remote config before legacy spec fallback', () => {
    const backend: AiBackendSummary = {
      backend_id: 'backend-remote-1',
      display_name: 'Remote OpenAI',
      provider: 'openai',
      model: 'gpt-4o-mini',
      hosting: 'remote',
      backend_kind: 'openai_chat_completion',
      desired_state: 'enabled',
      management_mode: 'sms_remote',
      credential_ref: null,
      spec: {
        name: 'backend-remote-1',
        kind: 'openai_chat_completion',
        operations: ['legacy_operation'],
        features: ['legacy_feature'],
        transports: ['legacy_transport'],
        weight: 120,
        priority: 5,
        base_url: 'https://legacy.example.com/v1',
        provider: 'openai',
        model: 'gpt-4o-mini',
        credential_ref: 'legacy-credential',
        origin: 0,
        deployment_id: '',
      },
      remote: {
        provider_family: 'open_ai_compatible',
        config: {
          base_url: 'https://api.openai.com/v1',
          credential_ref: 'openai-prod',
          operations: ['chat_completions', 'embeddings'],
          features: ['stream'],
          transports: ['http'],
        },
      },
      metadata: {},
      labels: {},
      generation: 1,
      created_at_ms: 0,
      updated_at_ms: 0,
    }

    const form = formFromBackend(backend)

    expect(form.credential_ref).toBe('openai-prod')
    expect(form.base_url).toBe('https://api.openai.com/v1')
    expect(form.operations).toBe('chat_completions, embeddings')
    expect(form.features).toBe('stream')
    expect(form.transports).toBe('http')
    expect(form.weight).toBe('120')
    expect(form.priority).toBe('5')
  })
})

import { describe, expect, it } from 'vitest'

import type { AiBackendSummary } from '@/api/ai-backends'
import {
  resolveBackendDetailSummary,
  resolveBackendProviderFields,
} from '@/features/ai-backends/AiBackendDetailPage'

describe('AiBackendDetailPage helpers', () => {
  it('prefers typed remote config over legacy spec fields for detail summary', () => {
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
        weight: 100,
        priority: 0,
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

    expect(resolveBackendDetailSummary(backend)).toEqual({
      operations: ['chat_completions', 'embeddings'],
      features: ['stream'],
      transports: ['http'],
      baseUrl: 'https://api.openai.com/v1',
      credentialRef: 'openai-prod',
    })
  })

  it('exposes typed local provider fields for known local providers', () => {
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
      metadata: {},
      labels: {},
      generation: 1,
      created_at_ms: 0,
      updated_at_ms: 0,
    }

    expect(resolveBackendProviderFields(backend)).toEqual([
      { label: 'Model URL', value: 'https://models.example.com/llama3.1.gguf' },
      { label: 'Model Path', value: '/models/llama3.1.gguf' },
      { label: 'Skip download', value: 'false' },
      { label: 'Download timeout', value: '90' },
      { label: 'Threads', value: '8' },
      { label: 'Context size', value: '4096' },
    ])
  })
})

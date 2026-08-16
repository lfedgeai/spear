import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import AiBackendEditorDialog from '@/features/ai-backends/AiBackendEditorDialog'
import type { AiBackendHosting } from '@/api/ai-backends'

const mockListCredentials = vi.fn()
const mockListNodes = vi.fn()

vi.mock('@/api/credentials', () => ({
  listCredentials: () => mockListCredentials(),
}))

vi.mock('@/api/nodes', () => ({
  listNodes: () => mockListNodes(),
}))

function renderDialog(options?: { initialHosting?: AiBackendHosting }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  })
  const onSubmit = vi.fn().mockResolvedValue(undefined)
  const onOpenChange = vi.fn()

  const rendered = render(
    <QueryClientProvider client={client}>
      <AiBackendEditorDialog
        open
        initialHosting={options?.initialHosting}
        onOpenChange={onOpenChange}
        onSubmit={onSubmit}
      />
    </QueryClientProvider>,
  )

  return { ...rendered, onSubmit, onOpenChange }
}

function getSelectForLabel(label: string): HTMLSelectElement {
  const title = screen.getByText(label)
  const wrapper = title.parentElement
  if (!wrapper) {
    throw new Error(`Missing wrapper for label ${label}`)
  }
  const select = wrapper.querySelector('select')
  if (!select) {
    throw new Error(`Missing select for label ${label}`)
  }
  return select as HTMLSelectElement
}

describe('AiBackendEditorDialog', () => {
  beforeEach(() => {
    mockListCredentials.mockReset()
    mockListNodes.mockReset()
    mockListCredentials.mockResolvedValue({
      success: true,
      credentials: [
        {
          name: 'openai-prod',
          provider_kind: 'openai',
          disabled: false,
          referenced_by_count: 0,
          created_at_ms: 0,
          updated_at_ms: 0,
          labels: {},
          metadata: {},
        },
      ],
    })
    mockListNodes.mockResolvedValue({
      nodes: [
        {
          uuid: 'node-1',
          name: 'spearlet-local',
          ip_address: '127.0.0.1',
          port: 50052,
          status: 'ready',
          last_heartbeat: 0,
          registered_at: 0,
          metadata: {},
        },
      ],
      total_count: 1,
    })
  })

  afterEach(() => {
    vi.clearAllMocks()
    cleanup()
  })

  it('submits labels, metadata, and credential ref through the editor', async () => {
    const { onSubmit } = renderDialog()

    fireEvent.change(screen.getByPlaceholderText('e.g. OpenAI Production'), {
      target: { value: 'Remote OpenAI' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. gpt-4.1'), {
      target: { value: 'gpt-4o' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. https://api.openai.com/v1'), {
      target: { value: 'https://api.openai.com/v1' },
    })

    await waitFor(() => expect(screen.getByRole('option', { name: 'openai-prod' })).toBeTruthy())
    await waitFor(() => expect(screen.queryByText('Loading nodes…')).toBeNull())

    const textareas = document.body.querySelectorAll('textarea')
    fireEvent.change(textareas[0], {
      target: { value: 'env=prod\nteam=ml-platform' },
    })
    fireEvent.change(textareas[1], {
      target: { value: '{\n  "region": "us-east-1"\n}' },
    })

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith(
        expect.objectContaining({
          provider: 'openai',
          model: 'gpt-4o',
          labels: {
            env: 'prod',
            team: 'ml-platform',
          },
          metadata: {
            region: 'us-east-1',
          },
        }),
        expect.objectContaining({
          scope: 'all_nodes',
          node_uuids: ['node-1'],
        }),
      ),
    )
  })

  it('blocks submission when metadata is not a JSON object', async () => {
    const { onSubmit } = renderDialog()

    fireEvent.change(screen.getByPlaceholderText('e.g. OpenAI Production'), {
      target: { value: 'Remote OpenAI' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. gpt-4.1'), {
      target: { value: 'gpt-4o' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. https://api.openai.com/v1'), {
      target: { value: 'https://api.openai.com/v1' },
    })

    const textareas = document.body.querySelectorAll('textarea')
    fireEvent.change(textareas[1], {
      target: { value: '[]' },
    })

    await waitFor(() => expect(screen.getByText('Metadata must be a JSON object')).toBeTruthy())

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(onSubmit).not.toHaveBeenCalled())
  })

  it('switches provider and backend kind defaults when hosting changes to local', async () => {
    renderDialog()

    const selects = screen.getAllByRole('combobox')
    fireEvent.change(selects[0], {
      target: { value: 'local' },
    })

    await waitFor(() => {
      const updatedSelects = screen.getAllByRole('combobox')
      expect((updatedSelects[1] as HTMLSelectElement).value).toBe('llamacpp')
      expect((updatedSelects[2] as HTMLSelectElement).value).toBe('llamacpp')
    })

    expect((screen.getByLabelText('chat_completions') as HTMLInputElement).checked).toBe(true)
    expect((screen.getByDisplayValue('http') as HTMLInputElement).value).toBe('http')
    await waitFor(() => expect(screen.getByRole('option', { name: /spearlet-local/ })).toBeTruthy())
    expect(getSelectForLabel('Deployment scope').value).toBe('single_node')
  })

  it('prefills the default OpenAI base url for chat and realtime backends', async () => {
    renderDialog()

    expect(
      (screen.getByPlaceholderText('e.g. https://api.openai.com/v1') as HTMLInputElement).value,
    ).toBe('https://api.openai.com/v1')

    const selects = screen.getAllByRole('combobox')
    fireEvent.change(selects[2], {
      target: { value: 'openai_realtime_ws' },
    })

    await waitFor(() => {
      expect((screen.getByLabelText('speech_to_text') as HTMLInputElement).checked).toBe(true)
    })
    expect((screen.getByDisplayValue('websocket') as HTMLInputElement).value).toBe('websocket')
    expect(
      (screen.getByPlaceholderText('e.g. https://api.openai.com/v1') as HTMLInputElement).value,
    ).toBe('https://api.openai.com/v1')
    expect((screen.getByPlaceholderText('e.g. gpt-4.1') as HTMLInputElement).value).toBe(
      'gpt-4o-mini-transcribe',
    )
  })

  it('supports multi-select operations via checkboxes', async () => {
    const { onSubmit } = renderDialog()

    fireEvent.change(screen.getByPlaceholderText('e.g. OpenAI Production'), {
      target: { value: 'OpenAI Chat + Embeddings' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. gpt-4.1'), {
      target: { value: 'gpt-4o' },
    })
    await waitFor(() => expect(screen.queryByText('Loading nodes…')).toBeNull())

    fireEvent.click(screen.getByLabelText('embeddings'))
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        spec: expect.objectContaining({
          operations: ['chat_completions', 'embeddings'],
        }),
      }),
      expect.anything(),
    )
  })

  it('supports multi-select features via checkboxes', async () => {
    const { onSubmit } = renderDialog()

    fireEvent.change(screen.getByPlaceholderText('e.g. OpenAI Production'), {
      target: { value: 'OpenAI Tooling Backend' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. gpt-4.1'), {
      target: { value: 'gpt-4o' },
    })
    await waitFor(() => expect(screen.queryByText('Loading nodes…')).toBeNull())

    fireEvent.click(screen.getByLabelText('stream'))
    fireEvent.click(screen.getByLabelText('supports_tools'))
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        spec: expect.objectContaining({
          features: ['stream', 'supports_tools'],
        }),
      }),
      expect.anything(),
    )
  })

  it('submits single-node placement for local backends by default', async () => {
    const { onSubmit } = renderDialog({ initialHosting: 'local' })

    fireEvent.change(screen.getByPlaceholderText('e.g. OpenAI Production'), {
      target: { value: 'Local llama.cpp' },
    })

    await waitFor(() => expect(screen.getByRole('option', { name: /spearlet-local/ })).toBeTruthy())
    expect(screen.getByText('Hosting')).toBeTruthy()
    expect(screen.getByText('local')).toBeTruthy()

    fireEvent.change(screen.getByPlaceholderText('e.g. gpt-4.1'), {
      target: { value: 'llama3.1' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. https://host/path/model.gguf'), {
      target: { value: 'https://models.example.com/llama3.1.gguf' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        hosting: 'local',
        metadata: expect.objectContaining({
          model_url: 'https://models.example.com/llama3.1.gguf',
        }),
      }),
      expect.objectContaining({
        scope: 'single_node',
        node_uuids: ['node-1'],
      }),
    )
  })

  it('shows model url field for local llamacpp and syncs it into metadata', async () => {
    const { onSubmit } = renderDialog({ initialHosting: 'local' })

    await waitFor(() => expect(screen.getByRole('option', { name: /spearlet-local/ })).toBeTruthy())
    fireEvent.change(screen.getByPlaceholderText('e.g. OpenAI Production'), {
      target: { value: 'Local llama.cpp' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. gpt-4.1'), {
      target: { value: 'llama3.1' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. https://host/path/model.gguf'), {
      target: { value: 'https://models.example.com/llama3.1.gguf' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        metadata: expect.objectContaining({
          model_url: 'https://models.example.com/llama3.1.gguf',
        }),
      }),
      expect.anything(),
    )
  })

  it('accepts model path for local llamacpp without requiring model url', async () => {
    const { onSubmit } = renderDialog({ initialHosting: 'local' })

    await waitFor(() => expect(screen.getByRole('option', { name: /spearlet-local/ })).toBeTruthy())
    fireEvent.change(screen.getByPlaceholderText('e.g. OpenAI Production'), {
      target: { value: 'Local llama.cpp Path' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. gpt-4.1'), {
      target: { value: 'llama3.1' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. /models/llama/model.gguf'), {
      target: { value: '/models/llama/llama3.1.gguf' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        metadata: expect.objectContaining({
          model_path: '/models/llama/llama3.1.gguf',
        }),
      }),
      expect.anything(),
    )
  })
})

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import AiBackendsPage from '@/features/ai-backends/AiBackendsPage'

const mockListAiBackends = vi.fn()
const mockListAiBackendNodeStatuses = vi.fn()
const mockDeleteAiBackend = vi.fn()
const mockUpdateAiBackend = vi.fn()
const mockCreateAiBackend = vi.fn()
const mockPreflightAiBackend = vi.fn()
const mockUpsertAiBackendPlacement = vi.fn()
const mockListCredentials = vi.fn()
const mockListNodes = vi.fn()

vi.mock('@/api/ai-backends', async () => {
  const actual = await vi.importActual<typeof import('@/api/ai-backends')>('@/api/ai-backends')
  return {
    ...actual,
    listAiBackends: (input?: unknown) => mockListAiBackends(input),
    listAiBackendNodeStatuses: (backendId: string) => mockListAiBackendNodeStatuses(backendId),
    deleteAiBackend: (backendId: string) => mockDeleteAiBackend(backendId),
    updateAiBackend: (backendId: string, input: unknown) => mockUpdateAiBackend(backendId, input),
    createAiBackend: (input: unknown) => mockCreateAiBackend(input),
    preflightAiBackend: (input: unknown) => mockPreflightAiBackend(input),
    upsertAiBackendPlacement: (input: unknown) => mockUpsertAiBackendPlacement(input),
  }
})

vi.mock('@/api/credentials', () => ({
  listCredentials: () => mockListCredentials(),
}))

vi.mock('@/api/nodes', () => ({
  listNodes: () => mockListNodes(),
}))

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  })
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <AiBackendsPage />
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('AiBackendsPage', () => {
  beforeEach(() => {
    mockListCredentials.mockReset()
    mockListCredentials.mockResolvedValue({
      success: true,
      credentials: [],
    })
    mockDeleteAiBackend.mockReset()
    mockDeleteAiBackend.mockResolvedValue({ success: true })
    mockListAiBackendNodeStatuses.mockReset()
    mockListAiBackendNodeStatuses.mockResolvedValue({
      success: true,
      statuses: [
        {
          backend_id: 'b-1',
          node_uuid: 'node-1',
          observed_generation: 1,
          status: 'reconciling',
          status_reason: 'pulling',
          available: false,
          operations: [],
          features: [],
          transports: [],
          last_heartbeat_at_ms: 0,
        },
      ],
    })
    mockUpdateAiBackend.mockReset()
    mockUpdateAiBackend.mockResolvedValue({ success: true })
    mockCreateAiBackend.mockReset()
    mockCreateAiBackend.mockResolvedValue({ success: true, backend: { backend_id: 'created-backend' } })
    mockPreflightAiBackend.mockReset()
    mockPreflightAiBackend.mockResolvedValue({ success: true, results: [] })
    mockUpsertAiBackendPlacement.mockReset()
    mockUpsertAiBackendPlacement.mockResolvedValue({ success: true })
    mockListNodes.mockReset()
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
    mockListAiBackends.mockReset()
    mockListAiBackends.mockResolvedValue({
      success: true,
      total_count: 25,
      backends: [
        {
          backend_id: 'b-1',
          display_name: 'Backend 1',
          provider: 'openai',
          model: 'gpt-4o',
          hosting: 'remote',
          backend_kind: 'openai_chat_completion',
          desired_state: 'enabled',
          management_mode: 'externally_managed',
          generation: 1,
          labels: {},
          metadata: {},
          created_at_ms: 0,
          updated_at_ms: 0,
          spec: {
            operations: ['chat_completions'],
            features: [],
            transports: ['http'],
            weight: 100,
            priority: 0,
          },
        },
      ],
    })
  })

  afterEach(() => {
    vi.clearAllMocks()
    cleanup()
  })

  it('passes server-side filters and pagination params to listAiBackends', async () => {
    renderPage()

    await waitFor(() =>
      expect(mockListAiBackends).toHaveBeenCalledWith({
        q: undefined,
        hosting: undefined,
        desired_state: undefined,
        limit: 20,
        offset: 0,
      }),
    )

    fireEvent.change(screen.getByPlaceholderText('name / provider / model / backend id'), {
      target: { value: 'gpt' },
    })
    fireEvent.change(screen.getAllByRole('combobox')[0], {
      target: { value: 'remote' },
    })
    fireEvent.change(screen.getAllByRole('combobox')[1], {
      target: { value: 'enabled' },
    })

    await waitFor(() =>
      expect(mockListAiBackends).toHaveBeenLastCalledWith({
        q: 'gpt',
        hosting: 'remote',
        desired_state: 'enabled',
        limit: 20,
        offset: 0,
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Next' }))

    await waitFor(() =>
      expect(mockListAiBackends).toHaveBeenLastCalledWith({
        q: 'gpt',
        hosting: 'remote',
        desired_state: 'enabled',
        limit: 20,
        offset: 20,
      }),
    )
  })

  it('shows aggregated status reason summary for each backend row', async () => {
    renderPage()

    await waitFor(() => expect(mockListAiBackendNodeStatuses).toHaveBeenCalledWith('b-1'))
    expect(await screen.findByText('pulling')).toBeTruthy()
  })

  it('preflights local llamacpp sources before creating the backend', async () => {
    renderPage()

    fireEvent.click(screen.getByRole('button', { name: 'Create Backend' }))
    fireEvent.click(screen.getByText('Create Local Backend'))

    await waitFor(() =>
      expect(screen.getByPlaceholderText('e.g. https://host/path/model.gguf')).toBeTruthy(),
    )
    fireEvent.change(screen.getByPlaceholderText('e.g. OpenAI Production'), {
      target: { value: 'Local llama.cpp' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. gpt-4.1'), {
      target: { value: 'llama3.1' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. https://host/path/model.gguf'), {
      target: { value: 'https://models.example.com/llama3.1.gguf' },
    })

    fireEvent.click(screen.getByRole('button', { name: 'Create Local Backend' }))

    await waitFor(() =>
      expect(mockPreflightAiBackend).toHaveBeenCalledWith(
        expect.objectContaining({
          backend: expect.objectContaining({
            hosting: 'local',
            provider: 'llamacpp',
            local: {
              provider_family: 'llama_cpp',
              config: expect.objectContaining({
                model_url: 'https://models.example.com/llama3.1.gguf',
              }),
            },
          }),
          node_uuids: ['node-1'],
        }),
      ),
    )
    expect(mockCreateAiBackend).toHaveBeenCalledTimes(1)
    expect(mockUpsertAiBackendPlacement).toHaveBeenCalledWith(
      expect.objectContaining({
        backend_id: 'created-backend',
        node_uuid: 'node-1',
      }),
    )
  })

  it('preflights remote openai backends before creating the backend', async () => {
    renderPage()

    fireEvent.click(screen.getByRole('button', { name: 'Create Backend' }))
    fireEvent.click(screen.getByText('Create Remote Backend'))

    await waitFor(() =>
      expect(screen.getByPlaceholderText('e.g. OpenAI Production')).toBeTruthy(),
    )
    fireEvent.change(screen.getByPlaceholderText('e.g. OpenAI Production'), {
      target: { value: 'OpenAI Remote' },
    })
    fireEvent.change(screen.getByPlaceholderText('e.g. gpt-4.1'), {
      target: { value: 'gpt-4o-mini' },
    })

    fireEvent.click(screen.getByRole('button', { name: 'Create Remote Backend' }))

    await waitFor(() =>
      expect(mockPreflightAiBackend).toHaveBeenCalledWith(
        expect.objectContaining({
          backend: expect.objectContaining({
            hosting: 'remote',
            provider: 'openai',
            model: 'gpt-4o-mini',
            remote: {
              provider_family: 'open_ai_compatible',
              config: expect.objectContaining({
                base_url: 'https://api.openai.com/v1',
                operations: ['chat_completions'],
              }),
            },
          }),
          node_uuids: ['node-1'],
          verification_policy: 'single_node_strict',
          requested_checks: ['connectivity', 'auth', 'model_access'],
        }),
      ),
    )
    expect(mockCreateAiBackend).toHaveBeenCalledTimes(1)
  })
})

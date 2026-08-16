import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import AiBackendsPage from '@/features/ai-backends/AiBackendsPage'

const mockListAiBackends = vi.fn()
const mockDeleteAiBackend = vi.fn()
const mockUpdateAiBackend = vi.fn()
const mockCreateAiBackend = vi.fn()
const mockListCredentials = vi.fn()

vi.mock('@/api/ai-backends', async () => {
  const actual = await vi.importActual<typeof import('@/api/ai-backends')>('@/api/ai-backends')
  return {
    ...actual,
    listAiBackends: (input?: unknown) => mockListAiBackends(input),
    deleteAiBackend: (backendId: string) => mockDeleteAiBackend(backendId),
    updateAiBackend: (backendId: string, input: unknown) => mockUpdateAiBackend(backendId, input),
    createAiBackend: (input: unknown) => mockCreateAiBackend(input),
  }
})

vi.mock('@/api/credentials', () => ({
  listCredentials: () => mockListCredentials(),
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
    mockUpdateAiBackend.mockReset()
    mockUpdateAiBackend.mockResolvedValue({ success: true })
    mockCreateAiBackend.mockReset()
    mockCreateAiBackend.mockResolvedValue({ success: true })
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
})

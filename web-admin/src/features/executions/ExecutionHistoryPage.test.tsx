import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const mockListExecutionHistory = vi.fn()

vi.mock('@/api/instanceExecution', () => ({
  listExecutionHistory: (input: unknown) => mockListExecutionHistory(input),
}))

import ExecutionHistoryPage from '@/features/executions/ExecutionHistoryPage'

function LocationDisplay() {
  const loc = useLocation()
  return <div data-testid="location">{loc.pathname + loc.search}</div>
}

function renderPage(initialPath: string) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  })
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[initialPath]}>
        <LocationDisplay />
        <Routes>
          <Route path="/executions" element={<ExecutionHistoryPage />} />
          <Route path="/executions/:executionId" element={<div>execution detail page</div>} />
          <Route path="/tasks/:taskId" element={<div>task detail page</div>} />
          <Route path="/instances/:instanceId" element={<div>instance detail page</div>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('ExecutionHistoryPage', () => {
  beforeEach(() => {
    mockListExecutionHistory.mockReset()
    mockListExecutionHistory.mockResolvedValue({
      success: true,
      executions: [
        {
          execution_id: 'e-1',
          task_id: 't-1',
          instance_id: 'i-1',
          node_uuid: 'n-1',
          status: 'completed',
          started_at_ms: 1000,
          completed_at_ms: 1100,
          function_name: 'main',
        },
      ],
      next_page_token: '',
    })
  })

  afterEach(() => {
    vi.clearAllMocks()
    cleanup()
  })

  it('renders archived execution rows', async () => {
    renderPage('/executions')

    await screen.findByText('Execution History')
    await screen.findByText('Archived executions')
    await screen.findByRole('link', { name: 'e-1' })
    await screen.findByRole('link', { name: 't-1' })
    await screen.findByRole('link', { name: 'i-1' })
    await screen.findByText('main')
  })

  it('applies filters through query params', async () => {
    renderPage('/executions')

    await screen.findByText('Execution History')
    fireEvent.change(screen.getByPlaceholderText('Filter by task id'), {
      target: { value: 't-1' },
    })
    fireEvent.change(screen.getByPlaceholderText('Filter by status'), {
      target: { value: 'completed' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Apply' }))

    await waitFor(() => {
      expect(screen.getByTestId('location').textContent).toBe(
        '/executions?task_id=t-1&status=completed',
      )
    })
  })
})

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Routes, Route } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import ExecutionDetailPage from '@/features/executions/ExecutionDetailPage'

const mockGetExecution = vi.fn()
const mockGetExecutionLogs = vi.fn()

vi.mock('@/api/instanceExecution', () => ({
  getExecution: (executionId: string) => mockGetExecution(executionId),
}))

vi.mock('@/api/logs', () => ({
  getExecutionLogs: (input: unknown) => mockGetExecutionLogs(input),
}))

function renderPage(initialPath: string) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  })
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[initialPath]}>
        <Routes>
          <Route path="/executions/:executionId" element={<ExecutionDetailPage />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('ExecutionDetailPage', () => {
  beforeEach(() => {
    mockGetExecution.mockReset()
    mockGetExecutionLogs.mockReset()
    mockGetExecutionLogs.mockResolvedValue({
      success: true,
      execution_id: 'e-1',
      lines: [],
      next_cursor: '0',
      truncated: false,
      completed: true,
    })
  })

  afterEach(() => {
    vi.clearAllMocks()
    cleanup()
  })

  it('disables Logs button when log_ref is missing', async () => {
    mockGetExecution.mockResolvedValue({
      success: true,
      found: true,
      execution: {
        execution_id: 'e-1',
        invocation_id: 'inv-1',
        task_id: 't-1',
        function_name: 'f1',
        node_uuid: 'n-1',
        instance_id: 'i-1',
        status: 'completed',
        started_at_ms: 1000,
        completed_at_ms: 2000,
        updated_at_ms: 2000,
        metadata: {},
        log_ref: null,
      },
    })

    renderPage('/executions/e-1')

    await screen.findByText('Run summary')
    const logsButton = screen.getByRole('button', { name: 'Logs' }) as HTMLButtonElement
    expect(logsButton.disabled).toBe(true)
  })

  it('opens logs dialog and refreshes execution detail', async () => {
    mockGetExecution.mockResolvedValue({
      success: true,
      found: true,
      execution: {
        execution_id: 'e-1',
        invocation_id: 'inv-1',
        task_id: 't-1',
        function_name: 'f1',
        node_uuid: 'n-1',
        instance_id: 'i-1',
        status: 'completed',
        started_at_ms: 1000,
        completed_at_ms: 2000,
        updated_at_ms: 2000,
        metadata: {},
        log_ref: {
          backend: 'b1',
          uri_prefix: 'u1',
          content_type: 'text/plain',
          compression: '',
        },
      },
    })

    renderPage('/executions/e-1')

    await screen.findByText('Run summary')

    const refresh = screen.getByRole('button', { name: 'Refresh' })
    fireEvent.click(refresh)
    await waitFor(() => expect(mockGetExecution).toHaveBeenCalledTimes(2))

    const logsButton = screen.getByRole('button', { name: 'Logs' })
    fireEvent.click(logsButton)

    await screen.findByText('Execution logs')
  })
})

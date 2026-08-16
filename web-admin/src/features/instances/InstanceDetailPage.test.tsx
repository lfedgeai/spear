import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Routes, Route, useLocation } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import InstanceDetailPage from '@/features/instances/InstanceDetailPage'

const mockListInstanceExecutions = vi.fn()
const mockGetInstance = vi.fn()
const mockDestroyInstance = vi.fn()

vi.mock('@/api/instanceExecution', () => ({
  listInstanceExecutions: (input: unknown) => mockListInstanceExecutions(input),
  getInstance: (instanceId: string) => mockGetInstance(instanceId),
}))

vi.mock('@/api/control', () => ({
  destroyInstance: (input: unknown) => mockDestroyInstance(input),
}))

function LocationDisplay() {
  const loc = useLocation()
  return <div data-testid="location">{loc.pathname}</div>
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
          <Route path="/instances/:instanceId" element={<InstanceDetailPage />} />
          <Route path="/executions/:executionId" element={<div>execution page</div>} />
          <Route path="/tasks/:taskId" element={<div>task detail page</div>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('InstanceDetailPage', () => {
  beforeEach(() => {
    mockListInstanceExecutions.mockReset()
    mockGetInstance.mockReset()
    mockDestroyInstance.mockReset()
    mockListInstanceExecutions.mockResolvedValue({
      success: true,
      executions: [
        {
          execution_id: 'e-1',
          task_id: 't-1',
          status: 'completed',
          started_at_ms: 1000,
          completed_at_ms: 2000,
          function_name: 'f1',
        },
      ],
      next_page_token: '',
    })
    mockGetInstance.mockResolvedValue({
      success: true,
      found: true,
      active: true,
      instance: {
        instance_id: 'i-1',
        task_id: 't-1',
        node_uuid: 'n-1',
        status: 'running',
        created_at_ms: 1000,
        last_seen_ms: 2000,
        updated_at_ms: 2000,
        current_execution_id: '',
        metadata: {},
      },
    })
    mockDestroyInstance.mockResolvedValue({
      success: true,
      instance_id: 'i-1',
      node_uuid: 'n-1',
    })
  })

  afterEach(() => {
    vi.clearAllMocks()
    cleanup()
  })

  it('shows inferred task link when executions exist', async () => {
    renderPage('/instances/i-1')

    await screen.findByText('e-1')
    const viewTask = screen.getByRole('button', { name: 'View task' })
    expect(viewTask).not.toBeNull()
  })

  it('navigates to execution on row click', async () => {
    renderPage('/instances/i-1')

    await screen.findByText('e-1')
    const link = screen.getByRole('link', { name: 'e-1' })
    fireEvent.click(link.closest('tr')!)

    await waitFor(() => {
      expect(screen.getByTestId('location').textContent).toBe('/executions/e-1')
    })
  })

  it('redirects back to task detail after destroy removes the instance', async () => {
    mockGetInstance
      .mockResolvedValueOnce({
        success: true,
        found: true,
        active: true,
        instance: {
          instance_id: 'i-1',
          task_id: 't-1',
          node_uuid: 'n-1',
          status: 'running',
          created_at_ms: 1000,
          last_seen_ms: 2000,
          updated_at_ms: 2000,
          current_execution_id: '',
          metadata: {},
        },
      })
      .mockResolvedValueOnce({
        success: true,
        found: false,
        active: false,
        instance: undefined,
      })

    renderPage('/instances/i-1')

    await screen.findByText('e-1')
    fireEvent.click(screen.getByRole('button', { name: 'Destroy' }))
    await screen.findByText('Destroy replica')
    fireEvent.click(screen.getByRole('button', { name: 'Destroy' }))

    await waitFor(() => {
      expect(mockDestroyInstance).toHaveBeenCalledWith({
        instance_id: 'i-1',
        node_uuid: 'n-1',
        reason: undefined,
      })
    })
    await waitFor(() => {
      expect(screen.getByTestId('location').textContent).toBe('/tasks/t-1')
    })
  })
})

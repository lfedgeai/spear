import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const mockGetTaskDetail = vi.fn()
const mockListTaskInstances = vi.fn()
const mockDestroyInstance = vi.fn()

vi.mock('@/api/tasks', () => ({
  getTaskDetail: (taskId: string) => mockGetTaskDetail(taskId),
}))

vi.mock('@/api/instanceExecution', () => ({
  listTaskInstances: (input: unknown) => mockListTaskInstances(input),
}))

vi.mock('@/api/control', () => ({
  destroyInstance: (input: unknown) => mockDestroyInstance(input),
}))

import TaskInstancesPage from '@/features/tasks/TaskInstancesPage'

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
          <Route path="/tasks/:taskId/instances" element={<TaskInstancesPage />} />
          <Route path="/tasks/:taskId" element={<div>task detail page</div>} />
          <Route path="/instances/:instanceId" element={<div>instance detail page</div>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('TaskInstancesPage', () => {
  beforeEach(() => {
    mockGetTaskDetail.mockReset()
    mockListTaskInstances.mockReset()
    mockDestroyInstance.mockReset()

    mockGetTaskDetail.mockResolvedValue({
      found: true,
      task: {
        task_id: 't-1',
        name: 'demo-task',
        status: 'deleting',
        priority: 'normal',
        desired_replicas: 2,
        endpoint: 'demo',
        version: 'v1',
        deletion_requested_at: 1_700_000_000,
        deletion_reason: 'cleanup',
      },
    })
    mockListTaskInstances.mockResolvedValue({
      success: true,
      instances: [
        {
          instance_id: 'i-1',
          task_id: 't-1',
          node_uuid: 'n-1',
          status: 'running',
          created_at_ms: 900,
          updated_at_ms: 1000,
          last_seen_ms: 1100,
          current_execution_id: 'e-1',
        },
        {
          instance_id: 'i-2',
          task_id: 't-1',
          node_uuid: 'n-2',
          status: 'terminating',
          created_at_ms: 1200,
          updated_at_ms: 1300,
          last_seen_ms: 1300,
          current_execution_id: '',
        },
      ],
      next_page_token: '',
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

  it('renders task-scoped instances overview and cleanup progress', async () => {
    renderPage('/tasks/t-1/instances')

    await screen.findByText('Task instances')
    await screen.findByText('Replica overview')
    await screen.findByText('Cleanup progress')
    await screen.findByText('Instances total')
    await screen.findByText('Instances remaining')
    await screen.findByText('cleanup pending')
    await screen.findByText('All instances for this task')
    await screen.findByText('i-1')
    await screen.findByText('i-2')
  })

  it('navigates back to task detail', async () => {
    renderPage('/tasks/t-1/instances')

    await screen.findByText('Task instances')
    fireEvent.click(screen.getByRole('button', { name: 'Back to task' }))

    await waitFor(() => {
      expect(screen.getByTestId('location').textContent).toBe('/tasks/t-1')
    })
  })
})

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Routes, Route, useLocation } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const mockGetTaskDetail = vi.fn()
const mockDeleteTask = vi.fn()
const mockListTaskInstances = vi.fn()
const mockDestroyInstance = vi.fn()

vi.mock('@/api/tasks', () => ({
  getTaskDetail: (taskId: string) => mockGetTaskDetail(taskId),
  deleteTask: (input: unknown) => mockDeleteTask(input),
}))

vi.mock('@/api/instanceExecution', () => ({
  listTaskInstances: (input: unknown) => mockListTaskInstances(input),
}))

vi.mock('@/api/control', () => ({
  destroyInstance: (input: unknown) => mockDestroyInstance(input),
}))

vi.mock('@/features/tasks/DeleteTaskDialog', () => ({
  default: (props: {
    open: boolean
    onDeleted?: () => void
    onOpenChange: (open: boolean) => void
  }) =>
    props.open ? (
      <div>
        <div>Delete task</div>
        <button
          onClick={() => {
            props.onOpenChange(false)
            props.onDeleted?.()
          }}
        >
          Confirm delete
        </button>
      </div>
    ) : null,
}))

import TaskDetailPage from '@/features/tasks/TaskDetailPage'

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
          <Route path="/tasks/:taskId" element={<TaskDetailPage />} />
          <Route path="/tasks/:taskId/instances" element={<div>task instances page</div>} />
          <Route path="/tasks" element={<div>tasks page</div>} />
          <Route path="/executions" element={<div>execution history page</div>} />
          <Route path="/instances/:instanceId" element={<div>instance page</div>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('TaskDetailPage', () => {
  beforeEach(() => {
    mockGetTaskDetail.mockReset()
    mockDeleteTask.mockReset()
    mockListTaskInstances.mockReset()
    mockDestroyInstance.mockReset()

    mockGetTaskDetail.mockResolvedValue({
      found: true,
      task: {
        task_id: 't-1',
        name: 'demo-task',
        status: 'registered',
        priority: 'normal',
        desired_replicas: 2,
        scheduling_strategy: 'spread',
        endpoint: 'demo',
        version: 'v1',
        capabilities: ['echo'],
      },
    })
    mockDeleteTask.mockResolvedValue({
      success: true,
      message: 'Task deletion requested',
      task_id: 't-1',
      task: { task_id: 't-1', status: 'deleting' },
    })
    mockDestroyInstance.mockResolvedValue({
      success: true,
      instance_id: 'i-1',
      node_uuid: 'n-1',
    })
    mockListTaskInstances.mockImplementation(
      (input: { task_id: string; page_token?: string }) => {
        if (!input.page_token) {
          return Promise.resolve({
            success: true,
            instances: [
              {
                instance_id: 'i-1',
                task_id: 't-1',
                node_uuid: 'n-1',
                status: 'running',
                created_at_ms: 900,
                updated_at_ms: 1000,
                last_seen_ms: 1000,
                current_execution_id: 'e-1',
              },
            ],
            next_page_token: 'p2',
          })
        }
        return Promise.resolve({
          success: true,
          instances: [
            {
              instance_id: 'i-2',
              task_id: 't-1',
              node_uuid: 'n-2',
              status: 'idle',
              created_at_ms: 1900,
              updated_at_ms: 2000,
              last_seen_ms: 2000,
              current_execution_id: '',
            },
          ],
          next_page_token: '',
        })
      },
    )
  })

  afterEach(() => {
    vi.clearAllMocks()
    cleanup()
  })

  it('renders task overview and instances paging', async () => {
    renderPage('/tasks/t-1')

    await screen.findByText('Replica status')
    await screen.findByText('Desired replicas')
    await screen.findByText('2')
    await screen.findByText('Instances total')
    await screen.findByRole('button', { name: 'Execution history' })
    await screen.findAllByRole('link', { name: 'i-1' })

    const loadMore = await screen.findByRole('button', { name: 'Load more' })
    fireEvent.click(loadMore)

    await screen.findByText('i-2')
  })

  it('navigates to instance on row click', async () => {
    renderPage('/tasks/t-1')

    const links = await screen.findAllByRole('link', { name: 'i-1' })
    const cell = links.find((link) => link.closest('tr')?.textContent?.includes('Destroy'))!
    fireEvent.click(cell.closest('tr')!)

    await waitFor(() => {
      expect(screen.getByTestId('location').textContent).toBe('/instances/i-1')
    })
  })

  it('renders delete action from task detail page', async () => {
    renderPage('/tasks/t-1')

    await screen.findByText('Replica status')

    expect(screen.getAllByRole('button', { name: 'Delete' })[0]).toBeTruthy()
  })

  it('navigates to task instances page from task detail', async () => {
    renderPage('/tasks/t-1')

    await screen.findByText('Current replicas')
    fireEvent.click(screen.getByRole('button', { name: 'View all instances' }))

    await waitFor(() => {
      expect(screen.getByTestId('location').textContent).toBe('/tasks/t-1/instances')
    })
  })

  it('navigates to execution history page from task detail', async () => {
    renderPage('/tasks/t-1')

    await screen.findByRole('button', { name: 'Execution history' })
    fireEvent.click(screen.getByRole('button', { name: 'Execution history' }))

    await waitFor(() => {
      expect(screen.getByTestId('location').textContent).toBe('/executions')
    })
  })

  it('shows cleanup status for deleting tasks', async () => {
    mockGetTaskDetail.mockResolvedValueOnce({
      found: true,
      task: {
        task_id: 't-1',
        name: 'demo-task',
        status: 'deleting',
        priority: 'normal',
        desired_replicas: 2,
        scheduling_strategy: 'spread',
        endpoint: 'demo',
        version: 'v1',
        deletion_requested_at: 1_700_000_000,
        deletion_reason: 'cleanup',
      },
    })
    mockListTaskInstances.mockResolvedValueOnce({
      success: true,
      instances: [
        {
          instance_id: 'i-1',
          task_id: 't-1',
          node_uuid: 'n-1',
          status: 'running',
          created_at_ms: 900,
          updated_at_ms: 1000,
          last_seen_ms: 1000,
          current_execution_id: 'e-1',
        },
      ],
      next_page_token: '',
    })

    renderPage('/tasks/t-1')

    await screen.findByText('Cleanup status')
    await screen.findByText('Instances remaining')
    await screen.findByText('Active executions remaining')
    await screen.findByText('cleanup pending')
  })

  it('shows destroying state and removes instance after destroy succeeds', async () => {
    mockListTaskInstances
      .mockResolvedValueOnce({
        success: true,
        instances: [
          {
            instance_id: 'i-1',
            task_id: 't-1',
            node_uuid: 'n-1',
            status: 'running',
            created_at_ms: 900,
            updated_at_ms: 1000,
            last_seen_ms: 1000,
            current_execution_id: '',
          },
        ],
        next_page_token: '',
      })
      .mockResolvedValueOnce({
        success: true,
        instances: [],
        next_page_token: '',
      })

    renderPage('/tasks/t-1')

    await screen.findAllByRole('link', { name: 'i-1' })
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
    await screen.findByRole('button', { name: 'Destroying…' })
    await waitFor(() => {
      expect(screen.getByText('No replicas are currently reported for this task.')).toBeTruthy()
    })
  })
})

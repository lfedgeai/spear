import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen } from '@testing-library/react'
import { MemoryRouter, Routes, Route } from 'react-router-dom'
import { afterEach, beforeEach, describe, it, vi } from 'vitest'

const mockListTasks = vi.fn()
const mockCreateTask = vi.fn()
const mockListFiles = vi.fn()
const mockListMcpServers = vi.fn()

vi.mock('@/api/tasks', () => ({
  listTasks: (input: unknown) => mockListTasks(input),
  createTask: (input: unknown) => mockCreateTask(input),
}))

vi.mock('@/api/files', () => ({
  listFiles: (input: unknown) => mockListFiles(input),
}))

vi.mock('@/api/mcp', () => ({
  listMcpServers: () => mockListMcpServers(),
}))

vi.mock('@/features/tasks/DeleteTaskDialog', () => ({
  default: () => null,
}))

import TasksPage from '@/features/tasks/TasksPage'

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 } },
  })
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={['/tasks']}>
        <Routes>
          <Route path="/tasks" element={<TasksPage />} />
          <Route path="/tasks/:taskId" element={<div>task detail</div>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('TasksPage', () => {
  beforeEach(() => {
    mockListTasks.mockReset()
    mockCreateTask.mockReset()
    mockListFiles.mockReset()
    mockListMcpServers.mockReset()

    mockListTasks.mockResolvedValue({
      tasks: [
        {
          task_id: 't-1',
          name: 'demo-task',
          endpoint: 'demo',
          status: 'registered',
          priority: 'normal',
          desired_replicas: 3,
          active_instances: 1,
          ready_instances: 1,
          reconciling: true,
          underprovisioned: true,
          scheduling_strategy: 'spread',
          registered_at: 1_700_000_000,
          version: 'v1',
          last_heartbeat: 1_700_000_000,
        },
      ],
      total_count: 1,
    })
    mockListFiles.mockResolvedValue({ files: [], total_count: 0 })
    mockListMcpServers.mockResolvedValue({ success: true, servers: [] })
  })

  afterEach(() => {
    vi.clearAllMocks()
    cleanup()
  })

  it('renders replica convergence summary in task rows', async () => {
    renderPage()

    await screen.findByText('Observe workload specs, replica targets, and current replica convergence')
    await screen.findByText('Task / Endpoint')
    await screen.findByText('Replica state')
    await screen.findByText('demo-task')
    await screen.findByText('demo')
    await screen.findByText('reconciling')
    await screen.findByText('active 1 / ready 1')
    await screen.findByText('3 x spread')
  })
})

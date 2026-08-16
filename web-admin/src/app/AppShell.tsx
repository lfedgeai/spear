import { useMemo } from 'react'
import { HashRouter, Link, NavLink, Route, Routes } from 'react-router-dom'
import { LayoutDashboard, Moon, Server, Settings, Sun, FileBox, ListTodo, Plug, Boxes, History } from 'lucide-react'
import { Toaster } from 'sonner'

import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { useThemeMode } from '@/app/useThemeMode'
import DashboardPage from '@/features/dashboard/DashboardPage'
import NodesPage from '@/features/nodes/NodesPage'
import NodeDetailPage from '@/features/nodes/NodeDetailPage'
import TasksPage from '@/features/tasks/TasksPage'
import TaskDetailPage from '@/features/tasks/TaskDetailPage'
import TaskInstancesPage from '@/features/tasks/TaskInstancesPage'
import FilesPage from '@/features/files/FilesPage'
import FileDetailPage from '@/features/files/FileDetailPage'
import AiBackendsPage from '@/features/ai-backends/AiBackendsPage'
import AiBackendDetailPage from '@/features/ai-backends/AiBackendDetailPage'
import CredentialsPage from '@/features/ai-credentials/CredentialsPage'
import AiModelsPage from '@/features/ai-models/AiModelsPage'
import AiModelDetailPage from '@/features/ai-models/AiModelDetailPage'
import McpPage from '@/features/mcp/McpPage'
import McpServerDetailPage from '@/features/mcp/McpServerDetailPage'
import SettingsPage from '@/features/settings/SettingsPage'
import InstanceDetailPage from '@/features/instances/InstanceDetailPage'
import ExecutionHistoryPage from '@/features/executions/ExecutionHistoryPage'
import ExecutionDetailPage from '@/features/executions/ExecutionDetailPage'

type NavItem = {
  label: string
  icon: typeof Boxes
  to?: string
  end?: boolean
  children?: NavItem[]
}

function Shell() {
  const { mode, setMode } = useThemeMode()
  const nav = useMemo(
    (): NavItem[] => [
      { to: '/', label: 'Dashboard', icon: LayoutDashboard },
      { to: '/nodes', label: 'Nodes', icon: Server },
      { to: '/tasks', label: 'Tasks', icon: ListTodo },
      { to: '/executions', label: 'Execution History', icon: History },
      { to: '/files', label: 'Files', icon: FileBox },
      {
        label: 'AI Backends',
        icon: Boxes,
        children: [
          { to: '/ai-backends', label: 'Backends', icon: Boxes, end: true },
          { to: '/ai-backends/credentials', label: 'Credentials', icon: Boxes, end: true },
        ],
      },
      { to: '/mcp', label: 'MCP', icon: Plug },
      { to: '/settings', label: 'Settings', icon: Settings },
    ],
    [],
  )

  return (
    <div className="h-full bg-[hsl(var(--background))] text-[hsl(var(--foreground))]">
      <Toaster richColors position="top-right" />
      <div className="mx-auto flex h-full max-w-[1400px]">
        <aside className="hidden w-64 flex-col border-r border-[hsl(var(--border))] p-4 md:flex">
          <div className="flex items-center gap-3">
            <div className="flex h-8 w-8 items-center justify-center rounded-[calc(var(--radius)-2px)] bg-[hsl(var(--primary))] text-xs font-semibold text-[hsl(var(--primary-foreground))]">
              SP
            </div>
            <div>
              <div className="text-sm font-semibold">SPEAR Operations</div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">
                Operations Console
              </div>
            </div>
          </div>

          <nav className="mt-6 space-y-1">
            {nav.map((item) =>
              item.children && item.children.length > 0 ? (
                <div key={item.label} className="space-y-1">
                  <div className="flex items-center gap-2 px-3 py-2 text-sm font-medium text-[hsl(var(--foreground))]">
                    <item.icon className="h-4 w-4" />
                    {item.label}
                  </div>
                  <div className="space-y-1 pl-3">
                    {item.children.map((child) => (
                      <NavLink
                        key={child.to}
                        to={child.to || '/'}
                        end={child.end}
                        data-testid={`nav-${child.label.toLowerCase()}`}
                        className={({ isActive }) =>
                          cn(
                            'flex items-center gap-2 rounded-[calc(var(--radius)-4px)] px-3 py-2 text-sm transition-colors',
                            isActive
                              ? 'bg-[hsl(var(--secondary))] text-[hsl(var(--foreground))]'
                              : 'text-[hsl(var(--muted-foreground))] hover:bg-[hsl(var(--accent))] hover:text-[hsl(var(--foreground))]',
                          )
                        }
                      >
                        <child.icon className="h-4 w-4 opacity-50" />
                        {child.label}
                      </NavLink>
                    ))}
                  </div>
                </div>
              ) : (
                <NavLink
                  key={item.to}
                  to={item.to || '/'}
                  end={item.end}
                  data-testid={`nav-${item.label.toLowerCase()}`}
                  className={({ isActive }) =>
                    cn(
                      'flex items-center gap-2 rounded-[calc(var(--radius)-4px)] px-3 py-2 text-sm transition-colors',
                      isActive
                        ? 'bg-[hsl(var(--secondary))] text-[hsl(var(--foreground))]'
                        : 'text-[hsl(var(--muted-foreground))] hover:bg-[hsl(var(--accent))] hover:text-[hsl(var(--foreground))]',
                    )
                  }
                >
                  <item.icon className="h-4 w-4" />
                  {item.label}
                </NavLink>
              ),
            )}
          </nav>

          <div className="mt-auto">
            <div className="rounded-[var(--radius)] border border-[hsl(var(--border))] bg-[hsl(var(--card))] p-3 text-xs text-[hsl(var(--muted-foreground))]">
              <div className="font-medium text-[hsl(var(--foreground))]">
                Tips
              </div>
              <div className="mt-1">Set token in Settings if enabled.</div>
            </div>
          </div>
        </aside>

        <main className="flex min-w-0 flex-1 flex-col">
          <header className="flex items-center justify-between border-b border-[hsl(var(--border))] bg-[hsl(var(--background))] px-4 py-3 md:px-6">
            <div className="flex items-center gap-3">
              <Link
                to="/"
                className="md:hidden inline-flex items-center gap-2 text-sm font-semibold"
              >
                <div className="flex h-6 w-6 items-center justify-center rounded-[calc(var(--radius)-4px)] bg-[hsl(var(--primary))] text-[10px] font-semibold text-[hsl(var(--primary-foreground))]">
                  SP
                </div>
                SPEAR Operations
              </Link>
              <div className="hidden md:block">
                <div className="text-sm font-semibold">SPEAR Operations Console</div>
                <div className="text-xs text-[hsl(var(--muted-foreground))]">
                  Enterprise console
                </div>
              </div>
            </div>

            <div className="flex items-center gap-2">
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setMode(mode === 'dark' ? 'light' : 'dark')}
              >
                {mode === 'dark' ? (
                  <Sun className="h-4 w-4" />
                ) : (
                  <Moon className="h-4 w-4" />
                )}
                {mode === 'dark' ? 'Light' : 'Dark'}
              </Button>
            </div>
          </header>

          <div className="min-h-0 flex-1 overflow-auto bg-[hsl(var(--secondary))]">
            <div className="p-4 md:p-6">
              <Routes>
                <Route path="/" element={<DashboardPage />} />
                <Route path="/nodes" element={<NodesPage />} />
                <Route path="/nodes/:uuid" element={<NodeDetailPage />} />
                <Route path="/tasks" element={<TasksPage />} />
                <Route path="/tasks/:taskId" element={<TaskDetailPage />} />
                <Route path="/tasks/:taskId/instances" element={<TaskInstancesPage />} />
                <Route path="/instances/:instanceId" element={<InstanceDetailPage />} />
                <Route path="/executions" element={<ExecutionHistoryPage />} />
                <Route path="/executions/:executionId" element={<ExecutionDetailPage />} />
                <Route path="/files" element={<FilesPage />} />
                <Route path="/files/:id" element={<FileDetailPage />} />
                <Route path="/ai-backends" element={<AiBackendsPage />} />
                <Route path="/ai-backends/:backendId" element={<AiBackendDetailPage />} />
                <Route path="/ai-backends/credentials" element={<CredentialsPage />} />
                <Route path="/ai-models/remote" element={<AiModelsPage hosting="remote" />} />
                <Route path="/ai-models/local" element={<AiModelsPage hosting="local" />} />
                <Route
                  path="/ai-models/:hosting/:provider/:model"
                  element={<AiModelDetailPage />}
                />
                <Route path="/mcp" element={<McpPage />} />
                <Route path="/mcp/:serverId" element={<McpServerDetailPage />} />
                <Route path="/settings" element={<SettingsPage />} />
              </Routes>
            </div>
          </div>
        </main>
      </div>
    </div>
  )
}

export default function AppShell() {
  return (
    <HashRouter>
      <Shell />
    </HashRouter>
  )
}

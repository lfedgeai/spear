import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Plus, Search } from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { toast } from 'sonner'

import { createTask, listTasks } from '@/api/tasks'
import { listFiles } from '@/api/files'
import { listMcpServers } from '@/api/mcp'
import type { TaskSummary } from '@/api/types'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Dialog, DialogContent, DialogHeader } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { TaskStatusBadge } from '@/features/shared/status-badges'
import DeleteTaskDialog from '@/features/tasks/DeleteTaskDialog'
import { cn, isValidEndpointName, normalizeEndpointName } from '@/lib/utils'

function formatTs(ts: number) {
  if (!ts) return '-'
  const d = new Date(ts * 1000)
  return d.toLocaleString()
}

function ReplicaHealthBadge(props: { reconciling?: boolean; underprovisioned?: boolean }) {
  if (props.reconciling || props.underprovisioned) {
    return <Badge variant="secondary">reconciling</Badge>
  }
  return <Badge variant="success">at target</Badge>
}

type CreateForm = {
  name: string
  description: string
  priority: string
  desired_replicas: string
  scheduling_strategy: string
  endpoint: string
  version: string
  capabilities: string
  executable_type: string
  executable_uri: string
  executable_name: string
  checksum: string
  args: string
  env: string
  mcp_enabled: boolean
  mcp_tool_allowlist: string
  mcp_tool_denylist: string
}

type UriScheme = 'smsfile' | 'https' | 's3' | 'minio'

function schemePrefix(s: UriScheme) {
  if (s === 'https') return 'https://'
  if (s === 's3') return 's3://'
  if (s === 'minio') return 'minio://'
  return 'smsfile://'
}

function parseCsv(v: string) {
  return v
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean)
}

function toJsonArrayString(items: string[]) {
  return JSON.stringify(items)
}

function parseEnv(v: string) {
  const lines = v
    .split('\n')
    .map((s) => s.trim())
    .filter(Boolean)
  const out: Record<string, string> = {}
  for (const line of lines) {
    const i = line.indexOf('=')
    if (i > 0) out[line.slice(0, i).trim()] = line.slice(i + 1).trim()
  }
  return out
}

function CreateTaskField(props: {
  label: string
  children: ReactNode
  className?: string
  hint?: string
}) {
  return (
    <div className={cn('space-y-1', props.className)}>
      <div className="text-sm font-medium">{props.label}</div>
      {props.children}
      {props.hint ? (
        <div className="text-xs text-[hsl(var(--muted-foreground))]">{props.hint}</div>
      ) : null}
    </div>
  )
}

function CreateTaskSection(props: {
  title: string
  description?: string
  children: ReactNode
  className?: string
}) {
  return (
    <div
      className={cn(
        'col-span-2 space-y-3 rounded-[var(--radius)] border border-[hsl(var(--border))] p-3',
        props.className,
      )}
    >
      <div>
        <div className="text-sm font-medium">{props.title}</div>
        {props.description ? (
          <div className="text-xs text-[hsl(var(--muted-foreground))]">{props.description}</div>
        ) : null}
      </div>
      {props.children}
    </div>
  )
}

function CreateTaskDialog(props: {
  open: boolean
  onOpenChange: (v: boolean) => void
  onCreated: () => void
}) {
  const mcpServersQuery = useQuery({
    queryKey: ['mcp-servers-for-task'],
    queryFn: listMcpServers,
    staleTime: 10_000,
  })

  const [scheme, setScheme] = useState<UriScheme>('smsfile')
  const [pickerOpen, setPickerOpen] = useState(false)
  const [fileQ, setFileQ] = useState('')
  const [fileOffset, setFileOffset] = useState(0)
  const fileLimit = 60
  const [fileRows, setFileRows] = useState<import('@/api/types').FileItem[]>([])
  const [fileTotal, setFileTotal] = useState<number | null>(null)
  const filesQuery = useQuery({
    queryKey: ['files-for-task', fileQ, fileOffset, pickerOpen],
    queryFn: () => listFiles({ q: fileQ || undefined, limit: fileLimit, offset: fileOffset }),
    enabled: pickerOpen,
    staleTime: 5_000,
  })

  useEffect(() => {
    if (!pickerOpen) return
    if (!filesQuery.data) return
    setFileTotal(filesQuery.data.total_count ?? null)
    if (fileOffset === 0) {
      setFileRows(filesQuery.data.files || [])
    } else {
      setFileRows((cur) => [...cur, ...(filesQuery.data.files || [])])
    }
  }, [filesQuery.data, fileOffset, pickerOpen])

  useEffect(() => {
    if (!pickerOpen) return
    setFileOffset(0)
  }, [fileQ, pickerOpen])

  const [form, setForm] = useState<CreateForm>({
    name: '',
    description: '',
    priority: 'normal',
    desired_replicas: '1',
    scheduling_strategy: 'spread',
    endpoint: '',
    version: 'v1',
    capabilities: '',
    executable_type: 'no-executable',
    executable_uri: '',
    executable_name: '',
    checksum: '',
    args: '',
    env: '',
    mcp_enabled: false,
    mcp_tool_allowlist: '',
    mcp_tool_denylist: '',
  })

  const [endpointTouched, setEndpointTouched] = useState(false)
  const endpointOk = useMemo(
    () => isValidEndpointName(form.endpoint),
    [form.endpoint],
  )

  const mcpServers = useMemo(() => {
    if (!mcpServersQuery.data?.success) return []
    return mcpServersQuery.data.servers ?? []
  }, [mcpServersQuery.data])

  const [mcpFilter, setMcpFilter] = useState('')
  const [mcpAllowed, setMcpAllowed] = useState<Set<string>>(() => new Set())
  const [mcpDefault, setMcpDefault] = useState<Set<string>>(() => new Set())

  useEffect(() => {
    if (!props.open) return
    setScheme('smsfile')
    setPickerOpen(false)
    setFileQ('')
    setFileOffset(0)
    setFileRows([])
    setFileTotal(null)
    setEndpointTouched(false)
    setMcpAllowed(new Set())
    setMcpDefault(new Set())
    setMcpFilter('')
    setForm({
      name: '',
      description: '',
      priority: 'normal',
      desired_replicas: '1',
      scheduling_strategy: 'spread',
      endpoint: '',
      version: 'v1',
      capabilities: '',
      executable_type: 'no-executable',
      executable_uri: '',
      executable_name: '',
      checksum: '',
      args: '',
      env: '',
      mcp_enabled: false,
      mcp_tool_allowlist: '',
      mcp_tool_denylist: '',
    })
  }, [props.open])

  const mcpDefaultIds = useMemo(
    () => Array.from(mcpDefault).sort(),
    [mcpDefault],
  )
  const mcpAllowedIds = useMemo(
    () => Array.from(mcpAllowed).sort(),
    [mcpAllowed],
  )

  const mcpCanSubmit = !form.mcp_enabled || mcpDefaultIds.length > 0
  const canSubmit = !!(form.name && endpointOk && form.version && mcpCanSubmit)

  const filteredMcpServers = useMemo(() => {
    const needle = mcpFilter.trim().toLowerCase()
    if (!needle) return mcpServers
    return mcpServers.filter((s) => {
      const id = (s.server_id || '').toLowerCase()
      const name = (s.display_name || '').toLowerCase()
      return id.includes(needle) || name.includes(needle)
    })
  }, [mcpServers, mcpFilter])

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent>
        <DialogHeader title="Create task" description="Register a task in SMS" />
        <div className="min-h-0 flex-1 overflow-y-auto pr-1">
          <div className="grid grid-cols-2 gap-3">
            <CreateTaskSection
              title="Task Basics"
              description="Identify the task and define default scheduling behavior."
            >
              <div className="grid grid-cols-2 gap-3">
                <CreateTaskField label="Name" className="col-span-2">
                  <Input
                    placeholder="Name"
                    value={form.name}
                    onChange={(e) => {
                      const nextName = e.target.value
                      setForm((f) => ({
                        ...f,
                        name: nextName,
                        endpoint: endpointTouched
                          ? f.endpoint
                          : normalizeEndpointName(nextName),
                      }))
                    }}
                  />
                </CreateTaskField>
                <CreateTaskField label="Description" className="col-span-2">
                  <Input
                    placeholder="Description"
                    value={form.description}
                    onChange={(e) => setForm((f) => ({ ...f, description: e.target.value }))}
                  />
                </CreateTaskField>
                <CreateTaskField label="Priority">
                  <select
                    className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
                    value={form.priority}
                    onChange={(e) => setForm((f) => ({ ...f, priority: e.target.value }))}
                  >
                    <option value="low">low</option>
                    <option value="normal">normal</option>
                    <option value="high">high</option>
                    <option value="urgent">urgent</option>
                  </select>
                </CreateTaskField>
                <CreateTaskField label="Desired Replicas">
                  <Input
                    type="number"
                    min={1}
                    placeholder="Desired replicas"
                    value={form.desired_replicas}
                    onChange={(e) => setForm((f) => ({ ...f, desired_replicas: e.target.value }))}
                  />
                </CreateTaskField>
                <CreateTaskField label="Scheduling Strategy" className="col-span-2">
                  <select
                    className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
                    value={form.scheduling_strategy}
                    onChange={(e) => setForm((f) => ({ ...f, scheduling_strategy: e.target.value }))}
                    aria-label="Scheduling strategy"
                  >
                    <option value="spread">spread</option>
                  </select>
                </CreateTaskField>
              </div>
            </CreateTaskSection>

            <div className="col-span-2 overflow-hidden rounded-[var(--radius)] border border-[hsl(var(--border))]">
            <div className="flex items-center justify-between bg-[hsl(var(--muted))] px-3 py-2">
              <div>
                <div className="text-sm font-medium">MCP tools</div>
                <div className="text-xs text-[hsl(var(--muted-foreground))]">
                  Default deny; pick minimal servers per task
                </div>
              </div>
              <input
                type="checkbox"
                checked={form.mcp_enabled}
                onChange={(e) => {
                  const on = e.target.checked
                  setForm((f) => ({ ...f, mcp_enabled: on }))
                  if (!on) {
                    setMcpAllowed(new Set())
                    setMcpDefault(new Set())
                  }
                }}
                aria-label="Enable MCP tools"
              />
            </div>

            {form.mcp_enabled ? (
              <div className="space-y-3 p-3">
                {!mcpServersQuery.data?.success ? (
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">
                    Failed to load MCP registry.
                  </div>
                ) : mcpServers.length === 0 ? (
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">
                    No MCP servers found. Create servers in MCP page first.
                  </div>
                ) : (
                  <>
                    <div>
                      <div className="mb-1 text-xs font-medium text-[hsl(var(--muted-foreground))]">
                        Search servers
                      </div>
                      <Input
                        value={mcpFilter}
                        onChange={(e) => setMcpFilter(e.target.value)}
                        placeholder="Filter by server_id or display_name"
                      />
                    </div>

                    <div className="max-h-[220px] overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))]">
                      <div className="grid grid-cols-12 border-b border-[hsl(var(--border))] bg-[hsl(var(--secondary))] px-3 py-2 text-xs font-medium text-[hsl(var(--muted-foreground))]">
                        <div className="col-span-7">Server</div>
                        <div className="col-span-3">Allow</div>
                        <div className="col-span-2">Default</div>
                      </div>
                      {filteredMcpServers.map((s) => {
                        const isAllowed = mcpAllowed.has(s.server_id)
                        const isDefault = mcpDefault.has(s.server_id)
                        return (
                          <div
                            key={s.server_id}
                            className="grid grid-cols-12 items-center gap-2 border-b border-[hsl(var(--border))] px-3 py-2 text-sm last:border-b-0"
                          >
                            <div className="col-span-7 min-w-0">
                              <div className="truncate font-mono text-xs">{s.server_id}</div>
                              <div className="mt-0.5 truncate text-xs text-[hsl(var(--muted-foreground))]">
                                {s.display_name || '-'}
                              </div>
                            </div>

                            <div className="col-span-3">
                              <input
                                type="checkbox"
                                checked={isAllowed}
                                onChange={(e) => {
                                  const next = new Set(mcpAllowed)
                                  const nextDefault = new Set(mcpDefault)
                                  if (e.target.checked) {
                                    next.add(s.server_id)
                                  } else {
                                    next.delete(s.server_id)
                                    nextDefault.delete(s.server_id)
                                  }
                                  setMcpAllowed(next)
                                  setMcpDefault(nextDefault)
                                }}
                                aria-label={`Allow ${s.server_id}`}
                              />
                            </div>

                            <div className="col-span-2 flex justify-end">
                              <input
                                type="checkbox"
                                checked={isDefault}
                                onChange={(e) => {
                                  const nextAllowed = new Set(mcpAllowed)
                                  const nextDefault = new Set(mcpDefault)
                                  if (e.target.checked) {
                                    nextDefault.add(s.server_id)
                                    nextAllowed.add(s.server_id)
                                  } else {
                                    nextDefault.delete(s.server_id)
                                  }
                                  setMcpAllowed(nextAllowed)
                                  setMcpDefault(nextDefault)
                                }}
                                aria-label={`Default ${s.server_id}`}
                              />
                            </div>
                          </div>
                        )
                      })}
                      {filteredMcpServers.length === 0 ? (
                        <div className="px-3 py-4 text-xs text-[hsl(var(--muted-foreground))]">
                          No matches
                        </div>
                      ) : null}
                    </div>

                    {!mcpCanSubmit ? (
                      <div className="text-xs text-[hsl(var(--destructive))]">
                        Pick at least one default MCP server.
                      </div>
                    ) : null}

                    <div className="grid grid-cols-2 gap-2">
                      <div className="col-span-1">
                        <Input
                          placeholder="Tool allowlist (comma patterns, optional)"
                          value={form.mcp_tool_allowlist}
                          onChange={(e) =>
                            setForm((f) => ({ ...f, mcp_tool_allowlist: e.target.value }))
                          }
                        />
                      </div>
                      <div className="col-span-1">
                        <Input
                          placeholder="Tool denylist (comma patterns, optional)"
                          value={form.mcp_tool_denylist}
                          onChange={(e) =>
                            setForm((f) => ({ ...f, mcp_tool_denylist: e.target.value }))
                          }
                        />
                      </div>
                    </div>
                  </>
                )}
              </div>
            ) : null}
            </div>

            <CreateTaskSection
              title="Routing"
              description="Define how callers address this task and what capabilities it exposes."
            >
              <div className="grid grid-cols-2 gap-3">
                <CreateTaskField label="Endpoint" className="col-span-2" hint="Must match ^[A-Za-z0-9_-]+$">
                  <Input
                    placeholder="Endpoint (e.g. echo_01)"
                    value={form.endpoint}
                    onChange={(e) => {
                      setEndpointTouched(true)
                      setForm((f) => ({ ...f, endpoint: e.target.value }))
                    }}
                  />
                  {form.endpoint && !endpointOk ? (
                    <div className="mt-1 text-xs text-[hsl(var(--destructive))]">
                      Endpoint must match ^[A-Za-z0-9_-]+$
                    </div>
                  ) : null}
                </CreateTaskField>
                <CreateTaskField label="Version" className="col-span-2">
                  <Input
                    placeholder="Version"
                    value={form.version}
                    onChange={(e) => setForm((f) => ({ ...f, version: e.target.value }))}
                  />
                </CreateTaskField>
                <CreateTaskField label="Capabilities" className="col-span-2" hint="Comma separated">
                  <Input
                    placeholder="Capabilities (comma separated)"
                    value={form.capabilities}
                    onChange={(e) =>
                      setForm((f) => ({ ...f, capabilities: e.target.value }))
                    }
                  />
                </CreateTaskField>
              </div>
            </CreateTaskSection>

            <CreateTaskSection
              title="Executable"
              description="Choose the artifact type and locate the binary, script, container, or wasm module."
            >
              <div className="grid grid-cols-2 gap-3">
                <CreateTaskField label="Executable Type" className="col-span-2">
                  <select
                    className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
                    value={form.executable_type}
                    onChange={(e) => {
                      const nextType = e.target.value
                      setForm((f) => ({
                        ...f,
                        executable_type: nextType,
                        executable_uri:
                          nextType === 'no-executable'
                            ? ''
                            : f.executable_uri || schemePrefix(scheme),
                      }))
                      setPickerOpen(false)
                    }}
                    data-testid="task-executable-type"
                    aria-label="Executable Type"
                  >
                    <option value="no-executable">no-executable</option>
                    <option value="binary">binary</option>
                    <option value="script">script</option>
                    <option value="container">container</option>
                    <option value="wasm">wasm</option>
                    <option value="process">process</option>
                  </select>
                </CreateTaskField>

                {form.executable_type !== 'no-executable' ? (
                  <>
                    <div className="col-span-2 grid grid-cols-3 gap-2">
                      <CreateTaskField label="Artifact Scheme">
                        <select
                          className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
                          value={scheme}
                          onChange={(e) => {
                            const next = e.target.value as UriScheme
                            setScheme(next)
                            setForm((f) => {
                              const nextPrefix = schemePrefix(next)
                              const knownPrefixes = [
                                schemePrefix('smsfile'),
                                schemePrefix('https'),
                                schemePrefix('s3'),
                                schemePrefix('minio'),
                              ]
                              if (
                                !f.executable_uri ||
                                knownPrefixes.includes(f.executable_uri)
                              ) {
                                return { ...f, executable_uri: nextPrefix }
                              }
                              return f
                            })
                            setPickerOpen(false)
                          }}
                          data-testid="task-uri-scheme"
                          aria-label="Scheme"
                        >
                          <option value="smsfile">smsfile</option>
                          <option value="https">https</option>
                          <option value="s3">s3</option>
                          <option value="minio">minio</option>
                        </select>
                      </CreateTaskField>
                      <CreateTaskField label="Executable URI" className="col-span-2">
                        <Input
                          placeholder="Executable URI"
                          value={form.executable_uri}
                          onChange={(e) =>
                            setForm((f) => ({ ...f, executable_uri: e.target.value }))
                          }
                          data-testid="task-executable-uri"
                        />
                      </CreateTaskField>
                    </div>

                    {scheme === 'smsfile' ? (
                      <div className="col-span-2">
                        <div className="flex items-center justify-between rounded-[var(--radius)] border border-[hsl(var(--border))] bg-[hsl(var(--muted))] px-3 py-2">
                          <div className="text-xs text-[hsl(var(--muted-foreground))]">
                            Pick an embedded file and insert smsfile:// URI
                          </div>
                          <Button
                            variant="secondary"
                            size="sm"
                            onClick={() => setPickerOpen((v) => !v)}
                            data-testid="task-choose-local"
                          >
                            Choose Local
                          </Button>
                        </div>

                        {pickerOpen ? (
                          <div className="mt-2 overflow-hidden rounded-[var(--radius)] border border-[hsl(var(--border))] bg-[hsl(var(--background))]">
                            <div className="border-b border-[hsl(var(--border))] bg-[hsl(var(--background))] p-2">
                              <Input
                                value={fileQ}
                                onChange={(e) => setFileQ(e.target.value)}
                                placeholder="Filter by name or id"
                                data-testid="task-file-filter"
                              />
                            </div>
                            <div className="grid grid-cols-12 border-b border-[hsl(var(--border))] bg-[hsl(var(--muted))] px-3 py-2 text-xs font-medium text-[hsl(var(--muted-foreground))]">
                              <div className="col-span-7">Name / ID</div>
                              <div className="col-span-3">Modified</div>
                              <div className="col-span-2">Action</div>
                            </div>
                            {filesQuery.isLoading ? (
                              <div className="p-3 text-sm text-[hsl(var(--muted-foreground))]">
                                Loading...
                              </div>
                            ) : filesQuery.isError ? (
                              <div className="p-3 text-sm text-[hsl(var(--muted-foreground))]">
                                Failed to load files
                              </div>
                            ) : fileRows.length === 0 ? (
                              <div className="p-3 text-sm text-[hsl(var(--muted-foreground))]">
                                No files
                              </div>
                            ) : (
                              <div className="max-h-[220px] overflow-auto">
                                {fileRows.map((f) => (
                                  <div
                                    key={f.id}
                                    className="grid grid-cols-12 items-center gap-2 border-b border-[hsl(var(--border))] px-3 py-2 text-sm last:border-b-0"
                                  >
                                    <div className="col-span-7 min-w-0">
                                      <div className="truncate font-medium">
                                        {f.name || '(unknown)'}
                                      </div>
                                      <div className="mt-1 truncate text-xs text-[hsl(var(--muted-foreground))]">
                                        {f.id}
                                      </div>
                                    </div>
                                    <div className="col-span-3 text-xs text-[hsl(var(--muted-foreground))]">
                                      {formatTs(f.modified_at)}
                                    </div>
                                    <div className="col-span-2 flex justify-end">
                                      <Button
                                        variant="ghost"
                                        size="sm"
                                        onClick={() => {
                                          setForm((cur) => ({
                                            ...cur,
                                            executable_uri: `smsfile://${f.id}`,
                                            executable_name: f.name || cur.executable_name,
                                          }))
                                          setPickerOpen(false)
                                        }}
                                        data-testid={`task-use-file-${f.id}`}
                                      >
                                        Use
                                      </Button>
                                    </div>
                                  </div>
                                ))}
                                {fileTotal !== null && fileRows.length < fileTotal ? (
                                  <div className="flex items-center justify-end border-t border-[hsl(var(--border))] bg-[hsl(var(--background))] px-3 py-2">
                                    <Button
                                      variant="secondary"
                                      size="sm"
                                      onClick={() => setFileOffset((v) => v + fileLimit)}
                                      disabled={filesQuery.isFetching}
                                      data-testid="task-files-load-more"
                                    >
                                      Load more
                                    </Button>
                                  </div>
                                ) : null}
                              </div>
                            )}
                          </div>
                        ) : null}
                      </div>
                    ) : null}

                    <CreateTaskField label="Executable Name" className="col-span-2" hint="Optional">
                      <Input
                        placeholder="Executable name (optional)"
                        value={form.executable_name}
                        onChange={(e) =>
                          setForm((f) => ({ ...f, executable_name: e.target.value }))
                        }
                      />
                    </CreateTaskField>
                    <CreateTaskField label="Checksum (SHA-256)" className="col-span-2" hint="Optional">
                      <Input
                        placeholder="Checksum sha256 (optional)"
                        value={form.checksum}
                        onChange={(e) =>
                          setForm((f) => ({ ...f, checksum: e.target.value }))
                        }
                      />
                    </CreateTaskField>
                    <CreateTaskField label="Arguments" className="col-span-2" hint="Comma separated">
                      <Input
                        placeholder="Args (comma separated)"
                        value={form.args}
                        onChange={(e) => setForm((f) => ({ ...f, args: e.target.value }))}
                      />
                    </CreateTaskField>
                    <CreateTaskField label="Environment Variables" className="col-span-2" hint="One key=value pair per line">
                      <textarea
                        className="h-24 w-full resize-none rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 py-2 text-sm"
                        placeholder="Env (key=value per line)"
                        value={form.env}
                        onChange={(e) => setForm((f) => ({ ...f, env: e.target.value }))}
                      />
                    </CreateTaskField>
                  </>
                ) : (
                  <div className="col-span-2 rounded-[calc(var(--radius)-4px)] border border-dashed border-[hsl(var(--border))] px-3 py-3 text-sm text-[hsl(var(--muted-foreground))]">
                    No executable selected. This task will rely on control-plane metadata only.
                  </div>
                )}
              </div>
            </CreateTaskSection>
          </div>
        </div>

        <div className="mt-4 flex shrink-0 justify-end gap-2 border-t border-[hsl(var(--border))] pt-3">
          <Button variant="secondary" onClick={() => props.onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            disabled={!canSubmit}
            onClick={async () => {
              try {
                const config: Record<string, string> = {}
                if (form.mcp_enabled) {
                  config['mcp.enabled'] = 'true'
                  if (mcpDefaultIds.length > 0) {
                    config['mcp.default_server_ids'] = toJsonArrayString(mcpDefaultIds)
                  }
                  const allowed = Array.from(new Set([...mcpAllowedIds, ...mcpDefaultIds])).sort()
                  if (
                    allowed.length > 0 &&
                    (allowed.length !== mcpDefaultIds.length ||
                      allowed.some((x, i) => x !== mcpDefaultIds[i]))
                  ) {
                    config['mcp.allowed_server_ids'] = toJsonArrayString(allowed)
                  }
                  const allow = parseCsv(form.mcp_tool_allowlist)
                  const deny = parseCsv(form.mcp_tool_denylist)
                  if (allow.length > 0) config['mcp.tool_allowlist'] = toJsonArrayString(allow)
                  if (deny.length > 0) config['mcp.tool_denylist'] = toJsonArrayString(deny)
                }

                const payload = {
                  name: form.name,
                  description: form.description || undefined,
                  priority: form.priority,
                  endpoint: form.endpoint,
                  version: form.version,
                  desired_replicas: Math.max(1, Number(form.desired_replicas || '1') || 1),
                  scheduling_strategy: form.scheduling_strategy || 'spread',
                  capabilities: parseCsv(form.capabilities),
                  executable:
                    form.executable_type === 'no-executable'
                      ? undefined
                      : {
                          type: form.executable_type,
                          uri: form.executable_uri || undefined,
                          name: form.executable_name || undefined,
                          checksum_sha256: form.checksum || undefined,
                          args: parseCsv(form.args),
                          env: parseEnv(form.env),
                        },
                  config: Object.keys(config).length ? config : undefined,
                }
                const res = await createTask(payload)
                if (!res.success) throw new Error(res.message || 'Create failed')
                toast.success(`Task created: ${res.task_id || ''}`)

                props.onCreated()
                props.onOpenChange(false)
              } catch (e) {
                toast.error((e as Error).message)
              }
            }}
          >
            Create
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

export default function TasksPage() {
  const navigate = useNavigate()
  const [q, setQ] = useState('')
  const [creating, setCreating] = useState(false)
  const [deletingTask, setDeletingTask] = useState<TaskSummary | null>(null)

  const tasksQuery = useQuery({
    queryKey: ['tasks', q],
    queryFn: () => listTasks({ q, sort_by: 'registered_at', order: 'desc', limit: 200 }),
    refetchInterval: 15_000,
  })

  const rows = tasksQuery.data?.tasks || []
  const total = tasksQuery.data?.total_count ?? 0
  const title = useMemo(() => `Tasks (${rows.length}/${total})`, [rows.length, total])

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-semibold">Tasks</div>
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            Observe workload specs, replica targets, and current replica convergence
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="secondary" onClick={() => tasksQuery.refetch()}>
            Refresh
          </Button>
          <Button onClick={() => setCreating(true)} data-testid="tasks-open-create">
            <Plus className="h-4 w-4" />
            Create
          </Button>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{title}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="mb-3 flex items-center gap-2">
            <div className="relative w-full max-w-md">
              <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[hsl(var(--muted-foreground))]" />
              <Input
                value={q}
                onChange={(e) => setQ(e.target.value)}
                placeholder="Search task/name/endpoint"
                className="pl-9"
              />
            </div>
          </div>

          <div className="overflow-hidden rounded-[var(--radius)] border border-[hsl(var(--border))] bg-[hsl(var(--background))]">
            <div className="grid grid-cols-12 border-b border-[hsl(var(--border))] bg-[hsl(var(--muted))] px-3 py-2 text-xs font-medium text-[hsl(var(--muted-foreground))]">
              <div className="col-span-3">Task / Endpoint</div>
              <div className="col-span-2">Status</div>
              <div className="col-span-2">Replica state</div>
              <div className="col-span-2">Replica policy</div>
              <div className="col-span-2">Priority</div>
              <div className="col-span-3">Registered / Actions</div>
            </div>

            {rows.length === 0 ? (
              <div className="p-6 text-sm text-[hsl(var(--muted-foreground))]">
                {tasksQuery.isLoading
                  ? 'Loading...'
                  : tasksQuery.isError
                    ? 'Failed to load tasks'
                    : 'No tasks'}
              </div>
            ) : (
              <div className="max-h-[560px] overflow-auto">
                {rows.map((t) => (
                  <div
                    key={t.task_id}
                    onClick={() => {
                      navigate(`/tasks/${encodeURIComponent(t.task_id)}`)
                    }}
                    className={cn(
                      'grid w-full grid-cols-12 items-center gap-2 px-3 py-2 text-left text-sm hover:bg-[hsl(var(--accent))]',
                      'border-b border-[hsl(var(--border))] last:border-b-0',
                    )}
                    role="button"
                    tabIndex={0}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault()
                        navigate(`/tasks/${encodeURIComponent(t.task_id)}`)
                      }
                    }}
                    data-testid={`task-row-${t.task_id}`}
                  >
                    <div className="col-span-3 min-w-0">
                      <div className="truncate font-medium">{t.name}</div>
                      <div className="mt-1 truncate text-xs text-[hsl(var(--muted-foreground))]">
                        {t.endpoint || t.task_id}
                      </div>
                      <div className="mt-1 truncate font-mono text-[10px] text-[hsl(var(--muted-foreground))]">
                        {t.task_id}
                      </div>
                    </div>
                    <div className="col-span-2">
                      <TaskStatusBadge status={t.status} />
                    </div>
                    <div className="col-span-2">
                      <div className="flex flex-col gap-1">
                        <ReplicaHealthBadge
                          reconciling={t.reconciling}
                          underprovisioned={t.underprovisioned}
                        />
                        <div className="text-xs text-[hsl(var(--muted-foreground))]">
                          active {t.active_instances ?? 0} / ready {t.ready_instances ?? 0}
                        </div>
                      </div>
                    </div>
                    <div className="col-span-2 truncate text-sm text-[hsl(var(--muted-foreground))]">
                      <div>{(t.desired_replicas || 1) + ' x ' + (t.scheduling_strategy || 'spread')}</div>
                      <div className="text-xs">target replicas</div>
                    </div>
                    <div className="col-span-2 text-sm text-[hsl(var(--muted-foreground))]">
                      {t.priority}
                    </div>
                    <div className="col-span-3 flex items-center justify-between gap-2 text-sm text-[hsl(var(--muted-foreground))]">
                      <div className="min-w-0 truncate">{formatTs(t.registered_at)}</div>
                      <div className="flex items-center gap-2">
                        <Button
                          variant="destructive"
                          size="sm"
                          onClick={(e) => {
                            e.preventDefault()
                            e.stopPropagation()
                            setDeletingTask(t)
                          }}
                          data-testid={`task-delete-${t.task_id}`}
                        >
                          Delete
                        </Button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      <CreateTaskDialog
        open={creating}
        onOpenChange={setCreating}
        onCreated={() => tasksQuery.refetch()}
      />
      <DeleteTaskDialog
        open={!!deletingTask}
        onOpenChange={(open) => {
          if (!open) setDeletingTask(null)
        }}
        taskId={deletingTask?.task_id || ''}
        taskName={deletingTask?.name}
        onDeleted={() => {
          setDeletingTask(null)
          void tasksQuery.refetch()
        }}
      />
    </div>
  )
}

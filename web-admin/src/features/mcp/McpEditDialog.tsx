import { useEffect, useState } from 'react'
import { toast } from 'sonner'

import { upsertMcpServer, type McpServer } from '@/api/mcp'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'

type FormState = {
  server_id: string
  display_name: string
  command: string
  args: string
  cwd: string
  env: string
  allowed_tools: string
}

function parseEnvLines(s: string) {
  const out: Record<string, string> = {}
  for (const line of s.split('\n')) {
    const t = line.trim()
    if (!t) continue
    const idx = t.indexOf('=')
    if (idx <= 0) continue
    out[t.slice(0, idx).trim()] = t.slice(idx + 1).trim()
  }
  return out
}

function parseCommaList(s: string) {
  return s
    .split(',')
    .map((x) => x.trim())
    .filter(Boolean)
}

function errMsg(e: unknown) {
  if (e && typeof e === 'object' && 'message' in e) {
    const msg = (e as { message?: unknown }).message
    if (typeof msg === 'string' && msg) return msg
  }
  return 'request failed'
}

function fmtArgs(server: McpServer) {
  return (server.stdio?.args ?? []).join(',')
}

function fmtEnv(server: McpServer) {
  const env = server.stdio?.env ?? {}
  return Object.entries(env)
    .map(([k, v]) => `${k}=${v}`)
    .join('\n')
}

function emptyForm(): FormState {
  return {
    server_id: '',
    display_name: '',
    command: '',
    args: '',
    cwd: '',
    env: '',
    allowed_tools: '',
  }
}

export default function McpEditDialog(props: {
  open: boolean
  onOpenChange: (v: boolean) => void
  initial?: McpServer | null
  onSaved?: () => void
}) {
  const [submitting, setSubmitting] = useState(false)
  const [form, setForm] = useState<FormState>(() => emptyForm())

  useEffect(() => {
    if (!props.open) return
    if (!props.initial) {
      setForm(emptyForm())
      return
    }
    const s = props.initial
    setForm({
      server_id: s.server_id,
      display_name: s.display_name ?? '',
      command: s.stdio?.command ?? '',
      args: fmtArgs(s),
      cwd: s.stdio?.cwd ?? '',
      env: fmtEnv(s),
      allowed_tools: (s.allowed_tools ?? []).join(','),
    })
  }, [props.open, props.initial])

  async function onSubmit() {
    if (!form.server_id.trim()) {
      toast.error('server_id is required')
      return
    }
    if (!form.command.trim()) {
      toast.error('command is required')
      return
    }

    setSubmitting(true)
    try {
      const resp = await upsertMcpServer({
        server_id: form.server_id.trim(),
        display_name: form.display_name.trim() || undefined,
        transport: 'stdio',
        stdio: {
          command: form.command.trim(),
          args: parseCommaList(form.args),
          cwd: form.cwd.trim() || undefined,
          env: parseEnvLines(form.env),
        },
        allowed_tools: parseCommaList(form.allowed_tools),
      })
      if (!resp.success) {
        toast.error(resp.message || 'upsert failed')
        return
      }
      toast.success('Saved')
      props.onOpenChange(false)
      props.onSaved?.()
    } catch (e: unknown) {
      toast.error(errMsg(e) || 'upsert failed')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent>
        <DialogHeader title={form.server_id ? 'Edit MCP Server' : 'Add MCP Server'} />

        <div className="min-h-0 flex-1 overflow-y-auto pr-1">
        <div className="grid grid-cols-2 gap-3">
          <div className="col-span-2">
            <Input
              placeholder="server_id"
              value={form.server_id}
              onChange={(e) => setForm((p) => ({ ...p, server_id: e.target.value }))}
            />
          </div>
          <div className="col-span-2">
            <Input
              placeholder="display_name"
              value={form.display_name}
              onChange={(e) => setForm((p) => ({ ...p, display_name: e.target.value }))}
            />
          </div>
          <div className="col-span-2">
            <Input
              placeholder="stdio.command"
              value={form.command}
              onChange={(e) => setForm((p) => ({ ...p, command: e.target.value }))}
            />
          </div>
          <div className="col-span-2">
            <Input
              placeholder="stdio.args (comma separated)"
              value={form.args}
              onChange={(e) => setForm((p) => ({ ...p, args: e.target.value }))}
            />
          </div>
          <div className="col-span-2">
            <Input
              placeholder="stdio.cwd"
              value={form.cwd}
              onChange={(e) => setForm((p) => ({ ...p, cwd: e.target.value }))}
            />
          </div>
          <div className="col-span-2">
            <textarea
              className="min-h-24 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 py-2 text-sm"
              placeholder="stdio.env (one key=value per line)"
              value={form.env}
              onChange={(e) => setForm((p) => ({ ...p, env: e.target.value }))}
            />
          </div>
          <div className="col-span-2">
            <Input
              placeholder="allowed_tools (comma separated patterns; required)"
              value={form.allowed_tools}
              onChange={(e) => setForm((p) => ({ ...p, allowed_tools: e.target.value }))}
            />
          </div>
        </div>
        </div>

        <div className="mt-4 flex shrink-0 justify-end gap-2 border-t border-[hsl(var(--border))] pt-3">
          <Button variant="secondary" onClick={() => props.onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={submitting}>
            Save
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

import { useMemo, useState } from 'react'
import { toast } from 'sonner'

import { upsertRemoteBackend } from '@/api/remote-backends'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'

type Kind = 'openai_chat_completion' | 'openai_realtime_ws'

type FormState = {
  name: string
  kind: Kind
  base_url: string
  model: string
  credential_ref: string
  operationsText: string
}

function emptyForm(): FormState {
  return {
    name: '',
    kind: 'openai_chat_completion',
    base_url: 'https://api.openai.com/v1',
    model: '',
    credential_ref: '',
    operationsText: 'chat_completions',
  }
}

function splitCsvLike(v: string): string[] {
  return v
    .split(',')
    .map((x) => x.trim())
    .filter(Boolean)
}

export default function CreateRemoteBackendDialog(props: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onCreated: () => void | Promise<void>
}) {
  const [form, setForm] = useState<FormState>(() => emptyForm())

  const handleOpenChange = (open: boolean) => {
    props.onOpenChange(open)
    if (open) setForm(emptyForm())
  }

  const operations = useMemo(() => splitCsvLike(form.operationsText), [form.operationsText])

  const canSubmit = useMemo(() => {
    if (!form.name.trim()) return false
    if (!form.kind.trim()) return false
    if (!form.base_url.trim()) return false
    if (operations.length === 0) return false
    return true
  }, [form.base_url, form.kind, form.name, operations.length])

  const disableReason = useMemo(() => {
    if (!form.name.trim()) return 'Name is required'
    if (!form.base_url.trim()) return 'Base URL is required'
    if (operations.length === 0) return 'Operations are required'
    return ''
  }, [form.base_url, form.name, operations.length])

  return (
    <Dialog open={props.open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader
          title="Create remote backend"
          description="This stores the backend definition in SMS. Restart SPEARlet to apply changes."
        />

        <div className="space-y-3">
          <div className="space-y-1">
            <div className="text-sm font-medium">Name</div>
            <Input
              value={form.name}
              onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
              placeholder="e.g. openai-default"
            />
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Kind</div>
            <select
              className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
              value={form.kind}
              onChange={(e) => {
                const kind = e.target.value as Kind
                setForm((f) => ({
                  ...f,
                  kind,
                  operationsText:
                    kind === 'openai_realtime_ws' ? 'realtime' : 'chat_completions',
                }))
              }}
              aria-label="Kind"
            >
              <option value="openai_chat_completion">openai_chat_completion</option>
              <option value="openai_realtime_ws">openai_realtime_ws</option>
            </select>
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Base URL</div>
            <Input
              value={form.base_url}
              onChange={(e) => setForm((f) => ({ ...f, base_url: e.target.value }))}
              placeholder="e.g. https://api.openai.com/v1"
            />
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Model (optional)</div>
            <Input
              value={form.model}
              onChange={(e) => setForm((f) => ({ ...f, model: e.target.value }))}
              placeholder="e.g. gpt-4o-mini"
            />
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Credential ref (optional)</div>
            <Input
              value={form.credential_ref}
              onChange={(e) => setForm((f) => ({ ...f, credential_ref: e.target.value }))}
              placeholder="e.g. openai_api_key"
            />
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Operations (comma-separated)</div>
            <Input
              value={form.operationsText}
              onChange={(e) => setForm((f) => ({ ...f, operationsText: e.target.value }))}
              placeholder="e.g. chat_completions"
            />
          </div>

          <div className="flex items-center justify-end gap-2 pt-2">
            <Button variant="secondary" onClick={() => handleOpenChange(false)}>
              Cancel
            </Button>
            <Button
              disabled={!canSubmit}
              title={canSubmit ? '' : disableReason}
              onClick={async () => {
                try {
                  const resp = await upsertRemoteBackend({
                    name: form.name.trim(),
                    kind: form.kind,
                    base_url: form.base_url.trim(),
                    model: form.model.trim() ? form.model.trim() : undefined,
                    credential_ref: form.credential_ref.trim()
                      ? form.credential_ref.trim()
                      : undefined,
                    operations,
                    transports:
                      form.kind === 'openai_realtime_ws' ? ['websocket'] : ['http'],
                  })
                  if (!resp.success) throw new Error(resp.message || 'Create failed')
                  toast.success('Remote backend saved. Restart SPEARlet to apply.')
                  await props.onCreated()
                  handleOpenChange(false)
                } catch (e) {
                  toast.error((e as Error).message)
                }
              }}
            >
              Save
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

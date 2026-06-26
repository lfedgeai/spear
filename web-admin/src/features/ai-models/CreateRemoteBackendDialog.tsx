import { useEffect, useMemo, useState } from 'react'
import { toast } from 'sonner'

import { listCredentials, type CredentialInfo } from '@/api/credentials'
import { upsertRemoteBackend } from '@/api/remote-backends'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'

type Kind = 'openai_chat_completion' | 'openai_realtime_ws'
type Operation = 'chat_completions' | 'speech_to_text'

type OperationOption = {
  value: Operation
  label: string
  description: string
}

type FormState = {
  name: string
  kind: Kind
  base_url: string
  model: string
  credential_ref: string
  operations: Operation[]
}

const KIND_OPERATION_OPTIONS: Record<Kind, OperationOption[]> = {
  openai_chat_completion: [
    {
      value: 'chat_completions',
      label: 'chat_completions',
      description: 'Standard chat completion requests over HTTP.',
    },
  ],
  openai_realtime_ws: [
    {
      value: 'speech_to_text',
      label: 'speech_to_text',
      description: 'Streaming speech-to-text over realtime websocket.',
    },
  ],
}

function defaultOperations(kind: Kind): Operation[] {
  return KIND_OPERATION_OPTIONS[kind].map((option) => option.value)
}

function emptyForm(): FormState {
  return {
    name: '',
    kind: 'openai_chat_completion',
    base_url: 'https://api.openai.com/v1',
    model: '',
    credential_ref: '',
    operations: defaultOperations('openai_chat_completion'),
  }
}

export default function CreateRemoteBackendDialog(props: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onCreated: () => void | Promise<void>
}) {
  const [form, setForm] = useState<FormState>(() => emptyForm())
  const [credentials, setCredentials] = useState<CredentialInfo[]>([])

  const handleOpenChange = (open: boolean) => {
    props.onOpenChange(open)
    if (open) setForm(emptyForm())
  }

  useEffect(() => {
    if (!props.open) return
    let cancelled = false
    void listCredentials()
      .then((resp) => {
        if (cancelled) return
        if (!resp.success) throw new Error(resp.message || 'Failed to load credentials')
        setCredentials(resp.credentials || [])
      })
      .catch((e) => {
        if (cancelled) return
        setCredentials([])
        toast.error((e as Error).message)
      })
    return () => {
      cancelled = true
    }
  }, [props.open])

  const operationOptions = useMemo(() => KIND_OPERATION_OPTIONS[form.kind], [form.kind])

  const canSubmit = useMemo(() => {
    if (!form.name.trim()) return false
    if (!form.kind.trim()) return false
    if (!form.base_url.trim()) return false
    if (form.operations.length === 0) return false
    return true
  }, [form.base_url, form.kind, form.name, form.operations.length])

  const disableReason = useMemo(() => {
    if (!form.name.trim()) return 'Name is required'
    if (!form.base_url.trim()) return 'Base URL is required'
    if (form.operations.length === 0) return 'At least one operation is required'
    return ''
  }, [form.base_url, form.name, form.operations.length])

  const toggleOperation = (operation: Operation, checked: boolean) => {
    setForm((current) => {
      const nextOperations = checked
        ? Array.from(new Set([...current.operations, operation]))
        : current.operations.filter((value) => value !== operation)
      return {
        ...current,
        operations: nextOperations,
      }
    })
  }

  return (
    <Dialog open={props.open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader
          title="Create remote backend"
          description="This stores the backend definition in SMS. SPEARlet will pick it up automatically (may take up to ~15s)."
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
                  operations: defaultOperations(kind),
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
            <select
              className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
              value={form.credential_ref}
              onChange={(e) => setForm((f) => ({ ...f, credential_ref: e.target.value }))}
              aria-label="Credential ref"
            >
              <option value="">None</option>
              {credentials.map((credential) => (
                <option key={credential.name} value={credential.name} disabled={credential.disabled}>
                  {credential.name}
                  {credential.disabled ? ' (disabled)' : ''}
                </option>
              ))}
            </select>
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Operations</div>
            <div className="space-y-2 rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] p-3">
              {operationOptions.map((option) => {
                const checked = form.operations.includes(option.value)
                return (
                  <label key={option.value} className="flex items-start gap-3 text-sm">
                    <input
                      type="checkbox"
                      className="mt-1"
                      checked={checked}
                      onChange={(e) => toggleOperation(option.value, e.target.checked)}
                    />
                    <span className="space-y-1">
                      <span className="block font-medium">{option.label}</span>
                      <span className="block text-xs text-[hsl(var(--muted-foreground))]">
                        {option.description}
                      </span>
                    </span>
                  </label>
                )
              })}
            </div>
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
                    operations: form.operations,
                    transports:
                      form.kind === 'openai_realtime_ws' ? ['websocket'] : ['http'],
                  })
                  if (!resp.success) throw new Error(resp.message || 'Create failed')
                  toast.success('Remote backend saved. SPEARlet will pick it up automatically.')
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

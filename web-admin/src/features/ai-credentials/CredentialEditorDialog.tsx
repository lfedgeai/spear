import { useEffect, useMemo, useState } from 'react'
import { toast } from 'sonner'

import { upsertCredential, type CredentialInfo } from '@/api/credentials'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'

type Props = {
  open: boolean
  mode: 'create' | 'edit'
  credential?: CredentialInfo | null
  onOpenChange: (open: boolean) => void
  onSaved: () => void | Promise<void>
}

type FormState = {
  name: string
  secret: string
  description: string
  disabled: boolean
}

function buildFormState(mode: Props['mode'], credential?: CredentialInfo | null): FormState {
  if (mode === 'edit' && credential) {
    return {
      name: credential.name,
      secret: '',
      description: credential.description || '',
      disabled: credential.disabled,
    }
  }

  return {
    name: '',
    secret: '',
    description: '',
    disabled: false,
  }
}

export default function CredentialEditorDialog(props: Props) {
  const [form, setForm] = useState<FormState>(() => buildFormState(props.mode, props.credential))
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (!props.open) return
    setForm(buildFormState(props.mode, props.credential))
  }, [props.credential, props.mode, props.open])

  const canSubmit = useMemo(() => {
    if (!form.name.trim()) return false
    if (props.mode === 'create' && !form.secret.trim()) return false
    return true
  }, [form.name, form.secret, props.mode])

  const disableReason = useMemo(() => {
    if (!form.name.trim()) return 'Credential name is required'
    if (props.mode === 'create' && !form.secret.trim()) return 'Secret is required for new credentials'
    return ''
  }, [form.name, form.secret, props.mode])

  const title = props.mode === 'create' ? 'Create credential' : `Edit ${props.credential?.name || 'credential'}`
  const description =
    props.mode === 'create'
      ? 'Create a reusable credential reference for AI backends.'
      : 'Update metadata, disabled status, or optionally rotate the secret for this credential.'

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="w-[min(640px,calc(100vw-24px))]">
        <DialogHeader title={title} description={description} />

        <div className="min-h-0 flex-1 overflow-y-auto pr-1">
        <div className="space-y-4">
          <div className="space-y-1">
            <div className="text-sm font-medium">Name</div>
            <Input
              value={form.name}
              disabled={props.mode === 'edit'}
              onChange={(e) => setForm((current) => ({ ...current, name: e.target.value }))}
              placeholder="e.g. openai-default"
            />
            {props.mode === 'edit' ? (
              <div className="text-xs text-[hsl(var(--muted-foreground))]">
                Rename is not supported. Create a new credential if you need a different ref name.
              </div>
            ) : null}
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">
              Secret {props.mode === 'create' ? '' : '(optional)'}
            </div>
            <Input
              value={form.secret}
              type="password"
              onChange={(e) => setForm((current) => ({ ...current, secret: e.target.value }))}
              placeholder={props.mode === 'create' ? 'sk-...' : 'Leave blank to keep current secret'}
            />
            <div className="text-xs text-[hsl(var(--muted-foreground))]">
              {props.mode === 'create'
                ? 'The plaintext secret is write-only and is encrypted before SMS stores it.'
                : 'Provide a new secret only when you want to rotate the current credential.'}
            </div>
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Description</div>
            <Input
              value={form.description}
              onChange={(e) =>
                setForm((current) => ({ ...current, description: e.target.value }))
              }
              placeholder="Optional operator-facing note"
            />
          </div>

          <label className="flex items-center gap-3 rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--border))] px-3 py-2 text-sm">
            <input
              type="checkbox"
              checked={form.disabled}
              onChange={(e) =>
                setForm((current) => ({ ...current, disabled: e.target.checked }))
              }
            />
            <span>
              Mark credential as disabled
              <span className="block text-xs text-[hsl(var(--muted-foreground))]">
                Disabled credentials remain stored but should not be selected for new backends.
              </span>
            </span>
          </label>

        </div>
        </div>
        <div className="mt-4 flex shrink-0 items-center justify-end gap-2 border-t border-[hsl(var(--border))] pt-3">
          <Button variant="secondary" onClick={() => props.onOpenChange(false)} disabled={saving}>
            Cancel
          </Button>
          <Button
            disabled={!canSubmit || saving}
            title={canSubmit ? '' : disableReason}
            onClick={async () => {
              setSaving(true)
              try {
                const resp = await upsertCredential({
                  name: form.name.trim(),
                  secret: form.secret.trim() ? form.secret.trim() : undefined,
                  description: form.description.trim() || undefined,
                  disabled: form.disabled,
                })
                if (!resp.success) throw new Error(resp.message || 'Save failed')
                toast.success(
                  props.mode === 'create' ? 'Credential created' : 'Credential updated',
                )
                await props.onSaved()
                props.onOpenChange(false)
              } catch (error) {
                toast.error((error as Error).message)
              } finally {
                setSaving(false)
              }
            }}
          >
            {saving ? 'Saving...' : 'Save'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

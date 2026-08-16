/**
 * Placement editor dialog for unified AI backends.
 * 统一 AI backend 的 placement 编辑对话框。
 */

import { useEffect, useState } from 'react'
import { toast } from 'sonner'

import type { AiBackendPlacement, WriteAiBackendPlacementInput } from '@/api/ai-backends'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'

type FormState = {
  placement_id: string
  node_uuid: string
  desired_state: 'enabled' | 'disabled'
  weight_override: string
  priority_override: string
}

function formFromPlacement(placement?: AiBackendPlacement | null): FormState {
  return {
    placement_id: placement?.placement_id || '',
    node_uuid: placement?.node_uuid || '',
    desired_state: placement?.desired_state === 'disabled' ? 'disabled' : 'enabled',
    weight_override:
      placement?.weight_override === null || placement?.weight_override === undefined
        ? ''
        : String(placement.weight_override),
    priority_override:
      placement?.priority_override === null || placement?.priority_override === undefined
        ? ''
        : String(placement.priority_override),
  }
}

export default function PlacementEditorDialog(props: {
  open: boolean
  backendId: string
  placement?: AiBackendPlacement | null
  onOpenChange: (open: boolean) => void
  onSubmit: (input: WriteAiBackendPlacementInput) => Promise<void>
}) {
  const [form, setForm] = useState<FormState>(() => formFromPlacement(props.placement))

  useEffect(() => {
    if (!props.open) return
    setForm(formFromPlacement(props.placement))
  }, [props.open, props.placement])

  const canSubmit = !!props.backendId.trim() && !!form.node_uuid.trim()

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent>
        <DialogHeader
          title={props.placement ? 'Edit placement' : 'Add placement'}
          description="Placements define which nodes should enable this backend."
        />

        <div className="space-y-3">
          <div className="space-y-1">
            <div className="text-sm font-medium">Node UUID</div>
            <Input
              value={form.node_uuid}
              onChange={(event) => setForm((current) => ({ ...current, node_uuid: event.target.value }))}
              placeholder="e.g. node-123"
            />
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Desired state</div>
            <select
              className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
              value={form.desired_state}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  desired_state: event.target.value as 'enabled' | 'disabled',
                }))
              }
            >
              <option value="enabled">enabled</option>
              <option value="disabled">disabled</option>
            </select>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <div className="text-sm font-medium">Weight override</div>
              <Input
                value={form.weight_override}
                onChange={(event) =>
                  setForm((current) => ({ ...current, weight_override: event.target.value }))
                }
                placeholder="Optional"
              />
            </div>
            <div className="space-y-1">
              <div className="text-sm font-medium">Priority override</div>
              <Input
                value={form.priority_override}
                onChange={(event) =>
                  setForm((current) => ({ ...current, priority_override: event.target.value }))
                }
                placeholder="Optional"
              />
            </div>
          </div>

          <div className="flex items-center justify-end gap-2 pt-2">
            <Button variant="secondary" onClick={() => props.onOpenChange(false)}>
              Cancel
            </Button>
            <Button
              disabled={!canSubmit}
              onClick={async () => {
                try {
                  await props.onSubmit({
                    placement_id: form.placement_id.trim() || undefined,
                    backend_id: props.backendId,
                    node_uuid: form.node_uuid.trim(),
                    desired_state: form.desired_state,
                    weight_override: form.weight_override.trim()
                      ? Number(form.weight_override)
                      : undefined,
                    priority_override: form.priority_override.trim()
                      ? Number(form.priority_override)
                      : undefined,
                  })
                  props.onOpenChange(false)
                } catch (error) {
                  toast.error((error as Error).message)
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

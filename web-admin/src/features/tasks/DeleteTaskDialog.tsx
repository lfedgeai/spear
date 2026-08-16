import { useEffect, useState } from 'react'
import { toast } from 'sonner'

import { deleteTask } from '@/api/tasks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'

type DeleteTaskDialogProps = {
  /** Whether the dialog is open / 对话框是否打开 */
  open: boolean
  /** Open state callback / 打开状态回调 */
  onOpenChange: (open: boolean) => void
  /** Target task id / 目标任务 ID */
  taskId: string
  /** Optional task name / 可选任务名称 */
  taskName?: string
  /** Called after delete request is accepted / 删除请求成功后的回调 */
  onDeleted?: () => void
}

/**
 * Confirm and submit a coordinated task delete request.
 * 确认并提交协同 task 删除请求。
 */
export default function DeleteTaskDialog(props: DeleteTaskDialogProps) {
  const [reason, setReason] = useState('')
  const [force, setForce] = useState(true)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    if (!props.open) {
      setReason('')
      setForce(true)
      setSubmitting(false)
      setError('')
    }
  }, [props.open])

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="w-[min(560px,calc(100vw-24px))]">
        <DialogHeader
          title="Delete task"
          description="This requests coordinated runtime cleanup and then removes the task from SMS."
        />
        <div className="space-y-3">
          <div className="grid grid-cols-1 gap-3 text-sm sm:grid-cols-2">
            <div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">Task ID</div>
              <div className="font-mono text-xs">{props.taskId || '-'}</div>
            </div>
            <div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">Task name</div>
              <div className="truncate text-sm">{props.taskName || '-'}</div>
            </div>
          </div>

          <div className="space-y-1">
            <div className="text-xs text-[hsl(var(--muted-foreground))]">Reason (optional)</div>
            <Input
              value={reason}
              onChange={(event) => setReason(event.target.value)}
              placeholder="Reason for deletion"
            />
          </div>

          <label className="flex items-start gap-3 rounded-[var(--radius)] border border-[hsl(var(--border))] p-3 text-sm">
            <input
              type="checkbox"
              checked={force}
              onChange={(event) => setForce(event.target.checked)}
              className="mt-0.5"
            />
            <div>
              <div className="font-medium">Force runtime cleanup</div>
              <div className="text-xs text-[hsl(var(--muted-foreground))]">
                Stops instances that still belong to this task before final deletion.
              </div>
            </div>
          </label>

          {error ? <div className="text-sm text-[hsl(var(--destructive))]">{error}</div> : null}

          <div className="flex justify-end gap-2">
            <Button
              variant="secondary"
              onClick={() => props.onOpenChange(false)}
              disabled={submitting}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              disabled={submitting || !props.taskId}
              onClick={async () => {
                if (!props.taskId) return
                setSubmitting(true)
                setError('')
                try {
                  const resp = await deleteTask({
                    task_id: props.taskId,
                    reason: reason.trim() ? reason.trim() : undefined,
                    force,
                  })
                  if (!resp.success) {
                    setError(resp.message || 'Delete task failed')
                    return
                  }
                  toast.success(resp.message || 'Task deletion requested')
                  props.onOpenChange(false)
                  props.onDeleted?.()
                } catch (err) {
                  setError(String((err as { message?: string } | null)?.message || err || 'Delete task failed'))
                } finally {
                  setSubmitting(false)
                }
              }}
            >
              {submitting ? 'Deleting…' : 'Delete'}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

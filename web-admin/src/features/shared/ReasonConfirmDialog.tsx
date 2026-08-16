import type { ReactNode } from 'react'

import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'

type DetailRow = {
  label: string
  value: ReactNode
}

type ReasonConfirmDialogProps = {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  description: string
  details?: DetailRow[]
  reason: string
  onReasonChange: (value: string) => void
  reasonPlaceholder?: string
  error?: string
  confirmLabel: string
  confirmingLabel: string
  confirmDisabled?: boolean
  confirming?: boolean
  onConfirm: () => void | Promise<void>
  children?: ReactNode
}

export function ReasonConfirmDialog(props: ReasonConfirmDialogProps) {
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="w-[min(520px,calc(100vw-24px))]">
        <DialogHeader title={props.title} description={props.description} />
        <div className="space-y-3">
          {props.details?.length ? (
            <div className="grid grid-cols-2 gap-3 text-sm">
              {props.details.map((detail) => (
                <div key={detail.label}>
                  <div className="text-xs text-[hsl(var(--muted-foreground))]">{detail.label}</div>
                  <div className="font-mono text-xs">{detail.value}</div>
                </div>
              ))}
            </div>
          ) : null}
          {props.children}
          <div className="space-y-1">
            <div className="text-xs text-[hsl(var(--muted-foreground))]">Reason (optional)</div>
            <Input
              value={props.reason}
              onChange={(event) => props.onReasonChange(event.target.value)}
              placeholder={props.reasonPlaceholder || 'Reason'}
            />
          </div>
          {props.error ? (
            <div className="text-sm text-[hsl(var(--destructive))]">{props.error}</div>
          ) : null}
          <div className="flex justify-end gap-2">
            <Button
              variant="secondary"
              onClick={() => props.onOpenChange(false)}
              disabled={props.confirming}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              disabled={props.confirming || props.confirmDisabled}
              onClick={props.onConfirm}
            >
              {props.confirming ? props.confirmingLabel : props.confirmLabel}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

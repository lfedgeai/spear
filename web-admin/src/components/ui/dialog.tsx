import * as DialogPrimitive from '@radix-ui/react-dialog'

import { cn } from '@/lib/utils'

export const Dialog = DialogPrimitive.Root

export function DialogContent(
  props: DialogPrimitive.DialogContentProps & { className?: string },
) {
  const { className, ...rest } = props
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="fixed inset-0 bg-black/50" />
      <DialogPrimitive.Content
        className={cn(
          'fixed left-1/2 top-1/2 flex max-h-[calc(100vh-24px)] w-[min(820px,calc(100vw-24px))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-y-auto rounded-[var(--radius)] border border-[hsl(var(--border))] bg-[hsl(var(--background))] p-5 text-[hsl(var(--foreground))] shadow-xl outline-none',
          className,
        )}
        {...rest}
      />
    </DialogPrimitive.Portal>
  )
}

export function DialogHeader(props: { title: string; description?: string }) {
  return (
    <div className="sticky top-0 z-10 -mx-5 -mt-5 mb-4 border-b border-[hsl(var(--border))] bg-[hsl(var(--background))] px-5 pb-4 pt-5">
      <DialogPrimitive.Title className="text-base font-semibold">
        {props.title}
      </DialogPrimitive.Title>
      {props.description ? (
        <DialogPrimitive.Description className="mt-1 text-sm text-[hsl(var(--muted-foreground))]">
          {props.description}
        </DialogPrimitive.Description>
      ) : null}
    </div>
  )
}

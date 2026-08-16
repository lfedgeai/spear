import * as React from 'react'

import { cn } from '../../lib/utils'

type ButtonVariant = 'default' | 'secondary' | 'destructive' | 'ghost' | 'outline'
type ButtonSize = 'default' | 'sm' | 'icon'

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant
  size?: ButtonSize
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = 'default', size = 'default', type = 'button', ...props }, ref) => {
    return (
      <button
        ref={ref}
        type={type}
        className={cn(
          'inline-flex items-center justify-center whitespace-nowrap rounded-md border text-sm font-medium transition-colors',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2',
          'ring-offset-[hsl(var(--background))] disabled:pointer-events-none disabled:opacity-50',
          variant === 'default' &&
            'border-[hsl(var(--primary))] bg-[hsl(var(--primary))] text-[hsl(var(--primary-foreground))] hover:opacity-95',
          variant === 'secondary' &&
            'border-[hsl(var(--border))] bg-[hsl(var(--secondary))] text-[hsl(var(--secondary-foreground))] hover:bg-[hsl(var(--accent))]',
          variant === 'destructive' &&
            'border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))] text-[hsl(var(--destructive-foreground))] hover:opacity-95',
          variant === 'outline' &&
            'border-[hsl(var(--border))] bg-[hsl(var(--background))] text-[hsl(var(--foreground))] hover:bg-[hsl(var(--accent))]',
          variant === 'ghost' &&
            'border-transparent bg-transparent text-[hsl(var(--foreground))] hover:bg-[hsl(var(--accent))]',
          size === 'default' && 'h-9 gap-2 px-4',
          size === 'sm' && 'h-8 gap-1.5 px-3',
          size === 'icon' && 'h-9 w-9',
          className,
        )}
        {...props}
      />
    )
  },
)

Button.displayName = 'Button'

import type { ComponentProps } from "react"

import { cn } from "@agent-factory/ui/lib/utils"

export function Shimmer({ className, ...props }: ComponentProps<"span">) {
  return (
    <span
      className={cn(
        "inline-block bg-[length:250%_100%] bg-clip-text text-transparent [background-image:linear-gradient(90deg,transparent,var(--foreground),transparent),linear-gradient(var(--muted-foreground),var(--muted-foreground))] [background-repeat:no-repeat,padding-box] animate-shimmer",
        className,
      )}
      {...props}
    />
  )
}

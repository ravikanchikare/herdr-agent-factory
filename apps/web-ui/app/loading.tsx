import { Skeleton } from "@agent-factory/ui/components/skeleton"

export default function Loading() {
  return (
    <main
      aria-label="Loading Agent Factory"
      className="flex h-svh flex-col gap-4 p-4"
    >
      <Skeleton className="h-8 w-48" />
      <div className="flex min-h-0 flex-1 gap-4">
        <Skeleton className="h-full w-64" />
        <Skeleton className="h-full flex-1" />
      </div>
    </main>
  )
}

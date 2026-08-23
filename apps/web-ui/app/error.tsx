"use client"

import { AlertCircleIcon, RotateCcwIcon } from "lucide-react"

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@agent-factory/ui/components/alert"
import { Button } from "@agent-factory/ui/components/button"

export default function ErrorBoundary({ reset }: { reset: () => void }) {
  return (
    <main className="flex h-svh items-center justify-center p-6">
      <Alert variant="destructive" className="max-w-lg">
        <AlertCircleIcon aria-hidden="true" />
        <AlertTitle>Agent Factory could not render</AlertTitle>
        <AlertDescription>
          The workspace UI encountered an unexpected presentation error.
        </AlertDescription>
        <Button variant="outline" onClick={reset}>
          <RotateCcwIcon data-icon="inline-start" />
          Try again
        </Button>
      </Alert>
    </main>
  )
}

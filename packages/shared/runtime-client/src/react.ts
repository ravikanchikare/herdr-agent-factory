"use client"

import * as React from "react"

import type { RuntimeClient } from "./client"

export function useRuntimeProjection(client: RuntimeClient) {
  return React.useSyncExternalStore(
    client.subscribe,
    client.getSnapshot,
    client.getSnapshot,
  )
}

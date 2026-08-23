import type { RuntimeClient } from "./client"
import type {
  NotificationRequestedDto,
  RuntimeIntent,
  WorkspaceProjection,
} from "./contracts"

export class TestRuntimeClient implements RuntimeClient {
  readonly intents: RuntimeIntent[] = []
  private readonly listeners = new Set<() => void>()
  private nativeTerminalVisible = false
  private readonly nativeTerminalVisibilityListeners = new Set<() => void>()
  private readonly notificationListeners = new Set<
    (notification: NotificationRequestedDto) => void
  >()

  constructor(private projection: WorkspaceProjection) {}

  connect = async () => undefined

  disconnect = () => undefined

  dispatch = async (intent: RuntimeIntent) => {
    this.intents.push(intent)
  }

  listVersionFiles = async () => {
    throw new Error("Version file listing is not configured for this test client.")
  }

  readVersionFile = async () => {
    throw new Error("Version file reading is not configured for this test client.")
  }

  readAgentTranscript = async () => {
    throw new Error("Agent transcript reading is not configured for this test client.")
  }

  readAgentScreen = async () => {
    throw new Error("Agent screen reading is not configured for this test client.")
  }

  writeAgentInput = async () => {
    throw new Error("Agent input is not configured for this test client.")
  }

  getSnapshot = () => this.projection

  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  getNativeTerminalVisibility = () => this.nativeTerminalVisible

  subscribeNativeTerminalVisibility = (listener: () => void) => {
    this.nativeTerminalVisibilityListeners.add(listener)
    return () => this.nativeTerminalVisibilityListeners.delete(listener)
  }

  subscribeNotifications = (
    listener: (notification: NotificationRequestedDto) => void,
  ) => {
    this.notificationListeners.add(listener)
    return () => this.notificationListeners.delete(listener)
  }

  replaceProjection(projection: WorkspaceProjection) {
    this.projection = projection
    this.listeners.forEach((listener) => listener())
  }

  setNativeTerminalVisibility(visible: boolean) {
    if (this.nativeTerminalVisible === visible) return
    this.nativeTerminalVisible = visible
    this.nativeTerminalVisibilityListeners.forEach((listener) => listener())
  }

  emitNotification(notification: NotificationRequestedDto) {
    this.notificationListeners.forEach((listener) => listener(notification))
  }
}

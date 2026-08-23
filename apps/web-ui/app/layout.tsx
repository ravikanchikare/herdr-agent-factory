import type { Metadata } from "next"

import { TooltipProvider } from "@agent-factory/ui/components/tooltip"
import { Toaster } from "@agent-factory/ui/components/toast"

import "./globals.css"

export const metadata: Metadata = {
  title: "Agent Factory",
  description: "A Herdr-driven agent factory.",
}

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body>
        <TooltipProvider>
          <Toaster>{children}</Toaster>
        </TooltipProvider>
      </body>
    </html>
  )
}

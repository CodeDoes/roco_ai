"use client";

import * as React from "react";
import * as ResizablePrimitive from "@radix-ui/react-resizable-panel";
import { cn } from "@/lib/utils";

const PanelGroup = React.forwardRef<
  React.ElementRef<typeof ResizablePrimitive.Group>,
  React.ComponentPropsWithoutRef<typeof ResizablePrimitive.Group>
>(({ className, ...props }, ref) => (
  <ResizablePrimitive.Group
    ref={ref}
    className={cn("flex h-full w-full", className)}
    {...props}
  />
));
PanelGroup.displayName = "PanelGroup";

const Panel = React.forwardRef<
  React.ElementRef<typeof ResizablePrimitive.Panel>,
  React.ComponentPropsWithoutRef<typeof ResizablePrimitive.Panel>
>(({ className, ...props }, ref) => (
  <ResizablePrimitive.Panel
    ref={ref}
    className={cn("h-full", className)}
    {...props}
  />
));
Panel.displayName = "Panel";

const PanelResizeHandle = React.forwardRef<
  React.ElementRef<typeof ResizablePrimitive.Handle>,
  React.ComponentPropsWithoutRef<typeof ResizablePrimitive.Handle>
>(({ className, ...props }, ref) => (
  <ResizablePrimitive.Handle
    ref={ref}
    className={cn(
      "relative flex w-px items-center justify-center bg-border after:absolute after:inset-y-0 after:left-1/2 after:w-1 after:-translate-x-1/2 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-offset-1 data-[panel-group-direction=vertical]:h-px data-[panel-group-direction=vertical]:w-full data-[panel-group-direction=vertical]:after:left-0 data-[panel-group-direction=vertical]:after:h-1 data-[panel-group-direction=vertical]:after:w-full data-[panel-group-direction=vertical]:after:-translate-y-1/2 data-[panel-group-direction=vertical]:after:translate-x-0 [&[data-panel-group-direction=vertical]>div]:rotate-90",
      className
    )}
    {...props}
  >
    <div className="z-10 flex h-4 w-3 items-center justify-center rounded-sm border bg-border" />
  </ResizablePrimitive.Handle>
));
PanelResizeHandle.displayName = "PanelResizeHandle";

export { PanelGroup, Panel, PanelResizeHandle };

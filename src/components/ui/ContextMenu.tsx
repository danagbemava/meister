import { forwardRef } from "react";
import * as Rx from "@radix-ui/react-context-menu";
import { cn } from "../../lib/cn";

type ContentProps = React.ComponentPropsWithoutRef<typeof Rx.Content>;

const Content = forwardRef<HTMLDivElement, ContentProps>(function ContextMenuContent(
  { className, children, ...rest },
  ref,
) {
  return (
    <Rx.Portal>
      <Rx.Content
        ref={ref}
        collisionPadding={8}
        className={cn(
          "z-50 min-w-[10rem] overflow-hidden rounded-card glass-overlay p-1",
          "data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95",
          "data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95",
          className,
        )}
        {...rest}
      >
        {children}
      </Rx.Content>
    </Rx.Portal>
  );
});

type ItemProps = React.ComponentPropsWithoutRef<typeof Rx.Item> & {
  variant?: "default" | "destructive";
};

const Item = forwardRef<HTMLDivElement, ItemProps>(function ContextMenuItem(
  { className, variant = "default", ...rest },
  ref,
) {
  return (
    <Rx.Item
      ref={ref}
      className={cn(
        "flex h-7 select-none items-center rounded-field px-3 text-body outline-none cursor-default",
        "data-[disabled]:opacity-50 data-[disabled]:pointer-events-none",
        variant === "default" &&
          "text-label-primary data-[highlighted]:bg-accent data-[highlighted]:text-accent-label",
        variant === "destructive" &&
          "text-status-error data-[highlighted]:bg-status-error data-[highlighted]:text-white",
        className,
      )}
      {...rest}
    />
  );
});

type SeparatorProps = React.ComponentPropsWithoutRef<typeof Rx.Separator>;

const Separator = forwardRef<HTMLDivElement, SeparatorProps>(function ContextMenuSeparator(
  { className, ...rest },
  ref,
) {
  return (
    <Rx.Separator
      ref={ref}
      className={cn("my-1 h-px bg-separator", className)}
      {...rest}
    />
  );
});

export const ContextMenu = Object.assign(Rx.Root, {
  Trigger: Rx.Trigger,
  Content,
  Item,
  Separator,
});

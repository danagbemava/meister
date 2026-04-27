import { createContext, forwardRef, useContext } from "react";
import * as ToggleGroup from "@radix-ui/react-toggle-group";
import { cn } from "../../lib/cn";
import { type Size, focusClasses } from "./_shared";

type RootProps = {
  value: string;
  onValueChange: (value: string) => void;
  size?: Size;
  className?: string;
  children: React.ReactNode;
  "aria-label"?: string;
};

const SizeContext = createContext<Size>("md");

const containerHeight: Record<Size, string> = {
  sm: "h-6",
  md: "h-7",
  lg: "h-8",
};

const itemHeight: Record<Size, string> = {
  sm: "h-5",
  md: "h-6",
  lg: "h-7",
};

const itemPad: Record<Size, string> = {
  sm: "px-2",
  md: "px-3",
  lg: "px-4",
};

const itemFont: Record<Size, string> = {
  sm: "text-callout",
  md: "text-body",
  lg: "text-headline",
};

const Root = forwardRef<HTMLDivElement, RootProps>(function SegmentedControlRoot(
  { value, onValueChange, size = "md", className, children, "aria-label": ariaLabel },
  ref,
) {
  return (
    <SizeContext.Provider value={size}>
      <ToggleGroup.Root
        ref={ref}
        type="single"
        value={value}
        onValueChange={(v) => {
          // ToggleGroup with type="single" allows empty string when user
          // toggles the active item off. We don't want that — keep the
          // current selection if Radix tries to clear it.
          if (v) onValueChange(v);
        }}
        aria-label={ariaLabel}
        className={cn(
          "inline-flex items-center bg-fill-tertiary rounded-md p-0.5 gap-0.5",
          containerHeight[size],
          className,
        )}
      >
        {children}
      </ToggleGroup.Root>
    </SizeContext.Provider>
  );
});

type ItemProps = {
  value: string;
  children: React.ReactNode;
  className?: string;
  disabled?: boolean;
};

const Item = forwardRef<HTMLButtonElement, ItemProps>(function SegmentedControlItem(
  { value, children, className, disabled },
  ref,
) {
  const size = useContext(SizeContext);
  return (
    <ToggleGroup.Item
      ref={ref}
      value={value}
      disabled={disabled}
      className={cn(
        "inline-flex items-center justify-center rounded-sm font-medium transition-colors",
        "text-label-secondary hover:text-label-primary",
        "data-[state=on]:bg-control-background data-[state=on]:text-label-primary data-[state=on]:shadow-[0_0_0_0.5px_rgb(0_0_0/0.04)]",
        "disabled:opacity-50 disabled:cursor-not-allowed",
        itemHeight[size],
        itemPad[size],
        itemFont[size],
        focusClasses,
        className,
      )}
    >
      {children}
    </ToggleGroup.Item>
  );
});

export const SegmentedControl = Object.assign(Root, { Item });

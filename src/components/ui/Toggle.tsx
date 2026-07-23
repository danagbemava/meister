import { forwardRef } from "react";
import * as Switch from "@radix-ui/react-switch";
import { cn } from "../../lib/cn";
import { Size, focusClasses } from "./_shared";

type Props = React.ComponentPropsWithoutRef<typeof Switch.Root> & {
  size?: Size;
};

const trackSize: Record<Size, string> = {
  sm: "h-4 w-7",
  md: "h-5 w-9",
  lg: "h-6 w-11",
};

const thumbSize: Record<Size, string> = {
  sm: "h-3 w-3 translate-x-0.5 data-[state=checked]:translate-x-3.5",
  md: "h-4 w-4 translate-x-0.5 data-[state=checked]:translate-x-4",
  lg: "h-5 w-5 translate-x-0.5 data-[state=checked]:translate-x-5",
};

export const Toggle = forwardRef<
  React.ElementRef<typeof Switch.Root>,
  Props
>(function Toggle({ size = "md", className, ...rest }, ref) {
  return (
    <Switch.Root
      ref={ref}
      className={cn(
        "group/switch inline-flex shrink-0 items-center rounded-full bg-fill-secondary transition-colors disabled:opacity-50 disabled:cursor-not-allowed data-[state=checked]:bg-accent",
        trackSize[size],
        focusClasses,
        className,
      )}
      {...rest}
    >
      <Switch.Thumb
        className={cn(
          "block rounded-full bg-white transition-transform shadow-sm group-active/switch:scale-x-110",
          thumbSize[size],
        )}
      />
    </Switch.Root>
  );
});

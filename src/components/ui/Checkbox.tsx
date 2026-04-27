import { forwardRef } from "react";
import * as RxCheckbox from "@radix-ui/react-checkbox";
import { cn } from "../../lib/cn";
import { Size, focusClasses } from "./_shared";

type Props = React.ComponentPropsWithoutRef<typeof RxCheckbox.Root> & {
  size?: Size;
};

const boxSize: Record<Size, string> = {
  sm: "h-4 w-4",
  md: "h-[18px] w-[18px]",
  lg: "h-[22px] w-[22px]",
};

const checkSize: Record<Size, string> = {
  sm: "h-3 w-3",
  md: "h-3.5 w-3.5",
  lg: "h-4 w-4",
};

export const Checkbox = forwardRef<
  React.ElementRef<typeof RxCheckbox.Root>,
  Props
>(function Checkbox({ size = "md", className, ...rest }, ref) {
  return (
    <RxCheckbox.Root
      ref={ref}
      className={cn(
        "inline-flex items-center justify-center rounded border bg-fill-tertiary border-separator transition-colors disabled:opacity-50 disabled:cursor-not-allowed data-[state=checked]:bg-accent data-[state=checked]:border-accent data-[state=indeterminate]:bg-accent data-[state=indeterminate]:border-accent",
        boxSize[size],
        focusClasses,
        className,
      )}
      {...rest}
    >
      <RxCheckbox.Indicator className="flex items-center justify-center text-accent-label">
        <CheckIcon className={checkSize[size]} />
      </RxCheckbox.Indicator>
    </RxCheckbox.Root>
  );
});

function CheckIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      <path d="M3 8.5L6.5 12L13 4.5" />
    </svg>
  );
}

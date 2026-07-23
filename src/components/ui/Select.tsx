import { forwardRef } from "react";
import { cn } from "../../lib/cn";
import { Size, sizeClasses, focusClasses } from "./_shared";

type Props = Omit<React.SelectHTMLAttributes<HTMLSelectElement>, "size"> & {
  size?: Size;
};

const CHEVRON_BG = `bg-[url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'><path fill='none' stroke='%23808080' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round' d='M3 4.5L6 7.5L9 4.5'/></svg>")] bg-no-repeat bg-[right_0.6rem_center]`;

export const Select = forwardRef<HTMLSelectElement, Props>(function Select(
  { size = "md", className, ...rest },
  ref,
) {
  return (
    <select
      ref={ref}
      className={cn(
        "w-full bg-fill-tertiary border border-separator text-label-primary rounded-control transition-colors appearance-none pr-8 disabled:opacity-50 disabled:cursor-not-allowed aria-invalid:border-status-error focus-visible:border-accent",
        sizeClasses(size),
        focusClasses,
        CHEVRON_BG,
        className,
      )}
      {...rest}
    />
  );
});

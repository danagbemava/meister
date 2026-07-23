import { forwardRef } from "react";
import { cn } from "../../lib/cn";
import { Size, sizeClasses, focusClasses } from "./_shared";

type Props = Omit<React.InputHTMLAttributes<HTMLInputElement>, "size"> & {
  size?: Size;
};

export const Input = forwardRef<HTMLInputElement, Props>(function Input(
  { size = "md", className, type = "text", ...rest },
  ref,
) {
  return (
    <input
      ref={ref}
      type={type}
      className={cn(
        "w-full bg-fill-tertiary border border-separator text-label-primary placeholder:text-text-placeholder rounded-control transition-colors disabled:opacity-50 disabled:cursor-not-allowed aria-invalid:border-status-error aria-invalid:focus-visible:ring-status-error/40 focus-visible:border-accent",
        sizeClasses(size),
        focusClasses,
        className,
      )}
      {...rest}
    />
  );
});

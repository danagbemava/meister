import { forwardRef } from "react";
import { cn } from "../../lib/cn";
import { Size, sizeClasses, focusClasses } from "./_shared";

type Variant = "primary" | "secondary" | "destructive" | "ghost";

type Props = React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: Variant;
  size?: Size;
};

const variantClasses: Record<Variant, string> = {
  primary:
    "bg-accent text-accent-label hover:bg-accent-hover active:bg-accent-pressed",
  secondary:
    "bg-fill-tertiary text-label-primary hover:bg-fill-secondary active:bg-fill-primary",
  destructive:
    "bg-status-error text-white hover:bg-status-error/90 active:bg-status-error/80",
  ghost:
    "text-label-primary hover:bg-fill-tertiary active:bg-fill-secondary",
};

export const Button = forwardRef<HTMLButtonElement, Props>(function Button(
  { variant = "secondary", size = "md", className, type = "button", ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      className={cn(
        "inline-flex items-center justify-center rounded-md font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
        sizeClasses(size),
        focusClasses,
        variantClasses[variant],
        className,
      )}
      {...rest}
    />
  );
});

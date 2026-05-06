import { forwardRef } from "react";
import { cn } from "../../lib/cn";
import { Size, sizeClasses, focusClasses } from "./_shared";
import { Icon, type IconName, type IconSize } from "./Icon";

type Variant = "primary" | "secondary" | "destructive" | "ghost";

type Props = React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: Variant;
  size?: Size;
  iconLeft?: IconName;
  iconRight?: IconName;
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

/** Icon size used inside a Button at a given Button size. Icon-only buttons get
 * one step bigger than icon-with-text so a single glyph reads at a comfortable weight. */
function iconSizeForButton(size: Size, iconOnly: boolean): IconSize {
  if (iconOnly) return size === "lg" ? "md" : "sm";
  return "sm";
}

const gapForSize: Record<Size, string> = {
  sm: "gap-1",
  md: "gap-1.5",
  lg: "gap-2",
};

export const Button = forwardRef<HTMLButtonElement, Props>(function Button(
  {
    variant = "secondary",
    size = "md",
    iconLeft,
    iconRight,
    className,
    type = "button",
    children,
    "aria-label": ariaLabel,
    ...rest
  },
  ref,
) {
  const hasChildren =
    children !== undefined &&
    children !== null &&
    children !== false &&
    !(typeof children === "string" && children.length === 0);
  const iconOnly = !hasChildren && (iconLeft !== undefined || iconRight !== undefined);

  if (iconOnly && !ariaLabel && process.env.NODE_ENV !== "production") {
    // eslint-disable-next-line no-console
    console.warn(
      "[Button] icon-only buttons must set aria-label — assistive tech has no text fallback.",
    );
  }

  const iconSize = iconSizeForButton(size, iconOnly);

  return (
    <button
      ref={ref}
      type={type}
      aria-label={ariaLabel}
      className={cn(
        "inline-flex items-center justify-center rounded-md font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
        sizeClasses(size),
        focusClasses,
        variantClasses[variant],
        hasChildren ? gapForSize[size] : "",
        iconOnly ? "aspect-square px-0" : "",
        className,
      )}
      {...rest}
    >
      {iconLeft && <Icon name={iconLeft} size={iconSize} />}
      {children}
      {iconRight && <Icon name={iconRight} size={iconSize} />}
    </button>
  );
});

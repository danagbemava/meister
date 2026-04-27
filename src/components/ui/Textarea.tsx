import { forwardRef } from "react";
import { cn } from "../../lib/cn";
import { focusClasses } from "./_shared";

type Props = React.TextareaHTMLAttributes<HTMLTextAreaElement>;

export const Textarea = forwardRef<HTMLTextAreaElement, Props>(function Textarea(
  { className, ...rest },
  ref,
) {
  return (
    <textarea
      ref={ref}
      className={cn(
        "w-full bg-fill-tertiary border border-separator text-label-primary placeholder:text-text-placeholder rounded-md px-3 py-2 text-body transition-colors disabled:opacity-50 disabled:cursor-not-allowed aria-invalid:border-status-error aria-invalid:focus-visible:ring-status-error/40 focus-visible:border-accent resize-y",
        focusClasses,
        className,
      )}
      {...rest}
    />
  );
});

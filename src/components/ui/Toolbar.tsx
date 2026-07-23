import { forwardRef } from "react";
import { cn } from "../../lib/cn";

type RootProps = React.HTMLAttributes<HTMLDivElement> & {
  /** From useScrollEdge(scrollRef) — true once content scrolls beneath. */
  scrolled?: boolean;
};

const Root = forwardRef<HTMLDivElement, RootProps>(function Toolbar(
  { className, children, scrolled = false, ...rest },
  ref,
) {
  return (
    <div
      ref={ref}
      data-scrolled={scrolled || undefined}
      className={cn(
        "sticky top-0 z-10 flex h-11 items-center gap-2 px-4",
        "border-b border-transparent transition-colors duration-150",
        "data-[scrolled]:bg-window-background data-[scrolled]:border-separator",
        className,
      )}
      {...rest}
    >
      {children}
    </div>
  );
});

const Spacer = () => <div className="flex-1" />;

export const Toolbar = Object.assign(Root, { Spacer });

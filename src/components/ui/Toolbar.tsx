import { forwardRef } from "react";
import { cn } from "../../lib/cn";

type RootProps = React.HTMLAttributes<HTMLDivElement>;

const Root = forwardRef<HTMLDivElement, RootProps>(function Toolbar(
  { className, children, ...rest },
  ref,
) {
  return (
    <div
      ref={ref}
      className={cn(
        "flex h-11 items-center gap-2 px-4 border-b border-separator",
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

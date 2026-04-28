import { forwardRef } from "react";
import { cn } from "../../lib/cn";

type RootProps = React.HTMLAttributes<HTMLElement>;

const Root = forwardRef<HTMLElement, RootProps>(function Toolbar(
  { className, children, ...rest },
  ref,
) {
  return (
    <section
      ref={ref}
      role="toolbar"
      className={cn(
        "flex h-11 items-center gap-2 px-4 border-b border-separator",
        className,
      )}
      {...rest}
    >
      <div className="contents">{children}</div>
    </section>
  );
});

const Spacer = () => <div className="flex-1" />;

export const Toolbar = Object.assign(Root, { Spacer });

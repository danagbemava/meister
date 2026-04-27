export type Size = "sm" | "md" | "lg";

/** Tailwind classes for height + horizontal padding + font token per size. */
export function sizeClasses(size: Size): string {
  switch (size) {
    case "sm":
      return "h-6 px-2.5 text-callout";
    case "md":
      return "h-8 px-3 text-body";
    case "lg":
      return "h-9 px-4 text-headline";
  }
}

/** Focus ring shown only on keyboard focus, matching macOS HIG. */
export const focusClasses =
  "outline-none focus-visible:ring-2 focus-visible:ring-focus-ring focus-visible:ring-offset-0";

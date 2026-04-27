/** Filter falsy values and join with spaces. Used by ui primitives. */
export function cn(
  ...args: Array<string | undefined | false | null>
): string {
  return args.filter(Boolean).join(" ");
}

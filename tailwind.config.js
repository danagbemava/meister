/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        sans: ["-apple-system", "BlinkMacSystemFont", "SF Pro Display", "Segoe UI", "sans-serif"],
        mono: ["SF Mono", "JetBrains Mono", "Fira Code", "monospace"],
      },
      fontSize: {
        // macOS HIG type scale. Each entry: [size, { lineHeight, fontWeight }]
        "large-title": ["1.625rem", { lineHeight: "2rem",      fontWeight: "600" }],
        "title-1":     ["1.375rem", { lineHeight: "1.625rem", fontWeight: "400" }],
        "title-2":     ["1.0625rem",{ lineHeight: "1.375rem", fontWeight: "400" }],
        "title-3":     ["0.9375rem",{ lineHeight: "1.25rem",  fontWeight: "400" }],
        "headline":    ["0.8125rem",{ lineHeight: "1rem",     fontWeight: "600" }],
        "body":        ["0.8125rem",{ lineHeight: "1rem",     fontWeight: "400" }],
        "callout":     ["0.75rem",  { lineHeight: "0.9375rem",fontWeight: "400" }],
        "subheadline": ["0.6875rem",{ lineHeight: "0.875rem", fontWeight: "400" }],
        "footnote":    ["0.625rem", { lineHeight: "0.8125rem",fontWeight: "400" }],
        "caption-1":   ["0.625rem", { lineHeight: "0.8125rem",fontWeight: "400" }],
        "caption-2":   ["0.625rem", { lineHeight: "0.8125rem",fontWeight: "500" }],
      },
      colors: {
        // Old tokens — kept during migration, removed in Task 15.
        surface: {
          DEFAULT: "#1c1c1e",
          secondary: "#2c2c2e",
          tertiary: "#3a3a3c",
        },

        // Opaque — `<alpha-value>` supported.
        "window-background":      "rgb(var(--color-window-background-rgb) / <alpha-value>)",
        "under-page-background":  "rgb(var(--color-under-page-background-rgb) / <alpha-value>)",
        "control-background":     "rgb(var(--color-control-background-rgb) / <alpha-value>)",
        "sidebar-background":     "rgb(var(--color-sidebar-background-rgb) / <alpha-value>)",
        "control-selected-text":  "rgb(var(--color-control-selected-text-rgb) / <alpha-value>)",
        "accent":                 "rgb(var(--color-accent-rgb) / <alpha-value>)",
        "accent-hover":           "rgb(var(--color-accent-hover-rgb) / <alpha-value>)",
        "accent-pressed":         "rgb(var(--color-accent-pressed-rgb) / <alpha-value>)",
        "accent-label":           "rgb(var(--color-accent-label-rgb) / <alpha-value>)",
        "status-success":         "rgb(var(--color-status-success-rgb) / <alpha-value>)",
        "status-warning":         "rgb(var(--color-status-warning-rgb) / <alpha-value>)",
        "status-error":           "rgb(var(--color-status-error-rgb) / <alpha-value>)",
        "status-info":            "rgb(var(--color-status-info-rgb) / <alpha-value>)",

        // Inherent alpha — plain var() reference, no alpha-modifier support.
        "label-primary":          "var(--color-label-primary)",
        "label-secondary":        "var(--color-label-secondary)",
        "label-tertiary":         "var(--color-label-tertiary)",
        "label-quaternary":       "var(--color-label-quaternary)",
        "text-placeholder":       "var(--color-text-placeholder)",
        "separator":              "var(--color-separator)",
        "fill-primary":           "var(--color-fill-primary)",
        "fill-secondary":         "var(--color-fill-secondary)",
        "fill-tertiary":          "var(--color-fill-tertiary)",
        "fill-quaternary":        "var(--color-fill-quaternary)",
        "control-text":           "var(--color-control-text)",
        "control-text-disabled":  "var(--color-control-text-disabled)",

        // Focus ring — accent at 50% alpha, reused in component focus states.
        "focus-ring":             "rgb(var(--color-accent-rgb) / 0.5)",
      },
    },
  },
  plugins: [],
};

export const colors = {
  surfaceCanvas: "#101114",
  surfacePanel: "#181a1f",
  surfaceRaised: "#20242b",
  textPrimary: "#f2f4f8",
  textSecondary: "#aeb6c2",
  textMuted: "#717987",
  accent: "#52c7b8",
  accentStrong: "#f0b35a",
  danger: "#e66a6a",
  borderSubtle: "rgba(255, 255, 255, 0.10)"
} as const;

export const spacing = {
  xs: "4px",
  sm: "8px",
  md: "12px",
  lg: "16px",
  xl: "24px",
  xxl: "32px"
} as const;

export const radii = {
  control: "6px",
  panel: "8px",
  overlay: "10px"
} as const;

export const typography = {
  fontFamily:
    "system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI Variable', 'Segoe UI', sans-serif",
  display: "600 26px/1.15 system-ui",
  section: "600 16px/1.25 system-ui",
  rowPrimary: "500 14px/1.3 system-ui",
  rowSecondary: "400 12px/1.3 system-ui",
  micro: "600 11px/1.2 system-ui"
} as const;

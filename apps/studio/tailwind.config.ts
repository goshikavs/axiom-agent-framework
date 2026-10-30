import type { Config } from "tailwindcss";

const config: Config = {
  darkMode: "class",
  content: ["./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        canvas: {
          DEFAULT: "#0b1017",
          raised: "#121821",
          overlay: "#18202b",
        },
        line: "#243041",
        ink: {
          DEFAULT: "#e7ecf3",
          muted: "#93a0b5",
          faint: "#66758c",
        },
        accent: {
          DEFAULT: "#3b82f6",
          dim: "#1d4ed8",
        },
      },
      fontFamily: {
        sans: ["IBM Plex Sans", "ui-sans-serif", "system-ui", "sans-serif"],
        mono: ["IBM Plex Mono", "ui-monospace", "SFMono-Regular", "monospace"],
      },
    },
  },
  plugins: [],
};

export default config;

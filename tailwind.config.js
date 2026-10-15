/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: "class",
  theme: {
    extend: {
      fontFamily: {
        sans: ["Inter", "-apple-system", "BlinkMacSystemFont", '"Segoe UI"', "Roboto", "sans-serif"],
        mono: ['"JetBrains Mono"', '"Cascadia Code"', '"Consolas"', '"Courier New"', "monospace"],
      },
      colors: {
        brand: {
          50: "#eef2ff",
          500: "#6366f1",
          600: "#4f46e5",
          700: "#4338ca",
        },
        dark: {
          950: "#070a12",
          900: "#0b0f19",
          850: "#0e1322",
          800: "#13192c",
          700: "#1d2640",
          border: "rgba(120, 160, 255, 0.14)",
          borderStrong: "rgba(120, 160, 255, 0.28)",
        },
      },
    },
  },
  plugins: [],
}

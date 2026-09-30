import parser from "@typescript-eslint/parser";

// Only enforces braces on control statements; layout is Prettier's job.
export default [
  {
    files: ["**/*.{ts,tsx,mts}"],
    languageOptions: { parser },
    rules: { curly: ["error", "all"] },
  },
  { ignores: ["**/node_modules/**", "**/dist/**", "server/src/generated/**"] },
];

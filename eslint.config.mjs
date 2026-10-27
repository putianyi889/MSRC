import js from "@eslint/js";
import globals from "globals";

export default [
  { ignores: [".tools/**", ".venv/**", "target/**", "dist/**"] },
  {
    files: ["**/*.mjs", "**/*.cjs"],
    languageOptions: { ecmaVersion: "latest", globals: globals.node },
    rules: {
      ...js.configs.recommended.rules,
      "eqeqeq": "error"
    }
  }
];

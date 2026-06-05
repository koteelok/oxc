import { describe, expect, it } from "vitest";

import { transformSync } from "../index";

// A capitalized function returning JSX is a React component the compiler memoizes:
// it injects a `_c(n)` memo cache backed by an `import ... from "react/compiler-runtime"`.
const code = `
  function Component(props) {
    return <div onClick={() => props.onClick()}>{props.text}</div>;
  }
`;

describe("reactCompiler", () => {
  it("memoizes a component when enabled with `true`", () => {
    const ret = transformSync("Component.jsx", code, { reactCompiler: true });
    expect(ret.errors).toEqual([]);
    expect(ret.code).toContain("react/compiler-runtime");
    expect(ret.code).toContain("_c(");
  });

  it("accepts a ReactCompilerOptions object", () => {
    const ret = transformSync("Component.jsx", code, {
      reactCompiler: { compilationMode: "all" },
    });
    expect(ret.errors).toEqual([]);
    expect(ret.code).toContain("react/compiler-runtime");
    expect(ret.code).toContain("_c(");
  });

  it("does nothing when omitted (the default)", () => {
    const enabled = transformSync("Component.jsx", code, { reactCompiler: true }).code;
    const disabled = transformSync("Component.jsx", code, {}).code;
    expect(disabled).not.toContain("react/compiler-runtime");
    expect(disabled).not.toContain("_c(");
    expect(disabled).not.toEqual(enabled);
  });

  it("does nothing when `reactCompiler` is false", () => {
    const ret = transformSync("Component.jsx", code, { reactCompiler: false });
    expect(ret.code).not.toContain("react/compiler-runtime");
    expect(ret.code).not.toContain("_c(");
  });
});

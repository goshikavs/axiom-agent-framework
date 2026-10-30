"use client";

import Editor from "@monaco-editor/react";

export function JsonView({ value }: { value: unknown }) {
  const text = JSON.stringify(value ?? null, null, 2);
  return (
    <div className="h-[280px] overflow-hidden rounded border border-line">
      <Editor
        height="280px"
        defaultLanguage="json"
        value={text}
        theme="vs-dark"
        options={{ readOnly: true, minimap: { enabled: false }, scrollBeyondLastLine: false, fontSize: 12, wordWrap: "on" }}
      />
    </div>
  );
}

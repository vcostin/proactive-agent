import { useState } from "react";

type Tab = "chat" | "debug" | "models";

export default function App() {
  const [tab, setTab] = useState<Tab>("chat");

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100vh", fontFamily: "monospace" }}>
      <nav style={{ display: "flex", gap: 8, padding: "8px 12px", borderBottom: "1px solid #333", background: "#1a1a1a" }}>
        {(["chat", "debug", "models"] as Tab[]).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            style={{
              padding: "4px 14px",
              background: tab === t ? "#3a3a3a" : "transparent",
              border: "1px solid #444",
              borderRadius: 4,
              color: "#eee",
              cursor: "pointer",
              textTransform: "capitalize",
            }}
          >
            {t}
          </button>
        ))}
      </nav>

      <main style={{ flex: 1, overflow: "hidden", background: "#111", color: "#eee" }}>
        {tab === "chat" && <ChatPlaceholder />}
        {tab === "debug" && <DebugPlaceholder />}
        {tab === "models" && <ModelsPlaceholder />}
      </main>
    </div>
  );
}

// Phase 5: replace with real components
function ChatPlaceholder() {
  return <Panel label="Chat" note="Phase 5" />;
}
function DebugPlaceholder() {
  return <Panel label="Debug" note="Phase 6" />;
}
function ModelsPlaceholder() {
  return <Panel label="Models" note="Phase 5" />;
}

function Panel({ label, note }: { label: string; note: string }) {
  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", gap: 8, opacity: 0.4 }}>
      <span style={{ fontSize: 18 }}>{label}</span>
      <span style={{ fontSize: 12 }}>({note})</span>
    </div>
  );
}

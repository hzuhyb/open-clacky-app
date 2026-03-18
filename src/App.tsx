import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

type Phase = "installing" | "error";

export default function App() {
  const [phase, setPhase] = useState<Phase>("installing");
  const [error, setError] = useState("");
  const [logs, setLogs] = useState<string[]>([]);
  const logsEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  useEffect(() => {
    const unlistenLog = listen<string>("install-log", (e) => {
      setLogs((prev) => [...prev, e.payload]);
    });

    const unlistenError = listen<string>("install-error", (e) => {
      setError(e.payload);
      setPhase("error");
    });

    return () => {
      unlistenLog.then((f) => f());
      unlistenError.then((f) => f());
    };
  }, []);

  return (
    <div className="installer">
      <div className="installer-header">
        <p className="installer-status">
          {phase === "installing" ? "Installing..." : "Installation failed"}
        </p>
        {phase === "installing" && <div className="progress-bar"><div className="progress-fill" /></div>}
      </div>

      <div className="log-box">
        {logs.map((line, i) => <div key={i} className="log-line">{line}</div>)}
        <div ref={logsEndRef} />
      </div>

      {phase === "error" && (
        <div className="error-box">
          <p>{error}</p>
          <button onClick={() => window.location.reload()}>Retry</button>
        </div>
      )}
    </div>
  );
}

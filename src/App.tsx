import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

type Phase = "installing" | "reboot" | "error";

export default function App() {
  const [phase, setPhase] = useState<Phase>("installing");
  const [message, setMessage] = useState("");
  const [logs, setLogs] = useState<string[]>([]);
  const logsEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  useEffect(() => {
    const unlistenLog = listen<string>("install-log", (e) => {
      setLogs((prev) => [...prev, e.payload]);
    });

    const unlistenReboot = listen<string>("install-reboot", (e) => {
      setMessage(e.payload);
      setPhase("reboot");
    });

    const unlistenError = listen<string>("install-error", (e) => {
      setMessage(e.payload);
      setPhase("error");
    });

    return () => {
      unlistenLog.then((f) => f());
      unlistenReboot.then((f) => f());
      unlistenError.then((f) => f());
    };
  }, []);

  return (
    <div className="installer">
      <div className="installer-header">
        <p className="installer-status">
          {phase === "installing" ? "Installing..." : phase === "reboot" ? "Restart required" : "Installation failed"}
        </p>
        {phase === "installing" && <div className="progress-bar"><div className="progress-fill" /></div>}
      </div>

      <div className="log-box">
        {logs.map((line, i) => <div key={i} className="log-line">{line}</div>)}
        <div ref={logsEndRef} />
      </div>

      {phase === "reboot" && (
        <div className="info-box">
          <p>{message}</p>
        </div>
      )}

      {phase === "error" && (
        <div className="error-box">
          <p>{message}</p>
          <button onClick={() => window.location.reload()}>Retry</button>
        </div>
      )}
    </div>
  );
}

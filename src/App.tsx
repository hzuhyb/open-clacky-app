import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

type Phase = "installing" | "starting" | "ready" | "error";

export default function App() {
  const [phase, setPhase] = useState<Phase>("installing");
  const [logs, setLogs] = useState<string[]>([]);
  const [error, setError] = useState("");
  const logsEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  useEffect(() => {
    const unlisten = listen<string>("install-log", (e) => {
      setLogs((prev) => [...prev, e.payload]);
    });

    invoke("install")
      .then(() => invoke<boolean>("check_server"))
      .then((running) => {
        setPhase("starting");
        return running ? Promise.resolve() : invoke("start_server");
      })
      .then(() => waitForServer())
      .catch((e) => {
        if (String(e).includes("REBOOT_REQUIRED")) {
          setError("WSL components installed. Please restart your computer, then reopen the app.");
        } else {
          setError(String(e));
        }
        setPhase("error");
      });

    return () => { unlisten.then((f) => f()); };
  }, []);

  async function waitForServer() {
    for (let i = 0; i < 60; i++) {
      const ready = await invoke<boolean>("check_server");
      if (ready) {
        setPhase("ready");
        await invoke("open_app");
        return;
      }
      await new Promise((r) => setTimeout(r, 1000));
    }
    setError("Server did not start within 60 seconds.");
    setPhase("error");
  }

  return (
    <div className="installer">
      <div className="installer-header">
        <h1>OpenClacky</h1>
        <p className="installer-status">
          {phase === "installing" && "Installing..."}
          {phase === "starting" && "Starting server..."}
          {phase === "ready" && "Opening..."}
          {phase === "error" && "Installation failed"}
        </p>
        {phase !== "error" && <div className="progress-bar"><div className="progress-fill" /></div>}
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

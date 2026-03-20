import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type Phase = "installing" | "reboot" | "notice" | "error" | "success";
type BusyAction = "idle" | "starting" | "stopping";
type InitialState = { installed: boolean; server_running: boolean };

export default function App() {
  const [phase, setPhase] = useState<Phase | null>(null);
  const [message, setMessage] = useState("");
  const [logs, setLogs] = useState<string[]>([]);
  const [serverRunning, setServerRunning] = useState(false);
  const [justInstalled, setJustInstalled] = useState(false);
  const [busyAction, setBusyAction] = useState<BusyAction>("idle");
  const logsEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  useEffect(() => {
    invoke<InitialState>("get_initial_state")
      .then((state) => {
        setServerRunning(state.server_running);
        if (state.installed) {
          setJustInstalled(false);
          setPhase("success");
        } else {
          setPhase("installing");
        }
      })
      .catch(() => {
        setPhase("installing");
      });

    const unlistenLog = listen<string>("install-log", (e) => {
      setLogs((prev) => [...prev, e.payload]);
    });

    const unlistenReboot = listen<string>("install-reboot", (e) => {
      setMessage(e.payload);
      setPhase("reboot");
    });

    const unlistenNotice = listen<string>("install-notice", (e) => {
      setMessage(e.payload);
      setPhase("notice");
    });

    const unlistenError = listen<string>("install-error", (e) => {
      setMessage(e.payload);
      setPhase("error");
    });

    const unlistenSuccess = listen<boolean>("install-success", (e) => {
      setServerRunning(e.payload);
      setJustInstalled(true);
      setPhase("success");
    });

    const unlistenDashboard = listen<boolean>("show-dashboard", async (e) => {
      if (typeof e.payload === "boolean") {
        setServerRunning(e.payload);
      } else {
        const running = await invoke<boolean>("get_server_status");
        setServerRunning(running);
      }
      setJustInstalled(false);
      setPhase("success");
    });

    const unlistenServerStatus = listen<boolean>("server-status", (e) => {
      setServerRunning(e.payload);
      setBusyAction("idle");
    });

    return () => {
      unlistenLog.then((f) => f());
      unlistenReboot.then((f) => f());
      unlistenNotice.then((f) => f());
      unlistenError.then((f) => f());
      unlistenSuccess.then((f) => f());
      unlistenDashboard.then((f) => f());
      unlistenServerStatus.then((f) => f());
    };
  }, []);

  const openInBrowser = () => {
    invoke("open_url").catch(() => {
      window.open(`http://127.0.0.1:7070`, "_blank");
    });
  };

  const handleStart = () => {
    setBusyAction("starting");
    invoke("start_server").catch(() => {
      setBusyAction("idle");
    });
  };

  const handleStop = () => {
    setBusyAction("stopping");
    invoke("stop_server").catch(() => {
      setBusyAction("idle");
    });
  };

  const handleRetry = () => {
    setLogs([]);
    setMessage("");
    setJustInstalled(false);
    setPhase("installing");
    invoke("retry_install").catch((error) => {
      setMessage(String(error));
      setPhase("error");
    });
  };

  const statusText = () => {
    if (phase === "installing") return "Installing...";
    if (phase === "reboot") return "Restart required";
    if (phase === "notice") return "Action required";
    return "Installation failed";
  };

  if (phase === null) {
    return <div className="celebrate" />;
  }

  const isStopped = !serverRunning && !justInstalled && busyAction === "idle";

  if (phase === "success") {
    return (
      <div className="celebrate">
        <div className={`celebrate-content${isStopped ? " stopped" : ""}`}>
          <div className="celebrate-icon">
            <div className="celebrate-ring ring1" />
            <div className="celebrate-ring ring2" />
            <div className="celebrate-ring ring3" />
            <img src="/icon.png" className="celebrate-logo" onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }} />
            <div className="celebrate-checkmark">{isStopped ? "–" : "✓"}</div>
          </div>
          <h1 className="celebrate-title">{justInstalled ? "You're all set!" : busyAction === "starting" ? "Starting server" : busyAction === "stopping" ? "Stopping server" : serverRunning ? "Server is running" : "Server is stopped"}</h1>
          <p className="celebrate-subtitle">{justInstalled ? "OpenClacky has been installed successfully." : busyAction === "starting" ? "Please wait while the service starts." : busyAction === "stopping" ? "Please wait while the service stops." : serverRunning ? "Ready to open in your browser." : "Start the service to continue."}</p>
          <div className="celebrate-buttons">
            <button className="btn btn-primary" onClick={openInBrowser} disabled={!serverRunning || busyAction !== "idle"}>Open in Browser</button>
            {!serverRunning && (
              <button className="btn btn-secondary" onClick={handleStart} disabled={busyAction !== "idle"}>{busyAction === "starting" ? "Starting..." : "Start Server"}</button>
            )}
            {serverRunning && (
              <button className="btn btn-ghost" onClick={handleStop} disabled={busyAction !== "idle"}>{busyAction === "stopping" ? "Stopping..." : "Stop Server"}</button>
            )}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="installer">
      <div className="installer-header">
        <p className="installer-status">{statusText()}</p>
        {phase === "installing" && <div className="progress-bar"><div className="progress-fill" /></div>}
      </div>

      <div className="log-box">
        {logs.length === 0
          ? <div className="log-line log-placeholder">Waiting...</div>
          : logs.map((line, i) => <div key={i} className="log-line">{line}</div>)
        }
        <div ref={logsEndRef} />
      </div>

      {phase === "reboot" && (
        <div className="info-box">
          <p>{message}</p>
          <button onClick={() => invoke("reboot_system")}>Restart Now</button>
        </div>
      )}

      {phase === "notice" && (
        <div className="info-box">
          <p>{message}</p>
        </div>
      )}

      {phase === "error" && (
        <div className="error-box">
          <p>{message}</p>
          <button onClick={handleRetry}>Retry</button>
        </div>
      )}
    </div>
  );
}

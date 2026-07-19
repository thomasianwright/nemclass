import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import "dockview-react/dist/styles/dockview.css";
// Side-effect: self-host Monaco (loader.config + worker) before any editor mounts.
import "./lib/monaco";

// dockview + React StrictMode double-mounting can race panel init, so we mount
// the app directly.
ReactDOM.createRoot(document.getElementById("root")!).render(<App />);

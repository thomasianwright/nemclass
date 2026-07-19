import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import "dockview-react/dist/styles/dockview.css";

// dockview + React StrictMode double-mounting can race panel init, so we mount
// the app directly.
ReactDOM.createRoot(document.getElementById("root")!).render(<App />);

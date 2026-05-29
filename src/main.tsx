import React from "react";
import ReactDOM from "react-dom/client";
import App from "./app";
import { applyTheme, loadTheme } from "./theme";
import "./styles.css";

// 渲染前先应用主题，避免明暗切换时的闪烁。
applyTheme(loadTheme());

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);

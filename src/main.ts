import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { log } from "./lib/api";

window.addEventListener("error", (e) => log(`erreur : ${e.message} (${e.filename}:${e.lineno})`));
window.addEventListener("unhandledrejection", (e) => log(`promesse rejetée : ${String(e.reason)}`));

export default mount(App, { target: document.getElementById("app")! });

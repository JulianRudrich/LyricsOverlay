import { mount } from "svelte";
import Editor from "./editor/Editor.svelte";
import Overlay from "./overlay/Overlay.svelte";
import Settings from "./settings/Settings.svelte";
import "./styles.css";

// One page serves every window; the Rust side opens `#/overlay`, `#/settings` or `#/editor`.
const route = window.location.hash.replace(/^#\/?/, "");
const view = route === "settings" || route === "editor" ? route : "overlay";
document.documentElement.dataset.view = view;

const target = document.getElementById("app");
if (target) {
  mount(view === "editor" ? Editor : view === "settings" ? Settings : Overlay, { target });
}

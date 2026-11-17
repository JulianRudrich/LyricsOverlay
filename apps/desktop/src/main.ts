import { mount } from "svelte";
import Overlay from "./overlay/Overlay.svelte";
import Settings from "./settings/Settings.svelte";
import "./styles.css";

// One page serves both windows; the Rust side opens `#/overlay` or `#/settings`.
const view = window.location.hash.replace(/^#\/?/, "") === "settings" ? "settings" : "overlay";
document.documentElement.dataset.view = view;

const target = document.getElementById("app");
if (target) {
  mount(view === "settings" ? Settings : Overlay, { target });
}

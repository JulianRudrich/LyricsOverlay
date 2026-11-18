import { mount } from "svelte";
import Editor from "./editor/Editor.svelte";
import Overlay from "./overlay/Overlay.svelte";
import Settings from "./settings/Settings.svelte";
import Stats from "./stats/Stats.svelte";
import "./styles.css";

// One page serves every window; the Rust side opens `#/overlay`, `#/settings`, `#/editor` or `#/stats`.
const route = window.location.hash.replace(/^#\/?/, "");
const views = { overlay: Overlay, settings: Settings, editor: Editor, stats: Stats } as const;
const view = (Object.prototype.hasOwnProperty.call(views, route) ? route : "overlay") as keyof typeof views;
document.documentElement.dataset.view = view;

const target = document.getElementById("app");
if (target) {
  mount(views[view], { target });
}

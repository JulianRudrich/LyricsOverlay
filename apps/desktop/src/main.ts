import { mount } from "svelte";
import Editor from "./editor/Editor.svelte";
import Karaoke from "./karaoke/Karaoke.svelte";
import Overlay from "./overlay/Overlay.svelte";
import Settings from "./settings/Settings.svelte";
import MenuPanel from "./menu/MenuPanel.svelte";
import Stats from "./stats/Stats.svelte";
import Shuffle from "./shuffle/Shuffle.svelte";
import Welcome from "./welcome/Welcome.svelte";
import "./styles.css";

// One page serves every window; the Rust side opens `#/overlay`, `#/settings`, `#/welcome`, `#/editor`, `#/stats`,
// `#/shuffle`, `#/karaoke` or `#/menu`, the tray panel.
const route = window.location.hash.replace(/^#\/?/, "");
const views = {
  overlay: Overlay,
  settings: Settings,
  welcome: Welcome,
  editor: Editor,
  stats: Stats,
  shuffle: Shuffle,
  karaoke: Karaoke,
  menu: MenuPanel,
} as const;
const view = (Object.prototype.hasOwnProperty.call(views, route) ? route : "overlay") as keyof typeof views;
document.documentElement.dataset.view = view;

// The Rust side says how it built the window (`?glass&titlebar=overlay`), so the
// page can match it from the first paint.
const look = new URLSearchParams(window.location.search);
if (look.has("glass")) document.documentElement.dataset.glass = "";
const titlebar = look.get("titlebar");
if (titlebar) document.documentElement.dataset.titlebar = titlebar;
// The corner radius of a window that draws its own edge, such as the tray panel.
const radius = Number(look.get("radius"));
if (radius > 0) document.documentElement.style.setProperty("--window-radius", `${radius}px`);

const target = document.getElementById("app");
if (target) {
  mount(views[view], { target });
}

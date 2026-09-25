import { mount } from "svelte";

import App from "./App.svelte";
import "./app.css";
import { followAppTheme } from "./lib/appTheme";

// Started before mounting so the first paint is already in the right mode.
void followAppTheme();

const target = document.getElementById("app");
if (!target) throw new Error("missing #app mount point");

export default mount(App, { target });

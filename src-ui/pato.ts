import * as db from "./lib/db.js";
import { PatoGrid, PatoHeader, PatoHeaderLeft, PatoHeaderCenter, PatoHeaderRight, PatoStatusWidgets, PatoSystemMenu } from "./components/index.js";
import { callRust } from "./lib/bindings.js";
import "./lib/db.js";
import "./lib/storage.js";
import "./lib/widgets.js";
import "./lib/log.js";

export async function init() {
  // Initialize dependencies
  await db.init();
  await initHeader();
  // The grid must exist before plugins load so widget register/update calls
  // from the core have somewhere to mount.
  document.body.append(new PatoGrid());
  await callRust("pato_ready");
}

export async function initHeader() {
  document.body.append(
    new PatoHeader(new PatoHeaderLeft(), new PatoHeaderCenter(), new PatoHeaderRight(new PatoStatusWidgets(), new PatoSystemMenu()))
  );
}

init();

import { initApiAdapter } from "./apiAdapter";

export async function configureDesktopRuntime() {
  await initApiAdapter();
}

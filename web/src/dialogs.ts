// The desktop WebView has no window.confirm/alert; there the app shows native
// dialogs instead.
import { isDesktop, run } from "./desktop";

export async function confirmDialog(message: string): Promise<boolean> {
  return isDesktop
    ? run<boolean>("dialog_confirm", { message })
    : window.confirm(message);
}

export function alertDialog(message: string): void {
  if (isDesktop) {
    void run("dialog_alert", { message });
  } else {
    window.alert(message);
  }
}

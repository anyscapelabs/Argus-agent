import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

export async function notifyDone(title: string, body: string) {
  try {
    if (!(await isPermissionGranted())) {
      const perm = await requestPermission();
      if (perm !== "granted") return;
    }

    sendNotification({ title, body });
  } catch {}
}

import { bootstrapApplication } from '@angular/platform-browser';
import { appConfig } from './app/app.config';
import { App } from './app/app';
import { SkinView } from './skin/skin-view';
import { isTauri } from './shared/ipc';

async function selectRoot() {
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    if (getCurrentWindow().label.startsWith('skin-')) {
      document.documentElement.classList.add('skin-mode');
      return SkinView;
    }
  }
  return App;
}

selectRoot()
  .then((root) => bootstrapApplication(root, appConfig))
  .catch((err) => console.error(err));

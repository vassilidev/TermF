import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "$lib/i18n";
import { showSettings } from "$lib/settings.svelte";

/** The native macOS menu in the app's language, and its Settings… item (⌘,). */
export function watchMenu(): () => void {
  const words = {
    about: t("packagingMenuAbout"),
    settings: t("packagingMenuSettings"),
    services: t("packagingMenuServices"),
    hide: t("packagingMenuHide"),
    hideOthers: t("packagingMenuHideOthers"),
    showAll: t("packagingMenuShowAll"),
    quit: t("packagingMenuQuit"),
    edit: t("packagingMenuEdit"),
    undo: t("packagingMenuUndo"),
    redo: t("packagingMenuRedo"),
    cut: t("packagingMenuCut"),
    copy: t("packagingMenuCopy"),
    paste: t("packagingMenuPaste"),
    selectAll: t("packagingMenuSelectAll"),
    view: t("packagingMenuView"),
    fullscreen: t("packagingMenuFullscreen"),
    window: t("packagingMenuWindow"),
    minimize: t("packagingMenuMinimize"),
    zoom: t("packagingMenuZoom"),
    front: t("packagingMenuFront"),
    help: t("packagingMenuHelp"),
    website: t("packagingMenuWebsite"),
  };
  invoke("app_menu", { words }).catch(() => {});
  const off = listen("menu-settings", () => showSettings());
  return () => void off.then((f) => f());
}

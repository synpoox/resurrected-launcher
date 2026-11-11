// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { installBrowserGuard, wantsContextMenu } from "./browserGuard";

let uninstall: () => void;
beforeEach(() => {
  document.body.innerHTML = `
    <main id="plain"><p id="text">Hello</p><img id="img" alt="" /><a id="link" href="#">x</a></main>
    <input id="input" /><textarea id="area"></textarea>
    <div id="editable" contenteditable="true"><span id="inner">e</span></div>
    <div id="off" contenteditable="false">n</div>`;
  uninstall = installBrowserGuard();
});
afterEach(() => {
  uninstall();
  document.getSelection()?.removeAllRanges();
});

const fire = (id: string, type: string) => {
  const event = new Event(type, { bubbles: true, cancelable: true });
  document.getElementById(id)!.dispatchEvent(event);
  return event.defaultPrevented;
};

describe("browser guard", () => {
  it("blocks the context menu on ordinary content", () => {
    expect(fire("plain", "contextmenu")).toBe(true);
    expect(fire("img", "contextmenu")).toBe(true);
    expect(fire("off", "contextmenu")).toBe(true);
  });

  it("keeps the context menu in editable fields", () => {
    for (const id of ["input", "area", "editable", "inner"]) expect(fire(id, "contextmenu")).toBe(false);
  });

  it("keeps the context menu when text is selected", () => {
    const range = document.createRange();
    range.selectNodeContents(document.getElementById("text")!);
    document.getSelection()!.addRange(range);
    expect(fire("plain", "contextmenu")).toBe(false);
  });

  it("blocks dragging images and links but not other drags", () => {
    expect(fire("img", "dragstart")).toBe(true);
    expect(fire("link", "dragstart")).toBe(true);
    expect(fire("text", "dragstart")).toBe(false);
  });

  it("uninstalls cleanly", () => {
    uninstall();
    expect(fire("plain", "contextmenu")).toBe(false);
    uninstall = () => {};
  });

  it("treats non-element targets as ordinary content", () => {
    expect(wantsContextMenu(null, "")).toBe(false);
    expect(wantsContextMenu(document, "")).toBe(false);
  });
});

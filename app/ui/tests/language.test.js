const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const test = require("node:test");

function loadLanguage(saved, systemLanguage) {
  const window = {};
  const context = {
    window,
    navigator: { language: systemLanguage },
    localStorage: { getItem: () => saved },
  };
  const source = fs.readFileSync(path.join(__dirname, "..", "i18n.js"), "utf8");
  vm.runInNewContext(source, context);
  return window.i18n.lang;
}

function loadTranslations() {
  const document = { documentElement: {}, querySelectorAll: () => [] };
  const window = { dispatchEvent: () => {} };
  const context = {
    window,
    document,
    navigator: { language: "en-US" },
    localStorage: { getItem: () => null, setItem: () => {} },
    Event: class Event {},
  };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, "..", "i18n.js"), "utf8"), context);
  return window.i18n;
}

test("English is the default regardless of the operating system language", () => {
  assert.equal(loadLanguage(null, "ru-RU"), "en");
  assert.equal(loadLanguage(null, "es-ES"), "en");
});

test("an explicitly saved optional language is restored", () => {
  assert.equal(loadLanguage("ru", "en-US"), "ru");
  assert.equal(loadLanguage("es", "en-US"), "es");
});

test("every translation key used by the shipped HTML exists in all supported languages", () => {
  const i18n = loadTranslations();
  const html = fs.readFileSync(path.join(__dirname, "..", "index.html"), "utf8");
  const keys = new Set();
  for (const match of html.matchAll(/data-i18n(?:-aria|-ph|-html)?="([^"]+)"/g)) keys.add(match[1]);
  assert.ok(keys.size > 50);
  for (const language of ["en", "ru", "es"]) {
    i18n.set(language);
    for (const key of keys) assert.ok(i18n.has(key), `${language} is missing ${key}`);
  }
});

test("hold-to-compare mode and instruction are translated in every supported language", () => {
  const i18n = loadTranslations();
  const values = {
    en: ["Hold", "Hold the pointer or Space to reveal the original"],
    ru: ["Удерживать", "Удерживайте указатель или Space, чтобы увидеть оригинал"],
    es: ["Mantener", "Mantén el puntero o pulsa Espacio para ver el original"],
  };
  for (const [language, expected] of Object.entries(values)) {
    i18n.set(language);
    assert.deepEqual([i18n.t("compareHold"), i18n.t("compareHoldHint")], expected);
  }
});

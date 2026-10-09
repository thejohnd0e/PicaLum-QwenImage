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

test("English is the default regardless of the operating system language", () => {
  assert.equal(loadLanguage(null, "ru-RU"), "en");
  assert.equal(loadLanguage(null, "es-ES"), "en");
});

test("an explicitly saved optional language is restored", () => {
  assert.equal(loadLanguage("ru", "en-US"), "ru");
  assert.equal(loadLanguage("es", "en-US"), "es");
});

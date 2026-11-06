import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  CATALOGS,
  LANGUAGE_LABELS,
  LANGUAGE_STORAGE_KEY,
  SUPPORTED_LANGUAGES,
  isSupportedLanguage,
  applyTranslations,
  loadLanguage,
  saveLanguage,
  translate,
} from "../assets/language.js";

const memory = () => {
  const data = new Map();
  return {
    data,
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => data.set(key, value),
  };
};

test("f15_us1_as1_supported_languages_are_exactly_english_french_spanish_and_german", () => {
  assert.deepEqual(SUPPORTED_LANGUAGES, ["en", "fr", "es", "de"]);
  assert.deepEqual(LANGUAGE_LABELS, {
    en: "English",
    fr: "Français",
    es: "Español",
    de: "Deutsch",
  });
  for (const language of SUPPORTED_LANGUAGES) assert.equal(isSupportedLanguage(language), true);
  for (const value of ["", "en-US", "it", "EN", null, undefined, 1]) {
    assert.equal(isSupportedLanguage(value), false);
  }
});

test("f15_us1_as2_all_language_catalogs_have_the_same_nonempty_message_keys", () => {
  const keys = Object.keys(CATALOGS.en).sort();
  assert.ok(keys.length > 0);
  for (const language of SUPPORTED_LANGUAGES) {
    assert.deepEqual(Object.keys(CATALOGS[language]).sort(), keys, `${language} catalog keys`);
    for (const key of keys) {
      assert.equal(typeof CATALOGS[language][key], "string", `${language}.${key}`);
      assert.ok(CATALOGS[language][key].trim().length > 0, `${language}.${key} is empty`);
    }
  }
});

test("f15_us1_as2_every_static_html_translation_key_exists_in_every_locale_catalog", () => {
  const html = readFileSync(new URL("../assets/index.html", import.meta.url), "utf8");
  const keys = [...new Set([...html.matchAll(/data-i18n(?:-[a-z-]+)?="([^"]+)"/g)].map((match) => match[1]))];
  assert.ok(keys.length >= 100);
  for (const language of SUPPORTED_LANGUAGES) {
    for (const key of keys) assert.ok(Object.hasOwn(CATALOGS[language], key), `${language}.${key}`);
  }
});

test("f15_us1_as3_static_text_and_accessibility_attributes_update_without_replacing_controls", () => {
  const label = {
    dataset: { i18n: "room.name", i18nAriaLabel: "room.name" },
    textContent: "Room name",
    attributes: {},
    setAttribute(name, value) { this.attributes[name] = value; },
  };
  const selector = { dataset: {}, value: "de" };
  const root = { matches: () => false, querySelectorAll: () => [label, selector] };
  applyTranslations(root, "fr");
  assert.equal(label.textContent, "Nom du salon");
  assert.equal(label.attributes["aria-label"], "Nom du salon");
  assert.equal(selector.value, "de");
  assert.equal(root.querySelectorAll()[0], label);
});

test("f15_us1_as1_localized_language_and_main_menu_copy_uses_the_selected_catalog", () => {
  assert.equal(translate("en", "language.label"), "Language");
  assert.equal(translate("fr", "language.label"), "Langue");
  assert.equal(translate("es", "language.label"), "Idioma");
  assert.equal(translate("de", "language.label"), "Sprache");
  assert.notEqual(translate("fr", "menu.create"), translate("en", "menu.create"));
});

test("f15_us2_as4_missing_or_invalid_catalog_entries_fall_back_without_exposing_a_key", () => {
  assert.equal(translate("not-a-language", "menu.create"), translate("en", "menu.create"));
  assert.equal(translate("fr", "menu.create"), translate("fr", "menu.create"));
  const missing = translate("fr", "missing.translation.key");
  assert.equal(typeof missing, "string");
  assert.ok(missing.length > 0);
  assert.notEqual(missing, "missing.translation.key");
});

test("f15_us1_as4_language_preference_is_persisted_and_restored_from_device_storage", () => {
  const storage = memory();
  assert.equal(LANGUAGE_STORAGE_KEY, "eksplode.re.language.v1");
  assert.equal(loadLanguage(storage), "en");
  saveLanguage(storage, "de");
  assert.equal(storage.data.get(LANGUAGE_STORAGE_KEY), "de");
  assert.equal(loadLanguage(storage), "de");
});

test("f15_us1_as5_a_new_or_unconfigured_device_defaults_to_english", () => {
  assert.equal(loadLanguage(undefined), "en");
  assert.equal(loadLanguage(memory()), "en");
});

test("f15_us2_as3_absent_invalid_and_unavailable_language_storage_defaults_to_english", () => {
  const storage = memory();
  storage.data.set(LANGUAGE_STORAGE_KEY, "it");
  assert.equal(loadLanguage(storage), "en");
  storage.data.set(LANGUAGE_STORAGE_KEY, "{broken");
  assert.equal(loadLanguage(storage), "en");
  assert.equal(loadLanguage(undefined), "en");
  const throwing = {
    getItem() { throw new Error("blocked"); },
    setItem() { throw new Error("full"); },
  };
  assert.equal(loadLanguage(throwing), "en");
  assert.doesNotThrow(() => saveLanguage(throwing, "fr"));
  assert.doesNotThrow(() => saveLanguage(storage, "unknown"));
  assert.equal(storage.data.get(LANGUAGE_STORAGE_KEY), "{broken");
});

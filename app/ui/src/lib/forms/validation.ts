// the engine's refusals, said again in a few plain words beside the field.
//
// the engine's sentences are already written for people ("The field
// “student_id” is required, so it can’t be left empty."), but they name
// the field by its internal name. beside the field itself the name is
// clear from where the message sits, so it is said shorter here.

import type { FieldInfo } from "./formsApi";

interface Rule {
  pattern: RegExp;
  say: (match: RegExpMatchArray, field: FieldInfo) => string;
}

const RULES: Rule[] = [
  { pattern: /is required, so it can’t be left empty/, say: () => "This field is required. Type an answer." },
  { pattern: /takes at most (\d+) characters/, say: (match) => `This field takes at most ${match[1]} characters.` },
  {
    pattern: /isn’t a date Letters understands/,
    say: (_, field) => (field.dateFormat ? `Type a date as ${field.dateFormat}, or like 2026-11-11.` : "Type a date like 2026-11-11."),
  },
  { pattern: /isn’t one of the choices/, say: () => "Choose one of the choices offered." },
  { pattern: /is read-only/, say: () => "This field is read-only, so it can’t be changed." },
  {
    pattern: /can’t write “.*” into a form field yet/,
    say: () => "Letters can only type letters used in Western European languages into form fields for now.",
  },
];

/** a refusal from the engine, said for the place beside the field. */
export function plainFieldMessage(engineMessage: string, field: FieldInfo): string {
  for (const rule of RULES) {
    const match = engineMessage.match(rule.pattern);
    if (match) return rule.say(match, field);
  }
  // anything else is the engine's own sentence, with "this field" for its internal name
  const said = engineMessage.replace(`“${field.name}”`, "this field");
  // a sentence that began with the name still begins with a capital (form logic's refusals do)
  return said.startsWith("this field") ? `T${said.slice(1)}` : said;
}

/** the mark beside a required field that has no answer yet. */
export function requiredNote(field: FieldInfo, filled: boolean): string | null {
  return field.required && !filled ? "Required" : null;
}

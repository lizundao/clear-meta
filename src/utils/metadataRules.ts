import type { FieldEditState, FieldRule, MetadataField } from "../types";

export function fieldKey(field: MetadataField): string {
  return field.key;
}

export function defaultEditState(field: MetadataField): FieldEditState {
  return { action: "keep", value: field.value };
}

export function editStatesToRules(
  fields: MetadataField[],
  states: Record<string, FieldEditState>,
): FieldRule[] {
  const rules: FieldRule[] = [];

  for (const field of fields) {
    if (field.editable === false) continue;
    const state = states[field.key] ?? defaultEditState(field);
    if (state.action === "keep") continue;

    rules.push({
      key: field.key,
      action: state.action,
      value: state.action === "set" ? state.value : undefined,
    });
  }

  return rules;
}

export function rulesToEditStates(
  fields: MetadataField[],
  rules: FieldRule[],
): Record<string, FieldEditState> {
  const next: Record<string, FieldEditState> = {};

  for (const field of fields) {
    if (field.editable === false) {
      next[field.key] = defaultEditState(field);
      continue;
    }

    const matched = rules.find((rule) => ruleMatches(rule.key, field.key));
    if (matched) {
      next[field.key] = {
        action: matched.action,
        value: matched.value ?? field.value,
      };
    } else {
      next[field.key] = defaultEditState(field);
    }
  }

  return next;
}

function ruleMatches(ruleKey: string, fieldKey: string): boolean {
  if (ruleKey === fieldKey) return true;
  if (ruleKey.endsWith(":*")) {
    const prefix = ruleKey.slice(0, -1);
    return fieldKey.startsWith(prefix);
  }
  return false;
}

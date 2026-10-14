// Types and parsing for `agentmark check --json` (documented in docs/check-json.md).
// Pure module: no vscode import, so it is unit-tested with plain node.

export type EffectClass = "pure" | "local" | "network" | "model" | "unverified";

export interface Diagnostic {
  code: string;
  severity: "error" | "warning";
  file: string;
  line: number;
  column: number;
  length: number;
  message: string;
  did_you_mean: string | null;
  help: string | null;
}

export interface EffectTools {
  model: boolean;
  network: string[];
  local: string[];
  unverified: string[];
}

export interface Effects {
  class: EffectClass;
  summary: string;
  tools: EffectTools;
}

export interface Block {
  label: string;
  file: string;
  line: number;
  effects: Effects;
}

export interface Unit {
  name: string;
  kind: "agent" | "service" | "command" | "function" | "rule";
  file: string;
  effects: Effects;
  blocks: Block[];
}

export interface InferredField {
  record: string;
  field: string;
  type: string;
  fallback: boolean;
  file: string;
  line: number;
}

export interface UnverifiedBinding {
  name: string;
  crate: string;
  file: string;
  line: number;
  proposed_effect: string | null;
  items: number;
}

export interface RustBlock {
  name: string;
  file: string;
  line: number;
  parent: string | null;
  effect: string;
}

export interface CheckReport {
  schema_version: number;
  ok: boolean;
  path: string;
  diagnostics: Diagnostic[];
  units: Unit[];
  inferred_fields: InferredField[];
  unverified_bindings: UnverifiedBinding[];
  rust_blocks: RustBlock[];
}

export const SUPPORTED_SCHEMA_VERSION = 1;

export class CheckJsonError extends Error {}

const emptyTools = (): EffectTools => ({ model: false, network: [], local: [], unverified: [] });

const CLASSES = ["pure", "local", "network", "model", "unverified"];

function normEffects(e: any): Effects {
  const t = e?.tools ?? {};
  return {
    // an effect class this extension does not know is treated as the most cautious one
    class: CLASSES.includes(e?.class) ? e.class : e?.class === undefined ? "pure" : "unverified",
    summary: String(e?.summary ?? "pure"),
    tools: {
      ...emptyTools(),
      model: !!t.model,
      network: Array.isArray(t.network) ? t.network : [],
      local: Array.isArray(t.local) ? t.local : [],
      unverified: Array.isArray(t.unverified) ? t.unverified : [],
    },
  };
}

/** Parse the stdout of `agentmark check --json`. Unknown extra keys are ignored (the schema only grows). */
export function parseCheckJson(text: string): CheckReport {
  let raw: any;
  try {
    raw = JSON.parse(text);
  } catch (e) {
    throw new CheckJsonError(`agentmark check --json did not print JSON: ${(e as Error).message}`);
  }
  if (typeof raw !== "object" || raw === null || typeof raw.ok !== "boolean") {
    throw new CheckJsonError("agentmark check --json printed JSON without an \"ok\" field.");
  }
  if (raw.schema_version !== SUPPORTED_SCHEMA_VERSION) {
    throw new CheckJsonError(
      `This extension understands check --json schema_version ${SUPPORTED_SCHEMA_VERSION}, but the agentmark binary printed ${raw.schema_version}. Update the extension.`,
    );
  }
  return {
    schema_version: raw.schema_version,
    ok: raw.ok,
    path: String(raw.path ?? ""),
    diagnostics: raw.diagnostics ?? [],
    units: (raw.units ?? []).map((u: any) => ({
      ...u,
      effects: normEffects(u.effects),
      blocks: (u.blocks ?? []).map((b: any) => ({ ...b, effects: normEffects(b.effects) })),
    })),
    inferred_fields: raw.inferred_fields ?? [],
    unverified_bindings: raw.unverified_bindings ?? [],
    rust_blocks: raw.rust_blocks ?? [],
  };
}

/** Every distinct tool a unit can reach, with the class it reaches it through. */
export function reach(effects: Effects): { name: string; class: EffectClass }[] {
  const out: { name: string; class: EffectClass }[] = [];
  if (effects.tools.model) out.push({ name: "model", class: "model" });
  for (const n of effects.tools.network) out.push({ name: n, class: "network" });
  for (const n of effects.tools.local) out.push({ name: n, class: "local" });
  for (const n of effects.tools.unverified) out.push({ name: n, class: "unverified" });
  return out;
}

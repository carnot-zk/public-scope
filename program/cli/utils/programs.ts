import path from "path";

export const ROOT_DIR = path.resolve(__dirname, "../..");

const RAW_PROGRAMS = [
  {
    id: "carnot_engine",
    displayName: "Carnot Engine",
    anchorName: "carnot_engine",
    testDir: "carnot_engine",
    rustFilter: "carnot_engine",
    computeStats: ["CarnotEngine"],
    aliases: ["carnot", "engine", "settlement"],
  },
] as const;

type ProgramId = (typeof RAW_PROGRAMS)[number]["id"];

export interface ProgramDefinition {
  id: ProgramId;
  displayName: string;
  anchorName: string;
  testDir?: string;
  rustFilter?: string;
  computeStats?: string[];
  aliases?: string[];
}

const PROGRAMS: ProgramDefinition[] = RAW_PROGRAMS.map((program) => ({
  ...program,
  computeStats: [...(program.computeStats ?? [])],
  aliases: "aliases" in program ? [...program.aliases] : undefined,
}));

const PROGRAM_LOOKUP = new Map<string, ProgramDefinition>();
PROGRAMS.forEach((program) => {
  const keys = new Set<string>();
  keys.add(program.id);
  keys.add(program.anchorName);
  keys.add(program.displayName);
  if (program.testDir) keys.add(program.testDir);
  program.aliases?.forEach((alias) => keys.add(alias));
  keys.forEach((key) => PROGRAM_LOOKUP.set(normalizeProgramKey(key), program));
});

function normalizeProgramKey(value: string | undefined): string {
  if (!value) return "";
  return value
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/[\s-]+/g, "_")
    .toLowerCase()
    .trim();
}

function resolveProgram(value: string): ProgramDefinition | undefined {
  if (!value) return undefined;
  const normalized = normalizeProgramKey(value);
  if (normalized === "all") return undefined;
  return PROGRAM_LOOKUP.get(normalized);
}

export function selectPrograms(programArg?: string): ProgramDefinition[] {
  const normalized = normalizeProgramKey(programArg);
  if (!normalized || normalized === "all") {
    return [...PROGRAMS];
  }
  const program = resolveProgram(normalized);
  if (!program) {
    throw new Error(
      `Unknown program "${programArg}". Use "pnpm build --list-programs" to see supported values.`
    );
  }
  return [program];
}

export function listProgramSummaries() {
  return PROGRAMS.map((program) => ({
    program: program.id,
    name: program.displayName,
    anchor: program.anchorName,
    tsTests: Boolean(program.testDir),
    rustTests: Boolean(program.rustFilter),
  }));
}


// No imports, so `pnpm test` runs it in plain Node.

/** Words that never name a project, in French and English. */
const FILLER = new Set(
  (
    "un une le la les des de du au aux et ou ni pour par avec sans qui que quoi dont en dans sur sous chez entre " +
    "mon ma mes ton ta tes son sa ses notre nos votre vos leur leurs ce cet cette ces je tu il elle on nous vous " +
    "ils elles est sont etre avoir ai as faut peut quand comme plus tres tout tous toute toutes faire creer cree " +
    "permet aide aider veux voudrais outil application appli app site logiciel service plateforme ligne commande " +
    "simple petit petite nouveau nouvelle " +
    "a an the of to for with without and or by on in at into from my our your their this that which who what " +
    "when where is are be can should would will i we you it its me us want build make help tool site service " +
    "platform command line cli simple small new"
  ).split(" "),
);

/**
 * A short name for a project, from the first words of its idea that say something: « Une app pour suivre l'arrosage de
 * mes plantes » gives `suivre-arrosage-plantes`.
 */
export function nameFromIdea(idea: string, words = 3): string {
  return idea
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((w) => w.length > 1 && !FILLER.has(w))
    .slice(0, words)
    .join("-")
    .slice(0, 48)
    .replace(/-+$/, "");
}

/** What the owner types, made a folder and GitHub repository name: letters, digits, `-`, `_` and `.`. */
export function cleanName(text: string): string {
  return text
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/\s+/g, "-")
    .replace(/[^A-Za-z0-9._-]/g, "")
    .slice(0, 100);
}

/** `name`, or `name-2`, `name-3`… when a project already has it (whatever the case): still a folder name. */
export function uniqueSlug(name: string, taken: string[]): string {
  const used = new Set(taken.map((t) => t.trim().toLowerCase()));
  if (!name || !used.has(name.toLowerCase())) return name;
  let n = 2;
  while (used.has(`${name}-${n}`.toLowerCase())) n++;
  return `${name}-${n}`;
}

const dir = (path: string) => path.replace(/[\\/]+$/, "").replace(/[\\/][^\\/]*$/, "");

/** Where the new project goes: the folder holding most of the known repositories, else the home folder. */
export function parentOf(repos: { path: string }[], home: string): string {
  const count = new Map<string, number>();
  for (const r of repos) {
    const d = dir(r.path);
    if (d) count.set(d, (count.get(d) ?? 0) + 1);
  }
  let best = home;
  let most = 0;
  for (const [d, n] of count) if (n > most) [best, most] = [d, n];
  return best;
}

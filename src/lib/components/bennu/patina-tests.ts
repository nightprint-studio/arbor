/**
 * I `#[test]` di un `.ptn`, letti dal testo: dove mettere il ▶ e quale cartella-mod lanciare.
 *
 * Funzioni pure. Il nome che il banco (`patina-test`) stampa e' `«mod»::«file»::«fn»`; qui basta il nome della funzione, perche' `--only`
 * accetta anche il suffisso `::«fn»`.
 */

export interface PatinaTestMark {
  /** 1-based: la riga dell'attributo `#[test]` (li' sta la freccia, come sull'annotazione `@Test` di Java). */
  line: number;
  /** Il nome della funzione che l'attributo annuncia. */
  name: string;
}

const TEST_ATTR = /^\s*#\[test\b[^\]]*\]/;
const FN = /^\s*(?:pub\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)/;

/** Le funzioni `#[test]` di un sorgente. L'attributo puo' essere seguito da altri attributi, commenti o righe vuote prima della `fn`. */
export function patinaTestMarks(src: string): PatinaTestMark[] {
  const lines = src.split('\n');
  const out: PatinaTestMark[] = [];
  for (let i = 0; i < lines.length; i++) {
    if (!TEST_ATTR.test(lines[i])) continue;
    for (let j = i; j < Math.min(lines.length, i + 8); j++) {
      const m = FN.exec(lines[j]);
      if (m) {
        out.push({ line: i + 1, name: m[1] });
        break;
      }
    }
  }
  return out;
}

/** La cartella della mod a cui appartiene un file sotto `script/`, o `null`. */
export function patinaModDirOf(path: string): string | null {
  const p = path.replace(/\\/g, '/');
  const i = p.lastIndexOf('/script/');
  return i > 0 ? p.slice(0, i) : null;
}

/**
 * Quale marchio porta «i test», in un progetto che può essere Java o Rust.
 *
 * Sembra un dettaglio e non lo è: il mark di **JUnit 5** su un workspace Cargo dice una
 * cosa falsa — che quei test siano JUnit — nel punto in cui l'utente sta guardando i propri
 * `#[test]`. Era già deciso bene in un posto (il pulsante del rail) e cablato a JUnit in
 * altri tre; sta qui perché una decisione che vive in quattro copie ne ha tre sbagliate
 * appena il progetto cambia ecosistema.
 */

import type { IconComponent } from '$lib/types/icon';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import JUnitIcon from './JUnitIcon.svelte';
import RustTestIcon from './RustTestIcon.svelte';

/** Il marchio dei test del progetto aperto. */
export function testIcon(): IconComponent {
  return (projectStore.isCargo ? RustTestIcon : JUnitIcon) as unknown as IconComponent;
}
